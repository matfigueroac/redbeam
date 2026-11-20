use crate::geometry::math::{Point2, Rect2};
use crate::geometry::snap::SnapIndex;
use hayro::hayro_interpret::font::FontQuery;
use hayro::hayro_interpret::hayro_cmap::CidFamily;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{Affine, BezPath, PathEl, Rect};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{PixmapSettings, RenderCache, RenderSettings, render, render_into};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RgbaImageData {
    pub width: usize,
    pub height: usize,
    /// Premultiplied RGBA8 bytes ready for `egui::ColorImage::from_rgba_premultiplied`.
    pub premul_rgba: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum RenderCommand {
    LoadDocument {
        doc_id: Uuid,
        clean_pdf_bytes: Arc<Vec<u8>>,
    },
    CloseDocument {
        doc_id: Uuid,
    },
    RenderThumbnail {
        doc_id: Uuid,
        page_index: usize,
        max_side_px: u32,
    },
    RenderBasePage {
        doc_id: Uuid,
        page_index: usize,
        max_side_px: u32,
        generation: u64,
    },
    RenderTile {
        doc_id: Uuid,
        page_index: usize,
        crop_rect_pt: Rect2,
        out_w: u16,
        out_h: u16,
        generation: u64,
    },
    ExtractSnap {
        doc_id: Uuid,
        page_index: usize,
    },
    ExtractSearchText {
        doc_id: Uuid,
        raw_pdf_bytes: Arc<Vec<u8>>,
    },
}

#[derive(Debug)]
pub enum RenderResult {
    ThumbnailReady {
        doc_id: Uuid,
        page_index: usize,
        image: RgbaImageData,
    },
    BasePageReady {
        doc_id: Uuid,
        page_index: usize,
        generation: u64,
        image: RgbaImageData,
    },
    TileReady {
        doc_id: Uuid,
        page_index: usize,
        crop_rect_pt: Rect2,
        generation: u64,
        image: RgbaImageData,
    },
    SnapReady {
        doc_id: Uuid,
        page_index: usize,
        index: SnapIndex,
    },
    SearchTextReady {
        doc_id: Uuid,
        text_by_page: Vec<String>,
    },
    Error {
        doc_id: Uuid,
        message: String,
    },
}

pub struct PdfRenderWorker {
    fast_tx: Sender<RenderCommand>,
    bg_tx: Sender<RenderCommand>,
    rx: Receiver<RenderResult>,
}

struct CachedDoc {
    pdf: Pdf,
}

impl PdfRenderWorker {
    pub fn spawn(repaint_cb: impl Fn() + Send + Sync + 'static) -> Self {
        let (fast_tx, fast_rx) = channel::<RenderCommand>();
        let (bg_tx, bg_rx) = channel::<RenderCommand>();
        let (res_tx, res_rx) = channel::<RenderResult>();
        let repaint_cb = Arc::new(repaint_cb);

        // Priority Lane 1: Active Base Page & Zoom Tiles (never blocked by thumbnails or text/snap!)
        {
            let res_tx = res_tx.clone();
            let repaint_cb = repaint_cb.clone();
            thread::Builder::new()
                .name("redbim-pdf-fast".to_string())
                .spawn(move || {
                    let mut docs: HashMap<Uuid, CachedDoc> = HashMap::new();
                    let interp_settings = default_interpreter_settings();
                    let render_settings = RenderSettings::default();

                    while let Ok(first) = fast_rx.recv() {
                        let mut batch = vec![first];
                        while let Ok(more) = fast_rx.try_recv() {
                            batch.push(more);
                        }

                        let mut latest_base: HashMap<(Uuid, usize), (u32, u64)> = HashMap::new();
                        let mut latest_tile: HashMap<(Uuid, usize), (Rect2, u16, u16, u64)> =
                            HashMap::new();

                        for cmd in batch {
                            match cmd {
                                RenderCommand::LoadDocument {
                                    doc_id,
                                    clean_pdf_bytes,
                                } => {
                                    if let Ok(pdf) = Pdf::new(clean_pdf_bytes) {
                                        docs.insert(doc_id, CachedDoc { pdf });
                                    }
                                }
                                RenderCommand::CloseDocument { doc_id } => {
                                    docs.remove(&doc_id);
                                    latest_base.retain(|(d, _), _| *d != doc_id);
                                    latest_tile.retain(|(d, _), _| *d != doc_id);
                                }
                                RenderCommand::RenderBasePage {
                                    doc_id,
                                    page_index,
                                    max_side_px,
                                    generation,
                                } => {
                                    latest_base
                                        .insert((doc_id, page_index), (max_side_px, generation));
                                }
                                RenderCommand::RenderTile {
                                    doc_id,
                                    page_index,
                                    crop_rect_pt,
                                    out_w,
                                    out_h,
                                    generation,
                                } => {
                                    latest_tile.insert(
                                        (doc_id, page_index),
                                        (crop_rect_pt, out_w, out_h, generation),
                                    );
                                }
                                _ => {}
                            }
                        }

                        for ((doc_id, page_index), (max_side_px, generation)) in latest_base {
                            if let Some(cached) = docs.get(&doc_id) {
                                match render_page_from_pdf(
                                    &cached.pdf,
                                    page_index,
                                    max_side_px,
                                    &interp_settings,
                                    &render_settings,
                                ) {
                                    Ok(image) => {
                                        let _ = res_tx.send(RenderResult::BasePageReady {
                                            doc_id,
                                            page_index,
                                            generation,
                                            image,
                                        });
                                        repaint_cb();
                                    }
                                    Err(message) => {
                                        let _ = res_tx.send(RenderResult::Error { doc_id, message });
                                    }
                                }
                            }
                        }

                        for ((doc_id, page_index), (crop_rect_pt, out_w, out_h, generation)) in
                            latest_tile
                        {
                            if let Some(cached) = docs.get(&doc_id) {
                                match render_tile_from_pdf(
                                    &cached.pdf,
                                    page_index,
                                    crop_rect_pt,
                                    out_w,
                                    out_h,
                                    &interp_settings,
                                    &render_settings,
                                ) {
                                    Ok(image) => {
                                        let _ = res_tx.send(RenderResult::TileReady {
                                            doc_id,
                                            page_index,
                                            crop_rect_pt,
                                            generation,
                                            image,
                                        });
                                        repaint_cb();
                                    }
                                    Err(message) => {
                                        let _ = res_tx.send(RenderResult::Error { doc_id, message });
                                    }
                                }
                            }
                        }
                    }
                })
                .expect("spawn pdf fast render worker");
        }

        // Background Lane 2: Thumbnails, Snap extraction, & Searchable text indexing
        thread::Builder::new()
            .name("redbim-pdf-bg".to_string())
            .spawn(move || {
                let mut docs: HashMap<Uuid, CachedDoc> = HashMap::new();
                let interp_settings = default_interpreter_settings();
                let render_settings = RenderSettings::default();

                while let Ok(first) = bg_rx.recv() {
                    let mut cmds = vec![first];
                    while let Ok(more) = bg_rx.try_recv() {
                        cmds.push(more);
                    }
                    // Apply lifecycle commands first so closed docs are skipped immediately
                    let mut work = Vec::with_capacity(cmds.len());
                    for cmd in cmds {
                        match cmd {
                            RenderCommand::LoadDocument {
                                doc_id,
                                clean_pdf_bytes,
                            } => {
                                if let Ok(pdf) = Pdf::new(clean_pdf_bytes) {
                                    docs.insert(doc_id, CachedDoc { pdf });
                                }
                            }
                            RenderCommand::CloseDocument { doc_id } => {
                                docs.remove(&doc_id);
                            }
                            other => work.push(other),
                        }
                    }

                    for cmd in work {
                        match cmd {
                            RenderCommand::RenderThumbnail {
                                doc_id,
                                page_index,
                                max_side_px,
                            } => {
                                if let Some(cached) = docs.get(&doc_id)
                                    && let Ok(image) = render_page_from_pdf(
                                        &cached.pdf,
                                        page_index,
                                        max_side_px,
                                        &interp_settings,
                                        &render_settings,
                                    )
                                {
                                    let _ = res_tx.send(RenderResult::ThumbnailReady {
                                        doc_id,
                                        page_index,
                                        image,
                                    });
                                    repaint_cb();
                                }
                            }
                            RenderCommand::ExtractSnap { doc_id, page_index } => {
                                if let Some(cached) = docs.get(&doc_id)
                                    && let Ok(index) = extract_snap_from_pdf(
                                        &cached.pdf,
                                        page_index,
                                        &interp_settings,
                                    )
                                {
                                    let _ = res_tx.send(RenderResult::SnapReady {
                                        doc_id,
                                        page_index,
                                        index,
                                    });
                                    repaint_cb();
                                }
                            }
                            RenderCommand::ExtractSearchText {
                                doc_id,
                                raw_pdf_bytes,
                            } if docs.contains_key(&doc_id) => {
                                let text_by_page = extract_search_text_by_page(&raw_pdf_bytes);
                                let _ = res_tx.send(RenderResult::SearchTextReady {
                                    doc_id,
                                    text_by_page,
                                });
                                repaint_cb();
                            }
                            _ => {}
                        }
                    }
                }
            })
            .expect("spawn pdf bg worker");

        Self {
            fast_tx,
            bg_tx,
            rx: res_rx,
        }
    }

    pub fn send(&self, cmd: RenderCommand) {
        match &cmd {
            RenderCommand::LoadDocument { .. } | RenderCommand::CloseDocument { .. } => {
                let _ = self.fast_tx.send(cmd.clone());
                let _ = self.bg_tx.send(cmd);
            }
            RenderCommand::RenderBasePage { .. } | RenderCommand::RenderTile { .. } => {
                let _ = self.fast_tx.send(cmd);
            }
            RenderCommand::RenderThumbnail { .. }
            | RenderCommand::ExtractSnap { .. }
            | RenderCommand::ExtractSearchText { .. } => {
                let _ = self.bg_tx.send(cmd);
            }
        }
    }

    pub fn drain_results(&self) -> Vec<RenderResult> {
        let mut out = Vec::new();
        while let Ok(r) = self.rx.try_recv() {
            out.push(r);
        }
        out
    }
}

fn extract_search_text_by_page(raw_bytes: &[u8]) -> Vec<String> {
    let Ok(doc) = lopdf::Document::load_mem(raw_bytes) else {
        return Vec::new();
    };
    let page_map = doc.get_pages();
    let mut out = vec![String::new(); page_map.len()];
    for &num in page_map.keys() {
        let idx = num.saturating_sub(1) as usize;
        if idx < out.len()
            && let Ok(txt) = doc.extract_text(&[num])
        {
            out[idx] = txt;
        }
    }
    out
}

pub fn default_interpreter_settings() -> InterpreterSettings {
    InterpreterSettings {
        font_resolver: Arc::new(|query| match query {
            FontQuery::Standard(s) => Some(s.get_font_data()),
            FontQuery::Fallback(f) => {
                let _ = CidFamily::AdobeGB1;
                Some(f.pick_standard_font().get_font_data())
            }
        }),
        ..Default::default()
    }
}

pub fn render_page_rgba(
    pdf_bytes: &[u8],
    page_index: usize,
    max_side_px: u32,
    interp_settings: &InterpreterSettings,
    render_settings: &RenderSettings,
) -> Result<RgbaImageData, String> {
    let pdf = Pdf::new(pdf_bytes.to_vec()).map_err(|e| format!("PDF inválido: {e:?}"))?;
    render_page_from_pdf(
        &pdf,
        page_index,
        max_side_px,
        interp_settings,
        render_settings,
    )
}

fn render_page_from_pdf(
    pdf: &Pdf,
    page_index: usize,
    max_side_px: u32,
    interp_settings: &InterpreterSettings,
    render_settings: &RenderSettings,
) -> Result<RgbaImageData, String> {
    let cache = RenderCache::new();
    let pages = pdf.pages();
    let page = pages
        .get(page_index)
        .ok_or_else(|| format!("Página {page_index} fuera de rango"))?;

    let (w, h) = page.render_dimensions();
    let max_dim = w.max(h).max(1.0);
    let scale = (max_side_px as f32 / max_dim).clamp(0.05, 4.0);
    let max_safe = (u16::MAX as f32 - 4.0) / max_dim;
    let scale = scale.min(max_safe);

    let pixmap = render(
        page,
        &cache,
        interp_settings,
        render_settings,
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );

    Ok(RgbaImageData {
        width: pixmap.width() as usize,
        height: pixmap.height() as usize,
        premul_rgba: pixmap.data_as_u8_slice().to_vec(),
    })
}

pub fn render_tile_rgba(
    pdf_bytes: &[u8],
    page_index: usize,
    crop_rect_pt: Rect2,
    out_w: u16,
    out_h: u16,
    interp_settings: &InterpreterSettings,
    render_settings: &RenderSettings,
) -> Result<RgbaImageData, String> {
    let pdf = Pdf::new(pdf_bytes.to_vec()).map_err(|e| format!("PDF inválido: {e:?}"))?;
    render_tile_from_pdf(
        &pdf,
        page_index,
        crop_rect_pt,
        out_w,
        out_h,
        interp_settings,
        render_settings,
    )
}

fn render_tile_from_pdf(
    pdf: &Pdf,
    page_index: usize,
    crop_rect_pt: Rect2,
    out_w: u16,
    out_h: u16,
    interp_settings: &InterpreterSettings,
    render_settings: &RenderSettings,
) -> Result<RgbaImageData, String> {
    if out_w < 8 || out_h < 8 || crop_rect_pt.width() <= 0.1 || crop_rect_pt.height() <= 0.1 {
        return Err("Región de tile inválida".to_string());
    }
    let cache = RenderCache::new();
    let pages = pdf.pages();
    let page = pages
        .get(page_index)
        .ok_or_else(|| format!("Página {page_index} fuera de rango"))?;

    let sx = out_w as f64 / crop_rect_pt.width();
    let sy = out_h as f64 / crop_rect_pt.height();
    let transform = Affine::scale_non_uniform(sx, sy)
        * Affine::translate((-crop_rect_pt.min_x, -crop_rect_pt.min_y))
        * page.initial_transform(true).to_kurbo();

    let mut ctx = RenderContext::new(out_w, out_h);
    render_into(page, &cache, interp_settings, render_settings, &mut ctx, transform);
    ctx.flush();

    let mut pixmap = Pixmap::new(out_w, out_h);
    ctx.render_with(
        &mut pixmap,
        &mut Resources::default(),
        RasterizerSettings {
            target_init: TargetInit::Clear(WHITE),
            ..Default::default()
        },
    );

    Ok(RgbaImageData {
        width: pixmap.width() as usize,
        height: pixmap.height() as usize,
        premul_rgba: pixmap.data_as_u8_slice().to_vec(),
    })
}

struct SnapDevice {
    index: SnapIndex,
}

impl SnapDevice {
    #[inline]
    fn xform(xf: Affine, p: hayro::kurbo::Point) -> Point2 {
        let q = xf * p;
        Point2::new(q.x, q.y)
    }
}

impl Device<'_> for SnapDevice {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'_>, _draw_mode: &DrawMode) {
        let xf = props.transform;
        let mut start_pt: Option<Point2> = None;
        let mut cur_pt: Option<Point2> = None;
        for el in path.elements() {
            match *el {
                PathEl::MoveTo(p) => {
                    let pt = Self::xform(xf, p);
                    start_pt = Some(pt);
                    cur_pt = Some(pt);
                }
                PathEl::LineTo(p) => {
                    let pt = Self::xform(xf, p);
                    if let Some(prev) = cur_pt {
                        self.index.insert_segment(prev, pt);
                    }
                    cur_pt = Some(pt);
                }
                PathEl::QuadTo(p1, p2) => {
                    let pt1 = Self::xform(xf, p1);
                    let pt2 = Self::xform(xf, p2);
                    if let Some(prev) = cur_pt {
                        self.index.insert_segment(prev, pt1.midpoint(pt2));
                        self.index.insert_segment(pt1.midpoint(pt2), pt2);
                    }
                    cur_pt = Some(pt2);
                }
                PathEl::CurveTo(p1, p2, p3) => {
                    let pt1 = Self::xform(xf, p1);
                    let pt2 = Self::xform(xf, p2);
                    let pt3 = Self::xform(xf, p3);
                    if let Some(prev) = cur_pt {
                        let mid = Point2::new(
                            (prev.x + 3.0 * pt1.x + 3.0 * pt2.x + pt3.x) / 8.0,
                            (prev.y + 3.0 * pt1.y + 3.0 * pt2.y + pt3.y) / 8.0,
                        );
                        self.index.insert_segment(prev, mid);
                        self.index.insert_segment(mid, pt3);
                    }
                    cur_pt = Some(pt3);
                }
                PathEl::ClosePath => {
                    if let (Some(prev), Some(st)) = (cur_pt, start_pt) {
                        self.index.insert_segment(prev, st);
                    }
                    cur_pt = start_pt;
                }
            }
        }
    }

    fn draw_rect(&mut self, rect: &Rect, props: DrawProps<'_>, _draw_mode: &DrawMode) {
        let xf = props.transform;
        let p0 = Self::xform(xf, hayro::kurbo::Point::new(rect.x0, rect.y0));
        let p1 = Self::xform(xf, hayro::kurbo::Point::new(rect.x1, rect.y0));
        let p2 = Self::xform(xf, hayro::kurbo::Point::new(rect.x1, rect.y1));
        let p3 = Self::xform(xf, hayro::kurbo::Point::new(rect.x0, rect.y1));
        self.index.insert_segment(p0, p1);
        self.index.insert_segment(p1, p2);
        self.index.insert_segment(p2, p3);
        self.index.insert_segment(p3, p0);
    }

    fn push_clip_path(&mut self, _clip_path: &ClipPath) {}
    fn push_transparency_group(
        &mut self,
        _opacity: f32,
        _mask: Option<SoftMask<'_>>,
        _blend_mode: BlendMode,
    ) {
    }
    fn draw_glyph_run(
        &mut self,
        _glyph_run: &hayro::hayro_interpret::font::GlyphRun<'_, '_>,
        _props: DrawProps<'_>,
        _draw_mode: &DrawMode,
    ) {
    }
    fn draw_image(&mut self, _image: Image<'_, '_>, _props: ImageDrawProps<'_>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

pub fn extract_snap_index(
    pdf_bytes: &[u8],
    page_index: usize,
    interp_settings: &InterpreterSettings,
) -> Result<SnapIndex, String> {
    let pdf = Pdf::new(pdf_bytes.to_vec()).map_err(|e| format!("PDF inválido: {e:?}"))?;
    extract_snap_from_pdf(&pdf, page_index, interp_settings)
}

fn extract_snap_from_pdf(
    pdf: &Pdf,
    page_index: usize,
    interp_settings: &InterpreterSettings,
) -> Result<SnapIndex, String> {
    let pages = pdf.pages();
    let page = pages
        .get(page_index)
        .ok_or_else(|| format!("Página {page_index} fuera de rango"))?;

    let (w, h) = page.render_dimensions();
    let cache = InterpreterCache::new();
    let transform = page.initial_transform(true).to_kurbo();
    let target = Rect::new(0.0, 0.0, w as f64, h as f64);
    let mut ctx = Context::new(
        transform,
        target,
        &cache,
        page.xref(),
        interp_settings.clone(),
    );
    let mut dev = SnapDevice {
        index: SnapIndex::new(16.0),
    };
    interpret_page(page, &mut ctx, &mut dev);
    Ok(dev.index)
}
