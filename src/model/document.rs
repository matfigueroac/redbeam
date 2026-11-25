use crate::geometry::math::{Point2, Rect2, point_in_polygon, polygon_area};
use crate::model::custom_columns::CustomColumnDef;
use crate::model::markup::{Markup, MarkupKind};
use crate::model::scale::{PageScale, ViewportScale};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// Named polygonal region on a sheet (mirrors Bluebeam's `PDFSpaceModel` / `SpacesContextModel`).
/// Markups whose anchor point falls inside a Space automatically inherit its `full_title`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceRegion {
    pub id: Uuid,
    pub page_index: usize,
    pub title: String,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    pub points: Vec<Point2>,
    pub fill_color: [u8; 4],
    pub fill_opacity: f32,
}

impl SpaceRegion {
    pub fn new(page_index: usize, title: impl Into<String>, points: Vec<Point2>) -> Self {
        Self {
            id: Uuid::new_v4(),
            page_index,
            title: title.into(),
            parent_id: None,
            points,
            fill_color: [56, 189, 248, 255],
            fill_opacity: 0.14,
        }
    }

    pub fn bounds(&self) -> Rect2 {
        Rect2::from_points(&self.points).unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageModel {
    pub page_index: usize,
    /// Visual page width in PDF points (after page rotation).
    pub width_pt: f64,
    /// Visual page height in PDF points (after page rotation).
    pub height_pt: f64,
    /// User-applied / effective rotation in degrees (0, 90, 180, 270).
    pub rotation_deg: u16,
    /// Optional architectural sheet label (e.g. "ARQ-01", "EST-04") — mirrors Bluebeam Page Labels.
    #[serde(default)]
    pub label: Option<String>,
    /// Underlying PDF CropBox/MediaBox `[x0, y0, x1, y1]` in PDF user space (before `/Rotate`).
    #[serde(default)]
    pub pdf_crop_box: Option<[f64; 4]>,
    /// Underlying PDF `/Rotate` value in degrees (0, 90, 180, 270) for exact user-space transforms.
    #[serde(default)]
    pub pdf_native_rotation: u16,
    pub scale: PageScale,
    pub viewports: Vec<ViewportScale>,
}

impl PageModel {
    pub fn new(page_index: usize, width_pt: f64, height_pt: f64) -> Self {
        let w = width_pt.max(10.0);
        let h = height_pt.max(10.0);
        Self {
            page_index,
            width_pt: w,
            height_pt: h,
            rotation_deg: 0,
            label: None,
            pdf_crop_box: Some([0.0, 0.0, w, h]),
            pdf_native_rotation: 0,
            scale: PageScale::default(),
            viewports: Vec::new(),
        }
    }

    /// Display label for UI/CSV: custom Page Label if set, otherwise `"Lámina N"`.
    pub fn display_label(&self) -> String {
        if let Some(ref l) = self.label
            && !l.trim().is_empty()
        {
            return l.trim().to_string();
        }
        format!("Lámina {}", self.page_index + 1)
    }

    /// Short page tag for compact tables/status bar: custom label or 1-based page number.
    pub fn short_label(&self) -> String {
        if let Some(ref l) = self.label
            && !l.trim().is_empty()
        {
            return l.trim().to_string();
        }
        format!("{}", self.page_index + 1)
    }

    fn effective_crop_box(&self) -> [f64; 4] {
        let fallback = match self.pdf_native_rotation % 360 {
            90 | 270 => [0.0, 0.0, self.height_pt, self.width_pt],
            _ => [0.0, 0.0, self.width_pt, self.height_pt],
        };
        self.pdf_crop_box.unwrap_or(fallback)
    }

    /// Maps normalized visual top-left page point `(vx, vy)` into exact PDF user-space `(ux, uy)`,
    /// honoring `/CropBox` origin and `/Rotate` (0°, 90°, 180°, 270°).
    pub fn page_to_pdf_pt(&self, pt: Point2) -> (f64, f64) {
        let [x0, y0, x1, y1] = self.effective_crop_box();
        match self.pdf_native_rotation % 360 {
            90 => (x0 + pt.y, y0 + pt.x),
            180 => (x1 - pt.x, y0 + pt.y),
            270 => (x1 - pt.y, y1 - pt.x),
            _ => (x0 + pt.x, y1 - pt.y),
        }
    }

    /// Maps a visual bounding rectangle into PDF user-space `[llx, lly, urx, ury]`.
    pub fn page_rect_to_pdf_rect(&self, rect: Rect2) -> [f64; 4] {
        let (ax, ay) = self.page_to_pdf_pt(rect.min());
        let (bx, by) = self.page_to_pdf_pt(rect.max());
        [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)]
    }

    /// Maps a PDF user-space point `(ux, uy)` back into normalized visual top-left page coordinates.
    pub fn pdf_to_page_pt(&self, ux: f64, uy: f64) -> Point2 {
        let [x0, y0, x1, y1] = self.effective_crop_box();
        match self.pdf_native_rotation % 360 {
            90 => Point2::new(uy - y0, ux - x0),
            180 => Point2::new(x1 - ux, uy - y0),
            270 => Point2::new(y1 - uy, x1 - ux),
            _ => Point2::new(ux - x0, y1 - uy),
        }
    }

    /// Returns a 6-element PDF `cm` matrix `[a, b, c, d, e, f]` mapping standard unrotated
    /// bottom-left `(x, page_h - y)` overlay drawing coordinates into this page's true PDF user space
    /// (supporting non-zero `/CropBox` and `/Rotate`).
    pub fn pdf_overlay_cm_matrix(&self) -> Option<[f64; 6]> {
        let [x0, y0, x1, y1] = self.effective_crop_box();
        let rot = self.pdf_native_rotation % 360;
        if rot == 0 && x0.abs() < 1e-6 && y0.abs() < 1e-6 && (y1 - self.height_pt).abs() < 1e-6 {
            return None;
        }
        // Input to overlay ops is `(ox, oy) = (vx, vis_h - vy)` so `vx = ox`, `vy = vis_h - oy`.
        let h = self.height_pt;
        let m = match rot {
            // ux = x0 + (h - oy) = (x0 + h) - oy;  uy = y0 + ox
            90 => [0.0, 1.0, -1.0, 0.0, x0 + h, y0],
            // ux = x1 - ox;  uy = y0 + (h - oy) = (y0 + h) - oy
            180 => [-1.0, 0.0, 0.0, -1.0, x1, y0 + h],
            // ux = x1 - (h - oy) = (x1 - h) + oy;  uy = y1 - ox
            270 => [0.0, -1.0, 1.0, 0.0, x1 - h, y1],
            // ux = x0 + ox;  uy = y1 - (h - oy) = (y1 - h) + oy
            _ => [1.0, 0.0, 0.0, 1.0, x0, y1 - h],
        };
        Some(m)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerItem {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub color: [u8; 4],
}

impl LayerItem {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            visible: true,
            locked: false,
            color: [229, 57, 53, 255],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookmarkNode {
    pub title: String,
    pub page_index: Option<usize>,
    pub children: Vec<BookmarkNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SplitViewMode {
    #[default]
    None,
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraState {
    pub page_index: usize,
    /// Screen logical points per 1 PDF point.
    pub zoom: f64,
    /// Normalized page point at top-left of viewport.
    pub pan: Point2,
    /// Keep the entire page centered as the viewport changes.
    pub fit_page: bool,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            page_index: 0,
            zoom: 0.75,
            pan: Point2::new(0.0, 0.0),
            fit_page: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedBimEmbeddedState {
    pub version: u32,
    pub pages: Vec<PageModel>,
    pub markups: Vec<Markup>,
    pub layers: Vec<LayerItem>,
    pub bookmarks: Vec<BookmarkNode>,
    #[serde(default)]
    pub spaces: Vec<SpaceRegion>,
    #[serde(default)]
    pub custom_columns: Vec<CustomColumnDef>,
}

#[derive(Debug, Clone)]
pub struct UndoSnapshot {
    pub markups: Vec<Markup>,
    pub pages: Vec<PageModel>,
    pub layers: Vec<LayerItem>,
    pub bookmarks: Vec<BookmarkNode>,
    pub spaces: Vec<SpaceRegion>,
    pub custom_columns: Vec<CustomColumnDef>,
}

#[derive(Debug, Clone)]
pub struct DocumentTab {
    pub id: Uuid,
    pub title: String,
    pub file_path: Option<PathBuf>,
    pub raw_pdf_bytes: Arc<Vec<u8>>,
    pub(crate) render_revision: u64,
    pub pages: Vec<PageModel>,
    pub markups: Vec<Markup>,
    pub layers: Vec<LayerItem>,
    pub bookmarks: Vec<BookmarkNode>,
    pub spaces: Vec<SpaceRegion>,
    pub highlight_spaces: bool,
    pub custom_columns: Vec<CustomColumnDef>,
    pub selected_ids: Vec<Uuid>,
    pub selected_viewport_id: Option<Uuid>,
    pub camera: CameraState,
    pub split_mode: SplitViewMode,
    pub secondary_camera: CameraState,
    pub active_pane_secondary: bool,
    pub sync_split_views: bool,
    pub dirty: bool,
    pub undo_stack: Vec<UndoSnapshot>,
    pub redo_stack: Vec<UndoSnapshot>,
    pub search_text_by_page: Vec<String>,
}

impl DocumentTab {
    pub fn new_with_pages(
        title: impl Into<String>,
        file_path: Option<PathBuf>,
        raw_pdf_bytes: impl Into<Arc<Vec<u8>>>,
        pages: Vec<PageModel>,
    ) -> Self {
        let mut layers = vec![
            LayerItem::new("General"),
            LayerItem {
                name: "Arquitectura".to_string(),
                visible: true,
                locked: false,
                color: [229, 57, 53, 255],
            },
            LayerItem {
                name: "Estructuras".to_string(),
                visible: true,
                locked: false,
                color: [37, 99, 235, 255],
            },
            LayerItem {
                name: "Mediciones".to_string(),
                visible: true,
                locked: false,
                color: [22, 163, 74, 255],
            },
        ];
        layers.dedup_by(|a, b| a.name == b.name);
        let page_count = pages.len();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            file_path,
            raw_pdf_bytes: raw_pdf_bytes.into(),
            render_revision: 0,
            pages,
            markups: Vec::new(),
            layers,
            bookmarks: Vec::new(),
            spaces: Vec::new(),
            highlight_spaces: true,
            custom_columns: CustomColumnDef::default_chilean_takeoff_columns(),
            selected_ids: Vec::new(),
            selected_viewport_id: None,
            camera: CameraState::default(),
            split_mode: SplitViewMode::None,
            secondary_camera: CameraState::default(),
            active_pane_secondary: false,
            sync_split_views: false,
            dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            search_text_by_page: vec![String::new(); page_count],
        }
    }

    /// Copy only the PDF and editable state required by a background operation.
    pub(crate) fn pdf_operation_snapshot(&self, page_index: Option<usize>) -> Self {
        let mut snapshot = Self::new_with_pages(
            &self.title,
            None,
            Arc::clone(&self.raw_pdf_bytes),
            self.pages.clone(),
        );
        snapshot.markups = self
            .markups
            .iter()
            .filter(|markup| page_index.is_none_or(|page| markup.page_index == page))
            .cloned()
            .collect();
        snapshot.layers = self.layers.clone();
        if page_index.is_none() {
            snapshot.bookmarks = self.bookmarks.clone();
        }
        snapshot.spaces = self
            .spaces
            .iter()
            .filter(|space| page_index.is_none_or(|page| space.page_index == page))
            .cloned()
            .collect();
        snapshot.custom_columns = self.custom_columns.clone();
        snapshot
    }

    pub(crate) fn markups_by_page(&self) -> Vec<Vec<&Markup>> {
        let mut pages = vec![Vec::new(); self.pages.len()];
        for markup in &self.markups {
            if let Some(page) = pages.get_mut(markup.page_index) {
                page.push(markup);
            }
        }
        pages
    }

    pub fn active_camera(&self) -> &CameraState {
        if self.split_mode != SplitViewMode::None && self.active_pane_secondary {
            &self.secondary_camera
        } else {
            &self.camera
        }
    }

    pub fn active_camera_mut(&mut self) -> &mut CameraState {
        if self.split_mode != SplitViewMode::None && self.active_pane_secondary {
            &mut self.secondary_camera
        } else {
            &mut self.camera
        }
    }

    pub fn active_page_index(&self) -> usize {
        self.active_camera()
            .page_index
            .min(self.pages.len().saturating_sub(1))
    }

    pub fn active_page(&self) -> Option<&PageModel> {
        self.pages.get(self.active_page_index())
    }

    pub fn active_page_mut(&mut self) -> Option<&mut PageModel> {
        let idx = self.active_page_index();
        self.pages.get_mut(idx)
    }

    pub fn push_undo(&mut self) {
        self.undo_stack.push(UndoSnapshot {
            markups: self.markups.clone(),
            pages: self.pages.clone(),
            layers: self.layers.clone(),
            bookmarks: self.bookmarks.clone(),
            spaces: self.spaces.clone(),
            custom_columns: self.custom_columns.clone(),
        });
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.dirty = true;
    }

    /// Exchange owned vectors so moving between undo and redo never duplicates their contents.
    fn exchange_snapshot(&mut self, mut snapshot: UndoSnapshot) -> UndoSnapshot {
        std::mem::swap(&mut self.markups, &mut snapshot.markups);
        std::mem::swap(&mut self.pages, &mut snapshot.pages);
        std::mem::swap(&mut self.layers, &mut snapshot.layers);
        std::mem::swap(&mut self.bookmarks, &mut snapshot.bookmarks);
        std::mem::swap(&mut self.spaces, &mut snapshot.spaces);
        std::mem::swap(&mut self.custom_columns, &mut snapshot.custom_columns);
        self.selected_ids
            .retain(|id| self.markups.iter().any(|m| m.id == *id));
        if let Some(vp_id) = self.selected_viewport_id
            && !self
                .pages
                .iter()
                .any(|p| p.viewports.iter().any(|v| v.id == vp_id))
        {
            self.selected_viewport_id = None;
        }
        self.dirty = true;
        snapshot
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo_stack.pop() else {
            return false;
        };
        let current = self.exchange_snapshot(prev);
        self.redo_stack.push(current);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };
        let current = self.exchange_snapshot(next);
        self.undo_stack.push(current);
        true
    }

    pub fn is_layer_visible(&self, layer_name: &str) -> bool {
        self.layers
            .iter()
            .find(|l| l.name == layer_name)
            .map(|l| l.visible)
            .unwrap_or(true)
    }

    pub fn is_layer_locked(&self, layer_name: &str) -> bool {
        self.layers
            .iter()
            .find(|l| l.name == layer_name)
            .map(|l| l.locked)
            .unwrap_or(false)
    }

    pub fn is_markup_locked(&self, m: &Markup) -> bool {
        m.locked || self.is_layer_locked(&m.layer)
    }

    pub fn ensure_layer(&mut self, layer_name: &str) {
        if !layer_name.trim().is_empty() && !self.layers.iter().any(|l| l.name == layer_name) {
            self.layers.push(LayerItem::new(layer_name));
        }
    }

    pub fn select_only(&mut self, id: Uuid) {
        if let Some(m) = self.markups.iter().find(|x| x.id == id)
            && let Some(gid) = m.group_id
        {
            self.selected_ids = self
                .markups
                .iter()
                .filter(|x| x.group_id == Some(gid))
                .map(|x| x.id)
                .collect();
            return;
        }
        self.selected_ids = vec![id];
    }

    pub fn toggle_select(&mut self, id: Uuid) {
        let group_members: Vec<Uuid> = if let Some(m) = self.markups.iter().find(|x| x.id == id)
            && let Some(gid) = m.group_id
        {
            self.markups
                .iter()
                .filter(|x| x.group_id == Some(gid))
                .map(|x| x.id)
                .collect()
        } else {
            vec![id]
        };

        if self.selected_ids.contains(&id) {
            let member_set: HashSet<Uuid> = group_members.into_iter().collect();
            self.selected_ids.retain(|x| !member_set.contains(x));
        } else {
            for mid in group_members {
                if !self.selected_ids.contains(&mid) {
                    self.selected_ids.push(mid);
                }
            }
        }
    }

    pub fn group_selected(&mut self) -> bool {
        if self.selected_ids.len() < 2 {
            return false;
        }
        self.push_undo();
        let gid = Uuid::new_v4();
        let set: HashSet<Uuid> = self.selected_ids.iter().copied().collect();
        for m in &mut self.markups {
            if set.contains(&m.id) {
                m.group_id = Some(gid);
            }
        }
        true
    }

    pub fn ungroup_selected(&mut self) -> bool {
        if self.selected_ids.is_empty() {
            return false;
        }
        let set: HashSet<Uuid> = self.selected_ids.iter().copied().collect();
        if !self
            .markups
            .iter()
            .any(|m| set.contains(&m.id) && m.group_id.is_some())
        {
            return false;
        }
        self.push_undo();
        for m in &mut self.markups {
            if set.contains(&m.id) {
                m.group_id = None;
            }
        }
        true
    }

    pub fn delete_selected(&mut self) -> usize {
        if self.selected_ids.is_empty() {
            return 0;
        }
        let set: HashSet<Uuid> = self.selected_ids.iter().copied().collect();
        let locked_layers: HashSet<String> = self
            .layers
            .iter()
            .filter(|l| l.locked)
            .map(|l| l.name.clone())
            .collect();
        let removable = self
            .markups
            .iter()
            .filter(|m| set.contains(&m.id) && !m.locked && !locked_layers.contains(&m.layer))
            .count();
        if removable == 0 {
            return 0;
        }
        let before = self.markups.len();
        self.push_undo();
        self.markups
            .retain(|m| !set.contains(&m.id) || m.locked || locked_layers.contains(&m.layer));
        let removed = before - self.markups.len();
        self.selected_ids
            .retain(|id| self.markups.iter().any(|m| m.id == *id));
        removed
    }

    pub fn bring_selected_to_front(&mut self) {
        if self.selected_ids.is_empty() {
            return;
        }
        let set: HashSet<Uuid> = self.selected_ids.iter().copied().collect();
        let mut back = Vec::new();
        let mut front = Vec::new();
        for m in &self.markups {
            if set.contains(&m.id) {
                front.push(m.id);
            } else {
                back.push(m.id);
            }
        }
        if front.is_empty() || back.is_empty() {
            return;
        }
        back.extend(front);
        let unchanged = self.markups.iter().zip(&back).all(|(m, id)| m.id == *id);
        if unchanged {
            return;
        }
        self.push_undo();
        let mut back_m = Vec::new();
        let mut front_m = Vec::new();
        for m in self.markups.drain(..) {
            if set.contains(&m.id) {
                front_m.push(m);
            } else {
                back_m.push(m);
            }
        }
        back_m.extend(front_m);
        self.markups = back_m;
    }

    pub fn send_selected_to_back(&mut self) {
        if self.selected_ids.is_empty() {
            return;
        }
        let set: HashSet<Uuid> = self.selected_ids.iter().copied().collect();
        let mut back = Vec::new();
        let mut front = Vec::new();
        for m in &self.markups {
            if set.contains(&m.id) {
                back.push(m.id);
            } else {
                front.push(m.id);
            }
        }
        if back.is_empty() || front.is_empty() {
            return;
        }
        back.extend(front);
        let unchanged = self.markups.iter().zip(&back).all(|(m, id)| m.id == *id);
        if unchanged {
            return;
        }
        self.push_undo();
        let mut back_m = Vec::new();
        let mut front_m = Vec::new();
        for m in self.markups.drain(..) {
            if set.contains(&m.id) {
                back_m.push(m);
            } else {
                front_m.push(m);
            }
        }
        back_m.extend(front_m);
        self.markups = back_m;
    }

    pub fn selected_markups(&self) -> Vec<&Markup> {
        let set: HashSet<Uuid> = self.selected_ids.iter().copied().collect();
        self.markups.iter().filter(|m| set.contains(&m.id)).collect()
    }

    pub fn export_embedded_state(&self) -> RedBimEmbeddedState {
        RedBimEmbeddedState {
            version: 2,
            pages: self.pages.clone(),
            markups: self.markups.clone(),
            layers: self.layers.clone(),
            bookmarks: self.bookmarks.clone(),
            spaces: self.spaces.clone(),
            custom_columns: self.custom_columns.clone(),
        }
    }

    /// Builds hierarchical full title (e.g. `"Piso 1 > Cocina"`) for a Space (`PDFSpaceModel.FullTitle`).
    pub fn space_full_title(&self, space_id: Uuid) -> String {
        let mut chain = Vec::new();
        let mut cur = Some(space_id);
        let mut guard = 0;
        while let Some(id) = cur {
            guard += 1;
            if guard > 16 {
                break;
            }
            let Some(sp) = self.spaces.iter().find(|s| s.id == id) else {
                break;
            };
            chain.push(sp.title.as_str());
            cur = sp.parent_id;
        }
        chain.reverse();
        chain.join(" > ")
    }

    /// Resolves the innermost Space containing `pt` on `page_index`.
    pub fn resolve_space_for_point(&self, page_index: usize, pt: Point2) -> Option<String> {
        let mut best: Option<(f64, Uuid)> = None;
        for sp in &self.spaces {
            if sp.page_index != page_index || sp.points.len() < 3 {
                continue;
            }
            if point_in_polygon(pt, &sp.points) {
                let area = polygon_area(&sp.points);
                if best.is_none_or(|(best_a, _)| area < best_a) {
                    best = Some((area, sp.id));
                }
            }
        }
        best.map(|(_, id)| self.space_full_title(id))
    }

    /// Resolves the enclosing Space title for a markup based on its anchor point.
    pub fn resolve_space_for_markup(&self, m: &Markup) -> Option<String> {
        self.resolve_space_for_point(m.page_index, m.anchor_point())
    }

    /// Returns the human-readable page label for `page_index`.
    pub fn page_display_label(&self, page_index: usize) -> String {
        self.pages
            .get(page_index)
            .map(|p| p.display_label())
            .unwrap_or_else(|| format!("Lámina {}", page_index + 1))
    }

    /// Computes live totals across selected page or entire document.
    pub fn compute_totals(&self, page_filter: Option<usize>) -> DocumentTotals {
        let mut totals = DocumentTotals::default();
        for m in &self.markups {
            if let Some(p) = page_filter
                && m.page_index != p
            {
                continue;
            }
            if matches!(m.kind, MarkupKind::Legend { .. }) {
                continue;
            }
            let Some(page) = self.pages.get(m.page_index) else {
                continue;
            };
            let metrics = m.compute_metrics(&page.scale, &page.viewports);
            if let Some(v) = metrics.length_m {
                totals.total_length_m += v;
            }
            if let Some(v) = metrics.perimeter_m {
                totals.total_perimeter_m += v;
            }
            if let Some(v) = metrics.area_m2 {
                totals.total_area_m2 += v;
            }
            if let Some(v) = metrics.volume_m3 {
                totals.total_volume_m3 += v;
            }
            if let Some(c) = metrics.count {
                totals.total_count += c;
            }
            if let Some(cost) = m.total_cost(&page.scale, &page.viewports) {
                totals.total_cost += cost;
            }
            totals.total_markups += 1;
        }
        totals
    }

    pub fn align_selected(&mut self, kind: AlignKind) -> bool {
        use crate::geometry::hit_test::{markup_bounds, translate_markup};
        if self.selected_ids.len() < 2 {
            return false;
        }
        let set: HashSet<Uuid> = self.selected_ids.iter().copied().collect();
        let locked_layers: HashSet<String> = self
            .layers
            .iter()
            .filter(|l| l.locked)
            .map(|l| l.name.clone())
            .collect();
        let mut items: Vec<(usize, Rect2)> = self
            .markups
            .iter()
            .enumerate()
            .filter(|(_, m)| set.contains(&m.id) && !m.locked && !locked_layers.contains(&m.layer))
            .map(|(idx, m)| (idx, markup_bounds(m)))
            .collect();
        let min_required = match kind {
            AlignKind::DistributeH | AlignKind::DistributeV => 3,
            _ => 2,
        };
        if items.len() < min_required {
            return false;
        }
        self.push_undo();

        let min_x = items.iter().map(|(_, r)| r.min_x).fold(f64::INFINITY, f64::min);
        let max_x = items.iter().map(|(_, r)| r.max_x).fold(f64::NEG_INFINITY, f64::max);
        let min_y = items.iter().map(|(_, r)| r.min_y).fold(f64::INFINITY, f64::min);
        let max_y = items.iter().map(|(_, r)| r.max_y).fold(f64::NEG_INFINITY, f64::max);
        let cx_all = (min_x + max_x) * 0.5;
        let cy_all = (min_y + max_y) * 0.5;

        match kind {
            AlignKind::Left => {
                for (idx, r) in items {
                    translate_markup(&mut self.markups[idx], Point2::new(min_x - r.min_x, 0.0));
                }
            }
            AlignKind::CenterX => {
                for (idx, r) in items {
                    translate_markup(&mut self.markups[idx], Point2::new(cx_all - r.center().x, 0.0));
                }
            }
            AlignKind::Right => {
                for (idx, r) in items {
                    translate_markup(&mut self.markups[idx], Point2::new(max_x - r.max_x, 0.0));
                }
            }
            AlignKind::Top => {
                for (idx, r) in items {
                    translate_markup(&mut self.markups[idx], Point2::new(0.0, min_y - r.min_y));
                }
            }
            AlignKind::CenterY => {
                for (idx, r) in items {
                    translate_markup(&mut self.markups[idx], Point2::new(0.0, cy_all - r.center().y));
                }
            }
            AlignKind::Bottom => {
                for (idx, r) in items {
                    translate_markup(&mut self.markups[idx], Point2::new(0.0, max_y - r.max_y));
                }
            }
            AlignKind::DistributeH if items.len() >= 3 => {
                items.sort_by(|a, b| a.1.center().x.partial_cmp(&b.1.center().x).unwrap_or(std::cmp::Ordering::Equal));
                let first = items.first().unwrap().1.center().x;
                let last = items.last().unwrap().1.center().x;
                let step = (last - first) / (items.len() - 1) as f64;
                for (k, (idx, r)) in items.iter().enumerate() {
                    let target = first + step * k as f64;
                    translate_markup(&mut self.markups[*idx], Point2::new(target - r.center().x, 0.0));
                }
            }
            AlignKind::DistributeV if items.len() >= 3 => {
                items.sort_by(|a, b| a.1.center().y.partial_cmp(&b.1.center().y).unwrap_or(std::cmp::Ordering::Equal));
                let first = items.first().unwrap().1.center().y;
                let last = items.last().unwrap().1.center().y;
                let step = (last - first) / (items.len() - 1) as f64;
                for (k, (idx, r)) in items.iter().enumerate() {
                    let target = first + step * k as f64;
                    translate_markup(&mut self.markups[*idx], Point2::new(0.0, target - r.center().y));
                }
            }
            _ => {}
        }
        true
    }

    pub fn jump_to_rect(&mut self, page_index: usize, rect: Rect2, viewport_size: (f64, f64)) {
        if page_index < self.pages.len() {
            let cam = self.active_camera_mut();
            cam.page_index = page_index;
            cam.fit_page = false;
            let cx = rect.center().x;
            let cy = rect.center().y;
            let half_w = (viewport_size.0 / cam.zoom.max(0.05)) * 0.5;
            let half_h = (viewport_size.1 / cam.zoom.max(0.05)) * 0.5;
            cam.pan = Point2::new((cx - half_w).max(-40.0), (cy - half_h).max(-40.0));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignKind {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
    DistributeH,
    DistributeV,
}

#[derive(Debug, Clone, Default)]
pub struct DocumentTotals {
    pub total_markups: usize,
    pub total_length_m: f64,
    pub total_perimeter_m: f64,
    pub total_area_m2: f64,
    pub total_volume_m3: f64,
    pub total_count: usize,
    pub total_cost: f64,
}
