use crate::model::document::DocumentTab;
use crate::model::markup::Markup;
use crate::model::settings::CsvDelimiter;
use crate::model::units::{format_area, format_length, format_volume};
use std::path::Path;

fn csv_escape(field: &str, delim: char) -> String {
    if field.contains(delim) || field.contains('"') || field.contains('\n') || field.contains('\r')
    {
        let escaped = field.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        field.to_string()
    }
}

/// Exports the Markups List (Lista de Marcas) to UTF-8 with BOM CSV, ready for Excel in Chile.
pub fn export_markups_csv(tab: &DocumentTab, delimiter: CsvDelimiter) -> String {
    let d = delimiter.as_char();
    let mut out = String::from("\u{FEFF}");
    let headers = [
        "Tipo",
        "Asunto",
        "Etiqueta",
        "Página",
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
        "Fecha modificación",
    ];
    out.push_str(&headers.join(&d.to_string()));
    out.push('\n');

    let mut sum_len_m = 0.0;
    let mut sum_area_m2 = 0.0;
    let mut sum_vol_m3 = 0.0;
    let mut sum_perim_m = 0.0;
    let mut sum_count = 0usize;
    let mut sum_cost = 0.0;

    for m in &tab.markups {
        let Some(page) = tab.pages.get(m.page_index) else {
            continue;
        };
        let metrics = m.compute_metrics(&page.scale, &page.viewports);
        let len_s = metrics
            .length_m
            .map(|v| {
                sum_len_m += v;
                format_length(v, page.scale.display_unit, page.scale.precision)
            })
            .unwrap_or_default();
        let area_s = metrics
            .area_m2
            .map(|v| {
                sum_area_m2 += v;
                format_area(v, page.scale.area_unit, page.scale.precision)
            })
            .unwrap_or_default();
        let vol_s = metrics
            .volume_m3
            .map(|v| {
                sum_vol_m3 += v;
                format_volume(v, page.scale.volume_unit, page.scale.precision)
            })
            .unwrap_or_default();
        let perim_s = metrics
            .perimeter_m
            .map(|v| {
                sum_perim_m += v;
                format_length(v, page.scale.display_unit, page.scale.precision)
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

        let row = [
            csv_escape(m.type_name_es(), d),
            csv_escape(&m.subject, d),
            csv_escape(&m.label, d),
            (m.page_index + 1).to_string(),
            csv_escape(&m.author, d),
            csv_escape(m.status.label(), d),
            csv_escape(&m.layer, d),
            csv_escape(&m.comment, d),
            csv_escape(&len_s, d),
            csv_escape(&area_s, d),
            csv_escape(&vol_s, d),
            csv_escape(&perim_s, d),
            count_s,
            csv_escape(&ang_s, d),
            csv_escape(&unit_cost_s, d),
            csv_escape(&total_cost_s, d),
            m.modified_at.format("%Y-%m-%d %H:%M").to_string(),
        ];
        out.push_str(&row.join(&d.to_string()));
        out.push('\n');
    }

    if let Some(first_page) = tab.pages.first() {
        let s = &first_page.scale;
        let tot_row = [
            "TOTALES".to_string(),
            format!("{} marcas", tab.markups.len()),
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
            String::new(),
        ];
        out.push_str(&tot_row.join(&d.to_string()));
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
    std::fs::write(path, csv.as_bytes())
        .map_err(|e| format!("Error al guardar CSV {}: {e}", path.display()))
}

pub fn export_markups_json_file(tab: &DocumentTab, path: &Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&tab.markups).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("Error al guardar JSON {}: {e}", path.display()))
}

pub fn import_markups_json_file(tab: &mut DocumentTab, path: &Path) -> Result<usize, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
    let imported: Vec<Markup> =
        serde_json::from_str(&raw).map_err(|e| format!("JSON inválido: {e}"))?;
    let count = imported.len();
    tab.push_undo();
    for mut m in imported {
        m.page_index = m.page_index.min(tab.pages.len().saturating_sub(1));
        tab.ensure_layer(&m.layer);
        tab.markups.push(m);
    }
    Ok(count)
}
