use crate::model::document::DocumentTab;
use crate::model::markup::MarkupKind;
use crate::pdf::{base64_decode, base64_encode};
use image::codecs::jpeg::JpegEncoder;
use lopdf::{Document, Object, ObjectId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptimizePreset {
    /// Lossless Flate recompression + orphan pruning + light image cap (2048px, Q=88).
    HighQuality,
    /// Balanced CAD/AEC compression (1600px max image side, Q=78).
    #[default]
    Balanced,
    /// Aggressive file-size reduction for email/WhatsApp sharing (1100px max image side, Q=65).
    MaxCompression,
}

impl OptimizePreset {
    pub fn label_es(self) -> &'static str {
        match self {
            Self::HighQuality => "Alta calidad (Suave)",
            Self::Balanced => "Equilibrado (Recomendado)",
            Self::MaxCompression => "Máxima compresión (Ligero)",
        }
    }

    fn params(self) -> (u32, u8) {
        match self {
            Self::HighQuality => (2048, 88),
            Self::Balanced => (1600, 78),
            Self::MaxCompression => (1100, 65),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OptimizeReport {
    pub bytes_before: usize,
    pub bytes_after: usize,
    pub streams_compressed: usize,
    pub images_reencoded: usize,
    pub objects_pruned: usize,
}

impl OptimizeReport {
    pub fn saved_percent(&self) -> f64 {
        if self.bytes_before == 0 || self.bytes_after >= self.bytes_before {
            return 0.0;
        }
        ((self.bytes_before - self.bytes_after) as f64 / self.bytes_before as f64) * 100.0
    }
}

fn encode_jpeg_rgb(rgb: &image::RgbImage, quality: u8) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut out, quality);
    enc.encode_image(rgb).ok()?;
    Some(out)
}

/// Reduces PDF file size offline (`SCmdReduceFileSize`):
/// 1. Recompresses any uncompressed PDF streams with FlateDecode.
/// 2. Downsamples & JPEG-reencodes oversized embedded RGB/JPEG XObject images when smaller.
/// 3. Compacts oversized embedded markup images and site photo attachments.
/// 4. Prunes orphaned/unreferenced PDF objects.
pub fn reduce_pdf_file_size_in_tab(
    tab: &mut DocumentTab,
    preset: OptimizePreset,
) -> Result<OptimizeReport, String> {
    let (max_img_side, jpeg_q) = preset.params();
    let bytes_before = tab.raw_pdf_bytes.len();
    let mut doc =
        Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| format!("Error lopdf: {e}"))?;

    let mut streams_compressed = 0usize;
    let mut images_reencoded = 0usize;

    let all_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for oid in all_ids {
        let Some(Object::Stream(stream)) = doc.objects.get_mut(&oid) else {
            continue;
        };

        let is_image = stream
            .dict
            .get(b"Subtype")
            .and_then(|o| o.as_name())
            .ok()
            == Some(b"Image");
        let has_soft_mask = stream.dict.has(b"SMask") || stream.dict.has(b"Mask");

        if is_image && !has_soft_mask {
            let filter_name = stream
                .dict
                .get(b"Filter")
                .and_then(|o| o.as_name())
                .ok()
                .map(|b| b.to_vec());
            let cs_name = stream
                .dict
                .get(b"ColorSpace")
                .and_then(|o| o.as_name())
                .ok()
                .map(|b| b.to_vec());
            let w = stream
                .dict
                .get(b"Width")
                .and_then(|o| o.as_i64())
                .unwrap_or(0) as u32;
            let h = stream
                .dict
                .get(b"Height")
                .and_then(|o| o.as_i64())
                .unwrap_or(0) as u32;
            let bpc = stream
                .dict
                .get(b"BitsPerComponent")
                .and_then(|o| o.as_i64())
                .unwrap_or(8);

            // Case A: DCTDecode (JPEG) image -> decode, downsample if > max_img_side, re-encode if smaller
            if filter_name.as_deref() == Some(b"DCTDecode")
                && let Ok(img) = image::load_from_memory(&stream.content)
            {
                let resized = if img.width() > max_img_side || img.height() > max_img_side {
                    img.thumbnail(max_img_side, max_img_side)
                } else {
                    img
                };
                let rgb = resized.to_rgb8();
                if let Some(new_jpg) = encode_jpeg_rgb(&rgb, jpeg_q)
                    && new_jpg.len() + 64 < stream.content.len()
                {
                    let (nw, nh) = rgb.dimensions();
                    stream.content = new_jpg;
                    stream.dict.set("Width", nw as i64);
                    stream.dict.set("Height", nh as i64);
                    stream.dict.set("ColorSpace", "DeviceRGB");
                    stream.dict.set("BitsPerComponent", 8i64);
                    stream.dict.set("Filter", "DCTDecode");
                    stream.dict.set("Length", stream.content.len() as i64);
                    images_reencoded += 1;
                    continue;
                }
            }

            // Case B: Raw / Flate DeviceRGB 8-bit image -> convert to compact JPEG if meaningfully smaller
            if cs_name.as_deref() == Some(b"DeviceRGB")
                && bpc == 8
                && w >= 64
                && h >= 64
                && (filter_name.is_none() || filter_name.as_deref() == Some(b"FlateDecode"))
            {
                let raw = stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone());
                if raw.len() == (w as usize) * (h as usize) * 3
                    && let Some(rgb_buf) = image::RgbImage::from_raw(w, h, raw)
                {
                    let dyn_img = image::DynamicImage::ImageRgb8(rgb_buf);
                    let resized = if w > max_img_side || h > max_img_side {
                        dyn_img.thumbnail(max_img_side, max_img_side)
                    } else {
                        dyn_img
                    };
                    let rgb_final = resized.to_rgb8();
                    if let Some(new_jpg) = encode_jpeg_rgb(&rgb_final, jpeg_q)
                        && new_jpg.len() + 128 < stream.content.len()
                    {
                        let (nw, nh) = rgb_final.dimensions();
                        stream.content = new_jpg;
                        stream.dict.set("Width", nw as i64);
                        stream.dict.set("Height", nh as i64);
                        stream.dict.set("Filter", "DCTDecode");
                        stream.dict.set("Length", stream.content.len() as i64);
                        images_reencoded += 1;
                        continue;
                    }
                }
            }
        }

        // Compress any uncompressed non-image stream with FlateDecode
        if !stream.dict.has(b"Filter") && stream.content.len() > 64 {
            let before_len = stream.content.len();
            let orig = stream.content.clone();
            if stream.compress().is_ok() && stream.content.len() < before_len {
                streams_compressed += 1;
            } else {
                stream.content = orig;
                stream.dict.remove(b"Filter");
                stream.dict.set("Length", stream.content.len() as i64);
            }
        }
    }

    // Compact oversized live MarkupKind::Image & site photo attachments too
    for m in &mut tab.markups {
        if let MarkupKind::Image { png_b64, .. } = &mut m.kind
            && let Some(raw) = base64_decode(png_b64)
            && let Ok(img) = image::load_from_memory(&raw)
        {
            let resized = if img.width() > max_img_side || img.height() > max_img_side {
                img.thumbnail(max_img_side, max_img_side)
            } else {
                img
            };
            let rgb = resized.to_rgb8();
            if let Some(jpg) = encode_jpeg_rgb(&rgb, jpeg_q)
                && jpg.len() + 128 < raw.len()
            {
                *png_b64 = base64_encode(&jpg).into();
                images_reencoded += 1;
            }
        }
        for att in &mut m.attachments {
            if let Some(raw) = base64_decode(&att.image_b64)
                && let Ok(img) = image::load_from_memory(&raw)
            {
                let cap = max_img_side.min(1024);
                let resized = if img.width() > cap || img.height() > cap {
                    img.thumbnail(cap, cap)
                } else {
                    img
                };
                let rgb = resized.to_rgb8();
                if let Some(jpg) = encode_jpeg_rgb(&rgb, jpeg_q)
                    && jpg.len() + 128 < raw.len()
                {
                    att.image_b64 = base64_encode(&jpg).into();
                    images_reencoded += 1;
                }
            }
        }
    }

    let pruned_ids = doc.prune_objects();
    let objects_pruned = pruned_ids.len();

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Error optimizando PDF: {e}"))?;
    let bytes_after = out.len();

    if bytes_after < bytes_before || images_reencoded > 0 || streams_compressed > 0 {
        tab.raw_pdf_bytes = out.into();
        tab.dirty = true;
    }

    Ok(OptimizeReport {
        bytes_before,
        bytes_after: tab.raw_pdf_bytes.len(),
        streams_compressed,
        images_reencoded,
        objects_pruned,
    })
}
