use crate::geometry::math::{Point2, Rect2};
use crate::model::document::AlignKind;
use crate::model::markup::{HyperlinkTarget, MarkupStatus};
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
    ManageCustomColumns,
    EditPageLabel {
        page_index: usize,
    },
    OverlayPdfs,
    VisualSearchResults {
        result: crate::pdf::visual_search::VisualSearchResult,
    },
    HeadersAndFooters,
    BatchProcess,
    ConfigureSequence {
        item_id: Uuid,
    },
    PhotoPreview {
        attachment_id: Uuid,
        caption: String,
    },
    ApplyRedactionsConfirm,
    ColorProcess,
    ReduceFileSize,
    Preferences,
    Shortcuts,
    CommandPalette,
    About,
    ConfirmClose {
        /// `Some(idx)` = closing a specific tab; `None` = exiting the whole app.
        tab_idx: Option<usize>,
    },
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
    FlattenInPlace {
        recoverable: bool,
    },
    UnflattenDocument,
    ReplaceCurrentPageContent,
    RunOverlay {
        pdf_b_path: PathBuf,
        color_a: [u8; 3],
        color_b: [u8; 3],
        alignment: crate::pdf::overlay::OverlayAlignmentMode,
        all_pages: bool,
    },
    RunVisualSearch(Rect2),
    CreateCountFromVisualSearch {
        page_index: usize,
        subject: String,
        points: Vec<Point2>,
    },
    ApplyHeadersFooters(crate::pdf::headers_footers::HeaderFooterConfig),
    RemoveHeadersFooters,
    RunBatchJob(crate::pdf::batch::BatchJobConfig),
    AttachPhotoToSelected,
    DeleteAttachment {
        markup_id: Uuid,
        attachment_id: Uuid,
    },
    ConsumeToolSequence {
        item_id: Uuid,
        markup_id: Uuid,
    },
    ExportToolSet(Uuid),
    ImportToolSet,
    ApplyRedactions {
        fill_color: [u8; 3],
    },
    RunColorProcess(crate::pdf::color_process::ColorProcessConfig),
    RunReduceFileSize(crate::pdf::optimize::OptimizePreset),
    ExportCsv(CsvDelimiter),
    ExportMarkupsJson,
    ImportMarkupsJson,
    CombinePdfs,
    CompareWithPdf,
    ExtractCurrentPage,
    CloseTab(usize),
    ForceCloseTab(usize),
    SelectTab(usize),

    Undo,
    Redo,
    CopySelected,
    PasteClipboard,
    PasteInPlace,
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
    ShowMarkupProperties,

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
    LocateTextMarkup(Uuid),
    JumpToPageText(usize),

    ToggleSplitVertical,
    ToggleSplitHorizontal,
    ToggleSyncSplitViews,
    SetSplitNone,

    RotatePage(usize, i32),
    InsertBlankPage(usize),
    DeletePage(usize),
    RenamePageLabel {
        page_index: usize,
        label: String,
    },
    FollowHyperlink(HyperlinkTarget),
    DeleteSpace(Uuid),
    ToggleHighlightSpaces,

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
    CheckForUpdates,
    ExitApp,
    ForceExitApp,
}
