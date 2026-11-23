use crate::geometry::math::{Point2, Rect2};
use crate::model::document::AlignKind;
use crate::model::markup::MarkupStatus;
use crate::model::scale::PageScale;
use crate::model::settings::{CsvDelimiter, ThemeMode};
use crate::model::tool_chest::{ActiveTool, ToolChestItem};
use crate::model::units::LengthUnit;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum DialogRequest {
    Calibrate {
        measured_pts: f64,
        initial_value: String,
        unit: LengthUnit,
        apply_all_pages: bool,
    },
    SetScale,
    EditViewport {
        viewport_id: Uuid,
    },
    SketchToScale,
    CreateStamp,
    Preferences,
    Shortcuts,
    CommandPalette,
    About,
}

#[derive(Debug, Clone)]
pub enum UiAction {
    NewBlankDocument,
    OpenPdfDialog,
    OpenPdfPath(PathBuf),
    OpenDemoChile,
    SaveDocument,
    SaveDocumentAs,
    ExportFlattenedPdf,
    ExportCsv(CsvDelimiter),
    ExportMarkupsJson,
    ImportMarkupsJson,
    CombinePdfs,
    CompareWithPdf,
    ExtractCurrentPage,
    CloseTab(usize),
    SelectTab(usize),

    Undo,
    Redo,
    CopySelected,
    PasteClipboard,
    DuplicateSelected,
    DeleteSelected,
    SelectAllOnPage,
    GroupSelected,
    UngroupSelected,
    AlignSelected(AlignKind),
    BringToFront,
    SendToBack,
    SnapSelectedToContent,
    ToggleLockSelected,

    SetTool(ActiveTool),
    ActivateToolChestItem(ToolChestItem),
    AddSelectedToToolChest,
    SetCurrentStyleAsDefault,
    MarkupsStyleChanged,

    FitPage,
    FitWidth,
    Zoom100,
    ZoomStep(f64),
    GoToPage(usize),
    JumpToMarkup(Uuid),
    JumpToPageText(usize),

    ToggleSplitVertical,
    ToggleSplitHorizontal,
    SetSplitNone,

    RotatePage(usize, i32),
    InsertBlankPage(usize),
    DeletePage(usize),

    ApplyPageScale {
        scale: PageScale,
        apply_all: bool,
    },
    ApplyScaleToRange {
        scale: PageScale,
        pages: Vec<usize>,
    },
    SaveViewportEdit {
        id: Uuid,
        name: String,
        scale: PageScale,
    },
    CaptureSnapshot(Rect2),
    StartTwoPointCalibration,
    OpenDialog(DialogRequest),
    SetTheme(ThemeMode),
    InsertLegendOnCurrentPage(Point2),
    CreateCutoutForMarkup(Uuid),
    DeleteViewport(Uuid),
    ChangeSelectedStatus(MarkupStatus),
    StartInlineEdit(Uuid),
    RequestRedrawTextures,
    ShowToast(String),
    ExitApp,
}
