use crate::model::custom_columns::{CustomColumnDef, evaluate_column_cell};
use crate::model::document::DocumentTab;
use crate::model::markup::Markup;
use crate::model::scale::resolve_scale_at;
use crate::model::settings::CsvDelimiter;
use crate::model::units::{format_area, format_length, format_volume};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

fn csv_escape(field: &str, delim: char) -> String {
    if field.contains(delim) || field.contains('"') || field.contains('\n') || field.contains('\r')
    {
        let escaped = field.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        field.to_string()
    }
}

/// Neutralizes spreadsheet formula injection (`=`, `+`, `-`, `@`, `\t`, `\r`) on user text columns.
fn csv_escape_text(field: &str, delim: char) -> String {
    let trimmed = field.trim_start();
    let needs_prefix = trimmed.starts_with(['=', '+', '-', '@'])
        || field.starts_with(['\t', '\r']);
    if needs_prefix {
        let safe = format!("'{field}");
        csv_escape(&safe, delim)
    } else {
        csv_escape(field, delim)
    }
}

/// Exports the Markups List (Lista de Marcas) to UTF-8 with BOM CSV, ready for Excel in Chile.
/// Includes Page Labels, enclosing Spaces, measurements, costs, and all visible Custom Columns (incl. Formulas).
pub fn export_markups_csv(tab: &DocumentTab, delimiter: CsvDelimiter) -> String {
    let d = delimiter.as_char();
    let d_str = d.to_string();
    let mut out = String::from("\u{FEFF}");
    let visible_custom: Vec<&CustomColumnDef> =
        tab.custom_columns.iter().filter(|c| c.visible).collect();

    let mut headers: Vec<String> = vec![
        "Tipo",
        "Asunto",
        "Etiqueta",
        "Página",
        "Lámina",
        "Espacio",
        "Autor",
        "Estado",
        "Capa",
        "Comentarios",
        "Longitud",
        "Área",
        "Volumen",
        "Perímetro",
        "Conteo",
        "Ángulo / Pendiente",
        "Precio Unitario",
        "Total ($)",
        "Fotos",
        "Fecha modificación",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    for col in &visible_custom {
        headers.push(csv_escape_text(&col.name, d));
    }
    out.push_str(&headers.join(&d_str));
    out.push('\n');

    let mut sum_len_m = 0.0;
    let mut sum_area_m2 = 0.0;
    let mut sum_vol_m3 = 0.0;
    let mut sum_perim_m = 0.0;
    let mut sum_count = 0usize;
    let mut sum_cost = 0.0;
    let mut custom_totals: Vec<f64> = vec![0.0; visible_custom.len()];

    for m in &tab.markups {
        let Some(page) = tab.pages.get(m.page_index) else {
            continue;
        };
        let metrics = m.compute_metrics(&page.scale, &page.viewports);
        let eff_scale = resolve_scale_at(&page.scale, &page.viewports, m.anchor_point());
        let opts = m.measurement_opts();
        let len_unit = opts.and_then(|o| o.length_unit_override).unwrap_or(eff_scale.display_unit);
        let area_unit = opts.and_then(|o| o.area_unit_override).unwrap_or(eff_scale.area_unit);
        let vol_unit = opts.and_then(|o| o.volume_unit_override).unwrap_or(eff_scale.volume_unit);
        let prec = opts.and_then(|o| o.precision_override).unwrap_or(eff_scale.precision);

        let len_s = metrics
            .length_m
            .map(|v| {
                sum_len_m += v;
                format_length(v, len_unit, prec)
            })
            .unwrap_or_default();
        let area_s = metrics
            .area_m2
            .map(|v| {
                sum_area_m2 += v;
                format_area(v, area_unit, prec)
            })
            .unwrap_or_default();
        let vol_s = metrics
            .volume_m3
            .map(|v| {
                sum_vol_m3 += v;
                format_volume(v, vol_unit, prec)
            })
            .unwrap_or_default();
        let perim_s = metrics
            .perimeter_m
            .map(|v| {
                sum_perim_m += v;
                format_length(v, len_unit, prec)
            })
            .unwrap_or_default();
        let count_s = metrics
            .count
            .map(|c| {
                sum_count += c;
                c.to_string()
            })
            .unwrap_or_default();
        let ang_s = metrics
            .angle_deg
            .map(|deg| format!("{deg:.1}°"))
            .unwrap_or_default();
        let unit_cost_s = m
            .unit_cost
            .map(|u| format!("{u:.2}"))
            .unwrap_or_default();
        let total_cost_s = m
            .total_cost(&page.scale, &page.viewports)
            .map(|c| {
                sum_cost += c;
                format!("{c:.0}")
            })
            .unwrap_or_default();
        let space_s = tab.resolve_space_for_markup(m).unwrap_or_default();

        let mut row = vec![
            csv_escape_text(m.type_name_es(), d),
            csv_escape_text(&m.subject, d),
            csv_escape_text(&m.label, d),
            (m.page_index + 1).to_string(),
            csv_escape_text(&page.display_label(), d),
            csv_escape_text(&space_s, d),
            csv_escape_text(&m.author, d),
            csv_escape_text(m.status.label(), d),
            csv_escape_text(&m.layer, d),
            csv_escape_text(&m.comment, d),
            csv_escape(&len_s, d),
            csv_escape(&area_s, d),
            csv_escape(&vol_s, d),
            csv_escape(&perim_s, d),
            count_s,
            csv_escape(&ang_s, d),
            csv_escape(&unit_cost_s, d),
            csv_escape(&total_cost_s, d),
            if m.attachments.is_empty() {
                String::new()
            } else {
                m.attachments.len().to_string()
            },
            m.modified_at.format("%Y-%m-%d %H:%M").to_string(),
        ];
        for (ci, col) in visible_custom.iter().enumerate() {
            let cell = evaluate_column_cell(
                col,
                &tab.custom_columns,
                m,
                &metrics,
                &page.scale,
                &page.viewports,
            );
            if col.kind.include_in_totals()
                && let Some(num) = cell.numeric
                && num.is_finite()
            {
                custom_totals[ci] += num;
            }
            row.push(csv_escape_text(&cell.display, d));
        }
        out.push_str(&row.join(&d_str));
        out.push('\n');
    }

    if let Some(first_page) = tab.pages.first() {
        let s = &first_page.scale;
        let mut tot_row = vec![
            "TOTALES".to_string(),
            format!("{} marcas", tab.markups.len()),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            csv_escape(&format_length(sum_len_m, s.display_unit, s.precision), d),
            csv_escape(&format_area(sum_area_m2, s.area_unit, s.precision), d),
            csv_escape(&format_volume(sum_vol_m3, s.volume_unit, s.precision), d),
            csv_escape(&format_length(sum_perim_m, s.display_unit, s.precision), d),
            sum_count.to_string(),
            String::new(),
            String::new(),
            format!("{sum_cost:.0}"),
            {
                let total_photos: usize = tab.markups.iter().map(|m| m.attachments.len()).sum();
                if total_photos > 0 {
                    total_photos.to_string()
                } else {
                    String::new()
                }
            },
            String::new(),
        ];
        for (ci, col) in visible_custom.iter().enumerate() {
            if col.kind.include_in_totals() {
                let dec = match &col.kind {
                    crate::model::custom_columns::CustomColumnKind::Number { decimals, .. }
                    | crate::model::custom_columns::CustomColumnKind::Formula { decimals, .. } => {
                        *decimals
                    }
                    _ => 2,
                };
                tot_row.push(format!("{:.dec$}", custom_totals[ci]));
            } else {
                tot_row.push(String::new());
            }
        }
        out.push_str(&tot_row.join(&d_str));
        out.push('\n');
    }

    out
}

pub fn export_markups_csv_file(
    tab: &DocumentTab,
    path: &Path,
    delimiter: CsvDelimiter,
) -> Result<(), String> {
    let csv = export_markups_csv(tab, delimiter);
    crate::storage::atomic_write_file(path, csv.as_bytes())
}

pub fn export_markups_json_file(tab: &DocumentTab, path: &Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&tab.markups).map_err(|e| e.to_string())?;
    crate::storage::atomic_write_file(path, json.as_bytes())
}

pub fn import_markups_json_file(tab: &mut DocumentTab, path: &Path) -> Result<usize, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
    let imported: Vec<Markup> =
        serde_json::from_str(&raw).map_err(|e| format!("JSON inválido: {e}"))?;
    let count = imported.len();
    if count == 0 {
        return Ok(0);
    }
    tab.push_undo();
    let mut group_remap: HashMap<Uuid, Uuid> = HashMap::new();
    for mut m in imported {
        m.id = Uuid::new_v4();
        if let Some(gid) = m.group_id {
            let new_gid = *group_remap.entry(gid).or_insert_with(Uuid::new_v4);
            m.group_id = Some(new_gid);
        }
        m.page_index = m.page_index.min(tab.pages.len().saturating_sub(1));
        tab.ensure_layer(&m.layer);
        tab.markups.push(m);
    }
    Ok(count)
}
