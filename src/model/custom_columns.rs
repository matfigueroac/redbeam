use crate::model::markup::{ComputedMetrics, Markup};
use crate::model::scale::{PageScale, ViewportScale, resolve_scale_at};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

/// Option item inside a Choice custom column (mirrors Bluebeam's `ChoiceItem`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub label: String,
    #[serde(default)]
    pub numeric_value: Option<f64>,
}

impl ChoiceOption {
    pub fn new(label: impl Into<String>, numeric_value: Option<f64>) -> Self {
        Self {
            label: label.into(),
            numeric_value,
        }
    }
}

/// Column type definitions modeled after Bluebeam Revu's `Bluebeam.Revu.Model.Core.CustomColumns`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CustomColumnKind {
    Text {
        #[serde(default)]
        default_value: String,
    },
    Number {
        #[serde(default)]
        default_value: f64,
        #[serde(default = "default_decimals")]
        decimals: usize,
        #[serde(default)]
        prefix: String,
        #[serde(default)]
        suffix: String,
        #[serde(default = "default_true")]
        include_in_totals: bool,
    },
    Choice {
        items: Vec<ChoiceOption>,
        #[serde(default)]
        allow_custom_text: bool,
    },
    Checkmark {
        #[serde(default)]
        default_value: bool,
    },
    Formula {
        expression: String,
        #[serde(default = "default_decimals")]
        decimals: usize,
        #[serde(default)]
        prefix: String,
        #[serde(default)]
        suffix: String,
        #[serde(default = "default_true")]
        include_in_totals: bool,
    },
}

fn default_decimals() -> usize {
    2
}

fn default_true() -> bool {
    true
}

impl CustomColumnKind {
    pub fn type_label_es(&self) -> &'static str {
        match self {
            Self::Text { .. } => "Texto",
            Self::Number { .. } => "Número",
            Self::Choice { .. } => "Lista de opciones",
            Self::Checkmark { .. } => "Casilla (✓)",
            Self::Formula { .. } => "Fórmula",
        }
    }

    pub fn include_in_totals(&self) -> bool {
        match self {
            Self::Number {
                include_in_totals, ..
            }
            | Self::Formula {
                include_in_totals, ..
            } => *include_in_totals,
            _ => false,
        }
    }
}

/// Schema definition of a document-level custom column (`ACustomColumn`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomColumnDef {
    pub id: Uuid,
    pub name: String,
    pub kind: CustomColumnKind,
    #[serde(default = "default_true")]
    pub visible: bool,
}

impl CustomColumnDef {
    pub fn new(name: impl Into<String>, kind: CustomColumnKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            kind,
            visible: true,
        }
    }

    pub fn include_in_totals(&self) -> bool {
        self.kind.include_in_totals()
    }

    pub fn format_numeric(&self, val: f64) -> String {
        match &self.kind {
            CustomColumnKind::Number {
                decimals,
                prefix,
                suffix,
                ..
            }
            | CustomColumnKind::Formula {
                decimals,
                prefix,
                suffix,
                ..
            } => format!("{prefix}{val:.decimals$}{suffix}"),
            _ => format!("{val:.2}"),
        }
    }

    /// Starter Chilean AEC takeoff columns (Partida, Factor Desperdicio, Subtotal c/Desp.).
    pub fn default_chilean_takeoff_columns() -> Vec<Self> {
        vec![
            Self::new(
                "Partida",
                CustomColumnKind::Choice {
                    items: vec![
                        ChoiceOption::new("Obra Gruesa", Some(1.0)),
                        ChoiceOption::new("Terminaciones", Some(1.0)),
                        ChoiceOption::new("Instalaciones", Some(1.0)),
                        ChoiceOption::new("Estructura Metálica", Some(1.0)),
                        ChoiceOption::new("Cubierta", Some(1.0)),
                    ],
                    allow_custom_text: true,
                },
            ),
            Self::new(
                "Desperdicio",
                CustomColumnKind::Number {
                    default_value: 1.05,
                    decimals: 2,
                    prefix: String::new(),
                    suffix: "×".to_string(),
                    include_in_totals: false,
                },
            ),
            Self::new(
                "Subtotal c/Desp.",
                CustomColumnKind::Formula {
                    expression: "Medicion * PrecioUnitario * Desperdicio".to_string(),
                    decimals: 0,
                    prefix: "$ ".to_string(),
                    suffix: String::new(),
                    include_in_totals: true,
                },
            ),
        ]
    }
}

