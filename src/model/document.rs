use crate::geometry::math::{Point2, Rect2};
use crate::model::markup::{Markup, MarkupKind};
use crate::model::scale::{PageScale, ViewportScale};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageModel {
    pub page_index: usize,
    /// Visual page width in PDF points (after page rotation).
    pub width_pt: f64,
    /// Visual page height in PDF points (after page rotation).
    pub height_pt: f64,
    /// User-applied extra rotation in degrees (0, 90, 180, 270).
    pub rotation_deg: u16,
    pub scale: PageScale,
    pub viewports: Vec<ViewportScale>,
}

impl PageModel {
    pub fn new(page_index: usize, width_pt: f64, height_pt: f64) -> Self {
        Self {
            page_index,
            width_pt: width_pt.max(10.0),
            height_pt: height_pt.max(10.0),
            rotation_deg: 0,
            scale: PageScale::default(),
            viewports: Vec::new(),
        }
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
    pub fit_pending: bool,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            page_index: 0,
            zoom: 0.75,
            pan: Point2::new(0.0, 0.0),
            fit_pending: true,
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
}

#[derive(Debug, Clone)]
pub struct UndoSnapshot {
    pub markups: Vec<Markup>,
    pub pages: Vec<PageModel>,
    pub layers: Vec<LayerItem>,
}

#[derive(Debug, Clone)]
pub struct DocumentTab {
    pub id: Uuid,
    pub title: String,
    pub file_path: Option<PathBuf>,
    pub raw_pdf_bytes: Vec<u8>,
    pub pages: Vec<PageModel>,
    pub markups: Vec<Markup>,
    pub layers: Vec<LayerItem>,
    pub bookmarks: Vec<BookmarkNode>,
    pub selected_ids: Vec<Uuid>,
    pub selected_viewport_id: Option<Uuid>,
    pub camera: CameraState,
    pub split_mode: SplitViewMode,
    pub secondary_camera: CameraState,
    pub active_pane_secondary: bool,
    pub dirty: bool,
    pub undo_stack: Vec<UndoSnapshot>,
    pub redo_stack: Vec<UndoSnapshot>,
    pub search_text_by_page: Vec<String>,
}

impl DocumentTab {
    pub fn new_with_pages(
        title: impl Into<String>,
        file_path: Option<PathBuf>,
        raw_pdf_bytes: Vec<u8>,
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
            raw_pdf_bytes,
            pages,
            markups: Vec::new(),
            layers,
            bookmarks: Vec::new(),
            selected_ids: Vec::new(),
            selected_viewport_id: None,
            camera: CameraState::default(),
            split_mode: SplitViewMode::None,
            secondary_camera: CameraState::default(),
            active_pane_secondary: false,
            dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            search_text_by_page: vec![String::new(); page_count],
        }
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
        });
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.dirty = true;
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo_stack.pop() else {
            return false;
        };
        self.redo_stack.push(UndoSnapshot {
            markups: self.markups.clone(),
            pages: self.pages.clone(),
            layers: self.layers.clone(),
        });
        self.markups = prev.markups;
        self.pages = prev.pages;
        self.layers = prev.layers;
        self.selected_ids
            .retain(|id| self.markups.iter().any(|m| m.id == *id));
        self.dirty = true;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };
        self.undo_stack.push(UndoSnapshot {
            markups: self.markups.clone(),
            pages: self.pages.clone(),
            layers: self.layers.clone(),
        });
        self.markups = next.markups;
        self.pages = next.pages;
        self.layers = next.layers;
        self.selected_ids
            .retain(|id| self.markups.iter().any(|m| m.id == *id));
        self.dirty = true;
        true
    }

    pub fn is_layer_visible(&self, layer_name: &str) -> bool {
        self.layers
            .iter()
            .find(|l| l.name == layer_name)
            .map(|l| l.visible)
            .unwrap_or(true)
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
        if let Some(pos) = self.selected_ids.iter().position(|x| *x == id) {
            self.selected_ids.remove(pos);
        } else {
            self.selected_ids.push(id);
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
        let before = self.markups.len();
        self.push_undo();
        self.markups.retain(|m| !set.contains(&m.id) || m.locked);
        let removed = before - self.markups.len();
        self.selected_ids.clear();
        removed
    }

    pub fn bring_selected_to_front(&mut self) {
        if self.selected_ids.is_empty() {
            return;
        }
        self.push_undo();
        let set: HashSet<Uuid> = self.selected_ids.iter().copied().collect();
        let mut back = Vec::new();
        let mut front = Vec::new();
        for m in self.markups.drain(..) {
            if set.contains(&m.id) {
                front.push(m);
            } else {
                back.push(m);
            }
        }
        back.extend(front);
        self.markups = back;
    }

    pub fn send_selected_to_back(&mut self) {
        if self.selected_ids.is_empty() {
            return;
        }
        self.push_undo();
        let set: HashSet<Uuid> = self.selected_ids.iter().copied().collect();
        let mut back = Vec::new();
        let mut front = Vec::new();
        for m in self.markups.drain(..) {
            if set.contains(&m.id) {
                back.push(m);
            } else {
                front.push(m);
            }
        }
        back.extend(front);
        self.markups = back;
    }

    pub fn selected_markups(&self) -> Vec<&Markup> {
        let set: HashSet<Uuid> = self.selected_ids.iter().copied().collect();
        self.markups.iter().filter(|m| set.contains(&m.id)).collect()
    }

    pub fn export_embedded_state(&self) -> RedBimEmbeddedState {
        RedBimEmbeddedState {
            version: 1,
            pages: self.pages.clone(),
            markups: self.markups.clone(),
            layers: self.layers.clone(),
            bookmarks: self.bookmarks.clone(),
        }
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
        let mut items: Vec<(usize, Rect2)> = self
            .markups
            .iter()
            .enumerate()
            .filter(|(_, m)| set.contains(&m.id) && !m.locked)
            .map(|(idx, m)| (idx, markup_bounds(m)))
            .collect();
        if items.len() < 2 {
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
            cam.fit_pending = false;
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