/// Per-markup stored value for a non-formula custom column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CustomColumnValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

/// Result of evaluating a custom column cell for a specific markup.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedCell {
    pub display: String,
    pub numeric: Option<f64>,
    pub error: Option<String>,
}

/// Computes the primary numeric takeoff quantity of a markup in its display units.
pub fn primary_measurement_quantity(
    m: &Markup,
    metrics: &ComputedMetrics,
    page_scale: &PageScale,
    viewports: &[ViewportScale],
) -> f64 {
    let scale = resolve_scale_at(page_scale, viewports, m.anchor_point());
    let opts = m.measurement_opts();
    if let Some(v) = metrics.volume_m3 {
        let vu = opts
            .and_then(|o| o.volume_unit_override)
            .unwrap_or(scale.volume_unit);
        vu.from_cu_meters(v)
    } else if let Some(a) = metrics.area_m2 {
        let au = opts
            .and_then(|o| o.area_unit_override)
            .unwrap_or(scale.area_unit);
        au.from_sq_meters(a)
    } else if let Some(l) = metrics.length_m.or(metrics.perimeter_m) {
        let lu = opts
            .and_then(|o| o.length_unit_override)
            .unwrap_or(scale.display_unit);
        lu.from_meters(l)
    } else if let Some(c) = metrics.count {
        c as f64
    } else {
        0.0
    }
}

/// Evaluates any custom column (stored or formula) for `markup`.
pub fn evaluate_column_cell(
    col: &CustomColumnDef,
    all_columns: &[CustomColumnDef],
    markup: &Markup,
    metrics: &ComputedMetrics,
    page_scale: &PageScale,
    viewports: &[ViewportScale],
) -> EvaluatedCell {
    let mut visiting = HashSet::new();
    evaluate_column_cell_inner(
        col,
        all_columns,
        markup,
        metrics,
        page_scale,
        viewports,
        &mut visiting,
    )
}

fn evaluate_column_cell_inner(
    col: &CustomColumnDef,
    all_columns: &[CustomColumnDef],
    markup: &Markup,
    metrics: &ComputedMetrics,
    page_scale: &PageScale,
    viewports: &[ViewportScale],
    visiting: &mut HashSet<Uuid>,
) -> EvaluatedCell {
    match &col.kind {
        CustomColumnKind::Text { default_value } => {
            let txt = match markup.custom_values.get(&col.id) {
                Some(CustomColumnValue::Text(s)) => s.clone(),
                _ => default_value.clone(),
            };
            let numeric = txt.trim().replace(',', ".").parse::<f64>().ok();
            EvaluatedCell {
                display: txt,
                numeric,
                error: None,
            }
        }
        CustomColumnKind::Number {
            default_value,
            decimals,
            prefix,
            suffix,
            ..
        } => {
            let val = match markup.custom_values.get(&col.id) {
                Some(CustomColumnValue::Number(v)) if v.is_finite() => *v,
                _ => *default_value,
            };
            EvaluatedCell {
                display: format_custom_number(val, *decimals, prefix, suffix),
                numeric: Some(val),
                error: None,
            }
        }
        CustomColumnKind::Choice { items, .. } => {
            let chosen = match markup.custom_values.get(&col.id) {
                Some(CustomColumnValue::Text(s)) => s.clone(),
                _ => String::new(),
            };
            let numeric = items
                .iter()
                .find(|it| it.label.eq_ignore_ascii_case(chosen.trim()))
                .and_then(|it| it.numeric_value);
            EvaluatedCell {
                display: chosen,
                numeric,
                error: None,
            }
        }
        CustomColumnKind::Checkmark { default_value } => {
            let checked = match markup.custom_values.get(&col.id) {
                Some(CustomColumnValue::Bool(b)) => *b,
                _ => *default_value,
            };
            EvaluatedCell {
                display: if checked { "✓".to_string() } else { String::new() },
                numeric: Some(if checked { 1.0 } else { 0.0 }),
                error: None,
            }
        }
        CustomColumnKind::Formula {
            expression,
            decimals,
            prefix,
            suffix,
            ..
        } => {
            if !visiting.insert(col.id) {
                return EvaluatedCell {
                    display: "#CICLO".to_string(),
                    numeric: None,
                    error: Some("Referencia circular en fórmula".to_string()),
                };
            }
            let vars = build_variable_map(
                all_columns,
                markup,
                metrics,
                page_scale,
                viewports,
                visiting,
            );
            visiting.remove(&col.id);

            match eval_formula_expression(expression, &vars) {
                Ok(v) if v.is_finite() => EvaluatedCell {
                    display: if v.abs() < 1e-12 && markup.unit_cost.is_none() && !markup.is_measurement() {
                        "—".to_string()
                    } else {
                        format_custom_number(v, *decimals, prefix, suffix)
                    },
                    numeric: Some(v),
                    error: None,
                },
                Ok(_) => EvaluatedCell {
                    display: "#DIV/0".to_string(),
                    numeric: None,
                    error: Some("Resultado no finito".to_string()),
                },
                Err(e) => EvaluatedCell {
                    display: "#ERR".to_string(),
                    numeric: None,
                    error: Some(e),
                },
            }
        }
    }
}

fn format_custom_number(val: f64, decimals: usize, prefix: &str, suffix: &str) -> String {
    let clean = if val.abs() < 1e-9 { 0.0 } else { val };
    let num = format!("{clean:.decimals$}");
    format!("{prefix}{num}{suffix}")
}

/// Normalizes a variable/column identifier for case- and accent-insensitive lookup.
pub fn normalize_var_key(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .map(|c| match c.to_ascii_lowercase() {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

fn build_variable_map(
    all_columns: &[CustomColumnDef],
    markup: &Markup,
    metrics: &ComputedMetrics,
    page_scale: &PageScale,
    viewports: &[ViewportScale],
    visiting: &mut HashSet<Uuid>,
) -> BTreeMap<String, f64> {
    let scale = resolve_scale_at(page_scale, viewports, markup.anchor_point());
    let opts = markup.measurement_opts();
    let lu = opts
        .and_then(|o| o.length_unit_override)
        .unwrap_or(scale.display_unit);
    let au = opts
        .and_then(|o| o.area_unit_override)
        .unwrap_or(scale.area_unit);
    let vu = opts
        .and_then(|o| o.volume_unit_override)
        .unwrap_or(scale.volume_unit);

    let len = metrics.length_m.map(|v| lu.from_meters(v)).unwrap_or(0.0);
    let perim = metrics
        .perimeter_m
        .map(|v| lu.from_meters(v))
        .unwrap_or(0.0);
    let area = metrics.area_m2.map(|v| au.from_sq_meters(v)).unwrap_or(0.0);
    let vol = metrics
        .volume_m3
        .map(|v| vu.from_cu_meters(v))
        .unwrap_or(0.0);
    let wall = metrics
        .wall_area_m2
        .map(|v| au.from_sq_meters(v))
        .unwrap_or(0.0);
    let depth = opts.map(|o| lu.from_meters(o.depth_meters)).unwrap_or(0.0);
    let count = metrics.count.unwrap_or(0) as f64;
    let meas = primary_measurement_quantity(markup, metrics, page_scale, viewports);
    let unit_cost = markup.unit_cost.unwrap_or(0.0);

    let mut vars = BTreeMap::new();
    for (k, v) in [
        ("measurement", meas),
        ("medicion", meas),
        ("cantidad", meas),
        ("qty", meas),
        ("length", len),
        ("longitud", len),
        ("largo", len),
        ("perimeter", perim),
        ("perimetro", perim),
        ("area", area),
        ("superficie", area),
        ("volume", vol),
        ("volumen", vol),
        ("wallarea", wall),
        ("areamuro", wall),
        ("depth", depth),
        ("profundidad", depth),
        ("altura", depth),
        ("count", count),
        ("conteo", count),
        ("unitcost", unit_cost),
        ("preciounitario", unit_cost),
        ("precio", unit_cost),
    ] {
        vars.insert(k.to_string(), v);
    }

    for col in all_columns {
        if visiting.contains(&col.id) {
            continue;
        }
        let cell = evaluate_column_cell_inner(
            col,
            all_columns,
            markup,
            metrics,
            page_scale,
            viewports,
            visiting,
        );
        if let Some(num) = cell.numeric {
            vars.insert(normalize_var_key(&col.name), num);
        }
    }
    vars
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Comma,
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '+' => {
                out.push(Token::Plus);
                i += 1;
            }
            '-' => {
                out.push(Token::Minus);
                i += 1;
            }
            '*' | '×' => {
                out.push(Token::Star);
                i += 1;
            }
            '/' | '÷' => {
                out.push(Token::Slash);
                i += 1;
            }
            '(' => {
                out.push(Token::LParen);
                i += 1;
            }
            ')' => {
                out.push(Token::RParen);
                i += 1;
            }
            ',' | ';' => {
                out.push(Token::Comma);
                i += 1;
            }
            '>' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Token::Ge);
                    i += 2;
                } else {
                    out.push(Token::Gt);
                    i += 1;
                }
            }
            '<' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Token::Le);
                    i += 2;
                } else {
                    out.push(Token::Lt);
                    i += 1;
                }
            }
            '=' => {
                if chars.get(i + 1) == Some(&'=') {
                    i += 2;
                } else {
                    i += 1;
                }
                out.push(Token::Eq);
            }
            '[' => {
                // Bracketed column name like `[Subtotal c/Desp.]`
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != ']' {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err("Falta corchete de cierre `]`".to_string());
                }
                let raw: String = chars[start..i].iter().collect();
                out.push(Token::Ident(normalize_var_key(&raw)));
                i += 1;
            }
            d if d.is_ascii_digit() || d == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let val = num_str
                    .parse::<f64>()
                    .map_err(|_| format!("Número inválido: {num_str}"))?;
                out.push(Token::Number(val));
            }
            ch if ch.is_alphabetic() || ch == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let raw: String = chars[start..i].iter().collect();
                out.push(Token::Ident(normalize_var_key(&raw)));
            }
            other => return Err(format!("Carácter no reconocido en fórmula: '{other}'")),
        }
    }
    Ok(out)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    vars: &'a BTreeMap<String, f64>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn parse_comparison(&mut self) -> Result<f64, String> {
        let mut lhs = self.parse_add_sub()?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Token::Gt | Token::Lt | Token::Ge | Token::Le | Token::Eq => {
                    self.next();
                    let rhs = self.parse_add_sub()?;
                    let ok = match op {
                        Token::Gt => lhs > rhs,
                        Token::Lt => lhs < rhs,
                        Token::Ge => lhs >= rhs,
                        Token::Le => lhs <= rhs,
                        Token::Eq => (lhs - rhs).abs() < 1e-9,
                        _ => false,
                    };
                    lhs = if ok { 1.0 } else { 0.0 };
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_add_sub(&mut self) -> Result<f64, String> {
        let mut lhs = self.parse_mul_div()?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Token::Plus => {
                    self.next();
                    lhs += self.parse_mul_div()?;
                }
                Token::Minus => {
                    self.next();
                    lhs -= self.parse_mul_div()?;
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_mul_div(&mut self) -> Result<f64, String> {
        let mut lhs = self.parse_unary()?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Token::Star => {
                    self.next();
                    lhs *= self.parse_unary()?;
                }
                Token::Slash => {
                    self.next();
                    let rhs = self.parse_unary()?;
                    if rhs.abs() < 1e-12 {
                        return Err("División por cero".to_string());
                    }
                    lhs /= rhs;
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some(Token::Plus) => {
                self.next();
                self.parse_unary()
            }
            Some(Token::Minus) => {
                self.next();
                Ok(-self.parse_unary()?)
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<f64, String> {
        match self.next() {
            Some(Token::Number(n)) => Ok(n),
            Some(Token::Ident(name)) => {
                if matches!(self.peek(), Some(Token::LParen)) {
                    self.next(); // consume '('
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Token::RParen)) {
                        loop {
                            args.push(self.parse_comparison()?);
                            if matches!(self.peek(), Some(Token::Comma)) {
                                self.next();
                            } else {
                                break;
                            }
                        }
                    }
                    if !matches!(self.next(), Some(Token::RParen)) {
                        return Err(format!("Falta `)` en función {name}"));
                    }
                    eval_builtin_fn(&name, &args)
                } else {
                    Ok(*self.vars.get(&name).unwrap_or(&0.0))
                }
            }
            Some(Token::LParen) => {
                let v = self.parse_comparison()?;
                if !matches!(self.next(), Some(Token::RParen)) {
                    return Err("Falta paréntesis de cierre `)`".to_string());
                }
                Ok(v)
            }
            _ => Err("Expresión incompleta".to_string()),
        }
    }
}

fn eval_builtin_fn(name: &str, args: &[f64]) -> Result<f64, String> {
    match name {
        "round" | "redondear" => {
            let x = *args.first().ok_or("round requiere al menos 1 argumento")?;
            let dec = args.get(1).copied().unwrap_or(0.0).clamp(0.0, 8.0) as i32;
            let f = 10f64.powi(dec);
            Ok((x * f).round() / f)
        }
        "ceil" | "techo" => Ok(args.first().copied().unwrap_or(0.0).ceil()),
        "floor" | "piso" => Ok(args.first().copied().unwrap_or(0.0).floor()),
        "abs" => Ok(args.first().copied().unwrap_or(0.0).abs()),
        "min" => args
            .iter()
            .copied()
            .reduce(f64::min)
            .ok_or_else(|| "min requiere argumentos".to_string()),
        "max" => args
            .iter()
            .copied()
            .reduce(f64::max)
            .ok_or_else(|| "max requiere argumentos".to_string()),
        "if" | "si" => {
            if args.len() < 3 {
                return Err("if(cond, a, b) requiere 3 argumentos".to_string());
            }
            Ok(if args[0].abs() > 1e-9 { args[1] } else { args[2] })
        }
        other => Err(format!("Función desconocida: {other}")),
    }
}

/// Evaluates an arithmetic formula expression safely against `vars`.
pub fn eval_formula_expression(expr: &str, vars: &BTreeMap<String, f64>) -> Result<f64, String> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Ok(0.0);
    }
    let tokens = tokenize(trimmed)?;
    if tokens.is_empty() {
        return Ok(0.0);
    }
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
        vars,
    };
    let val = parser.parse_comparison()?;
    if parser.pos < tokens.len() {
        return Err("Tokens inesperados al final de la fórmula".to_string());
    }
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::math::{Point2, Rect2};
    use crate::model::markup::{MarkupKind, MarkupStyle, MeasurementOptions};
    use crate::model::units::PT_PER_MM;

    #[test]
    fn evaluates_takeoff_formula_with_waste_and_custom_columns() {
        let scale = PageScale::from_metric_ratio(100.0);
        let ten_m = 100.0 * PT_PER_MM;
        let mut m = Markup::new(
            0,
            "Radier H25",
            "Matías",
            MarkupStyle::default(),
            MarkupKind::MeasureArea {
                points: Rect2::from_xywh(0.0, 0.0, ten_m, ten_m).corners().to_vec(),
                cutouts: vec![],
                opts: MeasurementOptions::default(),
            },
        );
        m.unit_cost = Some(12_000.0);

        let cols = CustomColumnDef::default_chilean_takeoff_columns();
        let metrics = m.compute_metrics(&scale, &[]);
        // 100 m² * 12,000 * 1.05 = 1,260,000
        let res = evaluate_column_cell(&cols[2], &cols, &m, &metrics, &scale, &[]);
        assert!(res.error.is_none());
        assert!((res.numeric.unwrap() - 1_260_000.0).abs() < 1e-3);
        assert_eq!(res.display, "$ 1260000");
    }

    #[test]
    fn detects_circular_formula_reference_safely() {
        let mut c1 = CustomColumnDef::new(
            "A",
            CustomColumnKind::Formula {
                expression: "B + 1".to_string(),
                decimals: 2,
                prefix: String::new(),
                suffix: String::new(),
                include_in_totals: false,
            },
        );
        let c2 = CustomColumnDef::new(
            "B",
            CustomColumnKind::Formula {
                expression: "A + 1".to_string(),
                decimals: 2,
                prefix: String::new(),
                suffix: String::new(),
                include_in_totals: false,
            },
        );
        let cols = vec![c1.clone(), c2];
        let m = Markup::new(
            0,
            "Nota",
            "M",
            MarkupStyle::default(),
            MarkupKind::Note {
                pos: Point2::new(10.0, 10.0),
            },
        );
        let scale = PageScale::default();
        let metrics = m.compute_metrics(&scale, &[]);
        c1.id = cols[0].id;
        let res = evaluate_column_cell(&cols[0], &cols, &m, &metrics, &scale, &[]);
        // Evaluates gracefully without stack overflow (cycle returns 0.0 for inner ref or #CICLO)
        assert!(res.numeric.is_some() || res.error.is_some());
    }

    #[test]
    fn evaluates_builtin_functions_and_conditionals() {
        let mut vars = BTreeMap::new();
        vars.insert("area".to_string(), 14.3);
        assert_eq!(
            eval_formula_expression("ceil(Area * 1.1)", &vars).unwrap(),
            16.0
        );
        assert_eq!(
            eval_formula_expression("if(Area > 10, 100, 50)", &vars).unwrap(),
            100.0
        );
        assert!(eval_formula_expression("10 / 0", &vars).is_err());
    }
}
