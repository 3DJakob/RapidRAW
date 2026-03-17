mod rapidraw_shader;

use ::image::{
    DynamicImage, GenericImageView, RgbImage, Rgba, RgbaImage, imageops::FilterType,
    open as open_image,
};
use imageproc::geometric_transformations::{Interpolation, rotate_about_center};
use iced::widget::{
    Space, button, canvas, column, container, image, mouse_area, row, scrollable, slider, stack,
    text, text_input, tooltip,
};
use iced::{
    Background, Border, Color, Element, Font, Length, Point, Rectangle, Size, Subscription, Task,
    Theme, application, keyboard, mouse, window,
};
use lucide_icons::{Icon as LucideIcon, LUCIDE_FONT_BYTES};
use rapidraw_shader::{
    BasicAdjustments, ColorCalibrationSettingsUi, ColorGradingSettingsUi, CropRect, CurvePoint,
    CurvesSettings, HslSettings, HueSatLum, RapidRawRenderer, ToneMapper, default_curve_points,
    parse_lut_metadata,
};
use rawler::{
    decoders::{Orientation, RawDecodeParams},
    imgop::develop::{DemosaicAlgorithm, Intermediate, ProcessingStep, RawDevelop},
    rawsource::RawSource,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

const BASE_CROP_RATIO: f32 = 1.618;
const CROP_RATIO_TOLERANCE: f32 = 0.01;

fn main() -> iced::Result {
    application("RapidRAW Iced POC Preview", App::update, App::view)
        .font(LUCIDE_FONT_BYTES)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(window::Settings {
            size: Size::new(1440.0, 920.0),
            min_size: Some(Size::new(1100.0, 720.0)),
            ..window::Settings::default()
        })
        .run_with(App::new)
}

#[derive(Debug, Clone)]
enum Message {
    EnterEditor,
    BackToHome,
    OpenFolder,
    FolderLoaded(Result<LoadedFolder, String>),
    SelectImage(usize),
    NavigateSelection(i32),
    SelectAllImages,
    ModifiersChanged(iced::keyboard::Modifiers),
    UndoRequested,
    AnimationFrame(Instant),
    ToggleBasicCard,
    ToggleCurvesCard,
    ToggleColorCard,
    ToggleDetailsCard,
    ToggleEffectsCard,
    ExposureChanged(f32),
    BrightnessChanged(f32),
    ContrastChanged(f32),
    HighlightsChanged(f32),
    ShadowsChanged(f32),
    WhitesChanged(f32),
    BlacksChanged(f32),
    TemperatureChanged(f32),
    TintChanged(f32),
    VibranceChanged(f32),
    SaturationChanged(f32),
    SharpnessChanged(f32),
    ClarityChanged(f32),
    DehazeChanged(f32),
    StructureChanged(f32),
    CentreChanged(f32),
    ChromaticAberrationRedCyanChanged(f32),
    ChromaticAberrationBlueYellowChanged(f32),
    GlowAmountChanged(f32),
    HalationAmountChanged(f32),
    FlareAmountChanged(f32),
    VignetteAmountChanged(f32),
    VignetteMidpointChanged(f32),
    VignetteRoundnessChanged(f32),
    VignetteFeatherChanged(f32),
    GrainAmountChanged(f32),
    GrainSizeChanged(f32),
    GrainRoughnessChanged(f32),
    SetRating(u8),
    SelectLut,
    SelectLutFolder,
    LutFolderLoaded(Result<LutBrowserState, String>),
    ClearLut,
    LutIntensityChanged(f32),
    HoverLut(Option<usize>),
    ApplyLutFromBrowser(usize),
    SelectSidebarPage(SidebarPage),
    CropPresetSelected(CropPresetKind),
    CropCustomWidthChanged(String),
    CropCustomHeightChanged(String),
    ApplyCustomCropRatio,
    InvertCropAspectRatio,
    RotateLeft,
    RotateRight,
    ToggleFlipHorizontal,
    ToggleFlipVertical,
    CropRotationChanged(f32),
    ApplyRulerRotation(f32),
    ToggleCropRuler,
    ResetCropRotation,
    CropOverlayChanged(CropRect),
    ResetCropTransform,
    ExportFormatChanged(ExportFileFormat),
    ExportJpegQualityChanged(f32),
    ExportResizeEnabledChanged(bool),
    ExportResizeModeChanged(ExportResizeMode),
    ExportResizeValueChanged(f32),
    ExportDontEnlargeChanged(bool),
    ExportKeepMetadataChanged(bool),
    ExportStripGpsChanged(bool),
    ExportMasksChanged(bool),
    ExportWatermarkEnabledChanged(bool),
    ExportWatermarkPathChanged(String),
    ExportWatermarkAnchorChanged(WatermarkAnchor),
    ExportWatermarkScaleChanged(f32),
    ExportWatermarkSpacingChanged(f32),
    ExportWatermarkOpacityChanged(f32),
    TriggerExport,
    ExportFinished(Result<ExportOutcome, String>),
    ToneMapperChanged(ToneMapper),
    ActiveCurveChannelChanged(CurveChannel),
    CurveChanged(CurveChannel, Vec<CurvePoint>),
    ResetCurveChannel(CurveChannel),
    ResetBasicAdjustments,
    ActiveHslBandChanged(HslBand),
    HslHueChanged(f32),
    HslSaturationChanged(f32),
    HslLuminanceChanged(f32),
    ColorGradingWheelChanged(ColorGradingZone, HueSatLum),
    ColorGradingZoneLuminanceChanged(ColorGradingZone, f32),
    ColorGradingBlendingChanged(f32),
    ColorGradingBalanceChanged(f32),
    ResetColorAdjustments,
    ResetDetailsAdjustments,
    ResetEffectsAdjustments,
    CommitPreviewRender,
    PreviewRendered {
        generation: u64,
        result: Result<RenderedPreview, String>,
    },
    ThumbnailsRendered {
        results: Vec<(usize, Result<RenderedThumbnail, String>)>,
    },
}

struct App {
    route: Route,
    samples: Vec<SampleImage>,
    selected_index: usize,
    selected_indices: BTreeSet<usize>,
    shift_pressed: bool,
    command_pressed: bool,
    basic_card: CardAnimation,
    curves_card: CardAnimation,
    color_card: CardAnimation,
    details_card: CardAnimation,
    effects_card: CardAnimation,
    active_curve_channel: CurveChannel,
    active_hsl_band: HslBand,
    active_color_grading_zone: ColorGradingZone,
    current_folder: Option<PathBuf>,
    is_loading: bool,
    status_message: Option<String>,
    basic_adjustments: BasicAdjustments,
    crop_custom_width: String,
    crop_custom_height: String,
    crop_ruler_active: bool,
    lut_browser: LutBrowserState,
    rendered_preview: Option<image::Handle>,
    rendered_preview_size: Option<(u32, u32)>,
    preview_generation: u64,
    is_rendering_preview: bool,
    pending_preview_quality: Option<PreviewQuality>,
    renderer: Option<RapidRawRenderer>,
    is_exporting: bool,
    undo_stack: Vec<UndoEntry>,
    pending_drag_undo: Option<UndoEntry>,
    sidebar_page: SidebarPage,
    export_settings: ExportSettingsUi,
    export_toggle_animations: ExportToggleAnimations,
}

#[derive(Debug, Clone)]
struct UndoEntry {
    changes: Vec<UndoChange>,
}

#[derive(Debug, Clone)]
struct UndoChange {
    index: usize,
    adjustments: BasicAdjustments,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UndoBehavior {
    Immediate,
    Coalesced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarPage {
    Crop,
    Adjustments,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CropPresetKind {
    Free,
    Original,
    Ratio(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportFileFormat {
    Jpeg,
    Png,
    Tiff,
    Webp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportResizeMode {
    LongEdge,
    ShortEdge,
    Width,
    Height,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatermarkAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Debug, Clone)]
struct ExportSettingsUi {
    file_format: ExportFileFormat,
    jpeg_quality: f32,
    enable_resize: bool,
    resize_mode: ExportResizeMode,
    resize_value: f32,
    dont_enlarge: bool,
    keep_metadata: bool,
    strip_gps: bool,
    export_masks: bool,
    enable_watermark: bool,
    watermark_path: String,
    watermark_anchor: WatermarkAnchor,
    watermark_scale: f32,
    watermark_spacing: f32,
    watermark_opacity: f32,
}

impl Default for ExportSettingsUi {
    fn default() -> Self {
        Self {
            file_format: ExportFileFormat::Jpeg,
            jpeg_quality: 90.0,
            enable_resize: false,
            resize_mode: ExportResizeMode::LongEdge,
            resize_value: 2048.0,
            dont_enlarge: true,
            keep_metadata: true,
            strip_gps: true,
            export_masks: false,
            enable_watermark: false,
            watermark_path: String::new(),
            watermark_anchor: WatermarkAnchor::BottomRight,
            watermark_scale: 10.0,
            watermark_spacing: 5.0,
            watermark_opacity: 75.0,
        }
    }
}

#[derive(Debug, Clone)]
struct ExportToggleAnimations {
    enable_resize: f32,
    dont_enlarge: f32,
    keep_metadata: f32,
    strip_gps: f32,
    export_masks: f32,
    enable_watermark: f32,
}

#[derive(Debug, Clone)]
struct ExportJob {
    path: PathBuf,
    is_raw: bool,
    adjustments: BasicAdjustments,
}

#[derive(Debug, Clone)]
struct ExportOutcome {
    exported_count: usize,
    output_folder: PathBuf,
}

impl Default for ExportToggleAnimations {
    fn default() -> Self {
        let settings = ExportSettingsUi::default();
        Self {
            enable_resize: bool_to_progress(settings.enable_resize),
            dont_enlarge: bool_to_progress(settings.dont_enlarge),
            keep_metadata: bool_to_progress(settings.keep_metadata),
            strip_gps: bool_to_progress(settings.strip_gps),
            export_masks: bool_to_progress(settings.export_masks),
            enable_watermark: bool_to_progress(settings.enable_watermark),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct LutBrowserState {
    folder: Option<PathBuf>,
    entries: Vec<LutListEntry>,
    hovered_index: Option<usize>,
    collapsed: bool,
}

#[derive(Debug, Clone)]
struct LutListEntry {
    name: String,
    path: PathBuf,
    size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    Home,
    Editor,
}

#[derive(Debug, Clone)]
struct SampleImage {
    name: String,
    path: PathBuf,
    source_width: u32,
    source_height: u32,
    thumbnail_dimensions: (u32, u32),
    interactive_preview_image: Arc<DynamicImage>,
    full_preview_image: Arc<DynamicImage>,
    preview: image::Handle,
    thumbnail: image::Handle,
    is_raw: bool,
    rating: u8,
    adjustments: BasicAdjustments,
    histogram: HistogramData,
}

#[derive(Debug, Clone)]
struct LoadedFolder {
    path: PathBuf,
    samples: Vec<SampleImage>,
}

#[derive(Debug, Clone)]
struct RenderedPreview {
    handle: image::Handle,
    width: u32,
    height: u32,
    changed: bool,
    updates_thumbnail: bool,
}

#[derive(Debug, Clone)]
struct RenderedThumbnail {
    handle: image::Handle,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone)]
struct ThumbnailCanvas {
    handle: image::Handle,
    corner_radius: f32,
}

#[derive(Debug, Clone, Copy)]
struct CropOverlay {
    crop: CropRect,
    image_size: (u32, u32),
    locked_ratio: Option<f32>,
    ruler_active: bool,
}

#[derive(Debug, Default, Clone, Copy)]
struct CropOverlayState {
    drag_mode: Option<CropDragMode>,
    last_position: Option<Point>,
    ruler_start: Option<Point>,
    ruler_end: Option<Point>,
}

#[derive(Debug, Clone, Copy)]
enum CropDragMode {
    Move,
    Handle(usize),
    Ruler,
}

#[derive(Debug, Clone, Copy)]
enum AppIcon {
    ArrowLeft,
    Check,
    ChevronDown,
    ChevronUp,
    Crop,
    SlidersHorizontal,
    Share,
    FolderOpen,
    RotateCcw,
    RotateCw,
    Ruler,
    FlipHorizontal,
    FlipVertical,
    RectangleHorizontal,
    RectangleVertical,
    Star,
    X,
}

const FILMSTRIP_CARD_RADIUS: f32 = 14.0;
const FILMSTRIP_CARD_PADDING: f32 = 6.0;
const FILMSTRIP_IMAGE_RADIUS: f32 = 24.0;
const FILMSTRIP_IMAGE_RADIUS_PX: u32 = FILMSTRIP_IMAGE_RADIUS as u32;
const APP_SPACING: f32 = 10.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImageMetadata {
    version: u32,
    rating: u8,
    adjustments: Value,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

impl Default for ImageMetadata {
    fn default() -> Self {
        Self {
            version: 1,
            rating: 0,
            adjustments: Value::Null,
            tags: None,
        }
    }
}

impl<Message> canvas::Program<Message> for ThumbnailCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let background = canvas::Path::rounded_rectangle(
            Point::ORIGIN,
            bounds.size(),
            self.corner_radius.into(),
        );

        frame.fill(&background, Color::from_rgb8(0x12, 0x16, 0x20));
        frame.with_clip(
            Rectangle {
                x: 0.0,
                y: 0.0,
                width: bounds.width,
                height: bounds.height,
            },
            |frame| {
                frame.draw_image(
                    Rectangle {
                        x: 0.0,
                        y: 0.0,
                        width: bounds.width,
                        height: bounds.height,
                    },
                    canvas::Image::new(self.handle.clone()),
                );
            },
        );

        vec![frame.into_geometry()]
    }
}

impl canvas::Program<Message> for CropOverlay {
    type State = CropOverlayState;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        let Some(position) = cursor.position_in(bounds) else {
            if matches!(
                event,
                canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            ) {
                state.drag_mode = None;
                state.last_position = None;
                return (
                    canvas::event::Status::Captured,
                    Some(Message::CommitPreviewRender),
                );
            }
            return (canvas::event::Status::Ignored, None);
        };

        let image_rect = fitted_rect(bounds.size(), self.image_size);
        let crop_rect = crop_overlay_rect_in_bounds(self.crop, self.image_size, bounds.size());

        if self.ruler_active {
            match event {
                canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    if point_in_rect(position, image_rect) {
                        let clamped = clamp_point_to_rect(position, image_rect);
                        state.drag_mode = Some(CropDragMode::Ruler);
                        state.last_position = Some(clamped);
                        state.ruler_start = Some(clamped);
                        state.ruler_end = Some(clamped);
                        return (canvas::event::Status::Captured, None);
                    }
                    return (canvas::event::Status::Ignored, None);
                }
                canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    if matches!(state.drag_mode, Some(CropDragMode::Ruler)) {
                        let clamped = clamp_point_to_rect(position, image_rect);
                        state.last_position = Some(clamped);
                        state.ruler_end = Some(clamped);
                        return (canvas::event::Status::Captured, None);
                    }
                }
                canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    if matches!(state.drag_mode, Some(CropDragMode::Ruler)) {
                        state.drag_mode = None;
                        state.last_position = None;
                        let rotation = state
                            .ruler_start
                            .zip(state.ruler_end)
                            .and_then(|(start, end)| ruler_rotation_from_line(start, end));
                        state.ruler_start = None;
                        state.ruler_end = None;
                        return (
                            canvas::event::Status::Captured,
                            rotation.map(Message::ApplyRulerRotation),
                        );
                    }
                }
                _ => {}
            }
        }

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(handle_index) = crop_handle_rects(crop_rect)
                    .iter()
                    .position(|handle| point_in_rect(position, *handle))
                {
                    state.drag_mode = Some(CropDragMode::Handle(handle_index));
                    state.last_position = Some(position);
                    return (canvas::event::Status::Captured, None);
                }

                if point_in_rect(position, crop_rect) {
                    state.drag_mode = Some(CropDragMode::Move);
                    state.last_position = Some(position);
                    return (canvas::event::Status::Captured, None);
                }

                (canvas::event::Status::Ignored, None)
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(last_position) = state.last_position else {
                    return (canvas::event::Status::Ignored, None);
                };
                let Some(drag_mode) = state.drag_mode else {
                    return (canvas::event::Status::Ignored, None);
                };

                let delta = Point::new(position.x - last_position.x, position.y - last_position.y);
                let updated_rect = match drag_mode {
                    CropDragMode::Move => move_crop_overlay_rect(crop_rect, image_rect, delta),
                    CropDragMode::Handle(handle) => {
                        resize_crop_overlay_rect(
                            crop_rect,
                            image_rect,
                            handle,
                            delta,
                            self.locked_ratio,
                        )
                    }
                    CropDragMode::Ruler => crop_rect,
                };
                state.last_position = Some(position);
                return (
                    canvas::event::Status::Captured,
                    Some(Message::CropOverlayChanged(
                        crop_rect_from_overlay_rect(updated_rect, self.image_size, bounds.size()),
                    )),
                );
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.drag_mode.is_some() {
                    state.drag_mode = None;
                    state.last_position = None;
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::CommitPreviewRender),
                    );
                }
                (canvas::event::Status::Ignored, None)
            }
            _ => (canvas::event::Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let image_rect = fitted_rect(bounds.size(), self.image_size);

        let scale_x = image_rect.width / self.image_size.0.max(1) as f32;
        let scale_y = image_rect.height / self.image_size.1.max(1) as f32;
        let crop_rect = Rectangle {
            x: image_rect.x + self.crop.x * scale_x,
            y: image_rect.y + self.crop.y * scale_y,
            width: self.crop.width * scale_x,
            height: self.crop.height * scale_y,
        };

        let dim_color = Color::from_rgba8(0x05, 0x09, 0x12, 0.55);
        let top = canvas::Path::rectangle(
            Point::new(image_rect.x, image_rect.y),
            Size::new(image_rect.width, (crop_rect.y - image_rect.y).max(0.0)),
        );
        let bottom = canvas::Path::rectangle(
            Point::new(image_rect.x, (crop_rect.y + crop_rect.height).min(image_rect.y + image_rect.height)),
            Size::new(
                image_rect.width,
                (image_rect.y + image_rect.height - crop_rect.y - crop_rect.height).max(0.0),
            ),
        );
        let left = canvas::Path::rectangle(
            Point::new(image_rect.x, crop_rect.y.max(image_rect.y)),
            Size::new((crop_rect.x - image_rect.x).max(0.0), crop_rect.height.max(0.0)),
        );
        let right = canvas::Path::rectangle(
            Point::new((crop_rect.x + crop_rect.width).min(image_rect.x + image_rect.width), crop_rect.y.max(image_rect.y)),
            Size::new(
                (image_rect.x + image_rect.width - crop_rect.x - crop_rect.width).max(0.0),
                crop_rect.height.max(0.0),
            ),
        );
        for path in [&top, &bottom, &left, &right] {
            frame.fill(path, dim_color);
        }

        let crop_outline = canvas::Path::rectangle(
            Point::new(crop_rect.x, crop_rect.y),
            Size::new(crop_rect.width, crop_rect.height),
        );
        frame.stroke(
            &crop_outline,
            canvas::Stroke::default()
                .with_color(Color::WHITE)
                .with_width(1.5),
        );

        for step in [1.0_f32 / 3.0, 2.0 / 3.0] {
            let vertical = canvas::Path::line(
                Point::new(crop_rect.x + crop_rect.width * step, crop_rect.y),
                Point::new(crop_rect.x + crop_rect.width * step, crop_rect.y + crop_rect.height),
            );
            let horizontal = canvas::Path::line(
                Point::new(crop_rect.x, crop_rect.y + crop_rect.height * step),
                Point::new(crop_rect.x + crop_rect.width, crop_rect.y + crop_rect.height * step),
            );
            frame.stroke(
                &vertical,
                canvas::Stroke::default()
                    .with_color(Color::from_rgba8(0xff, 0xff, 0xff, 0.30))
                    .with_width(1.0),
            );
            frame.stroke(
                &horizontal,
                canvas::Stroke::default()
                    .with_color(Color::from_rgba8(0xff, 0xff, 0xff, 0.30))
                    .with_width(1.0),
            );
        }

        for handle in crop_handle_rects(crop_rect) {
            let path = canvas::Path::rounded_rectangle(
                Point::new(handle.x, handle.y),
                Size::new(handle.width, handle.height),
                4.0.into(),
            );
            frame.fill(&path, Color::WHITE);
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb8(0x0d, 0x12, 0x1b))
                    .with_width(1.0),
            );
        }

        if let Some((start, end)) = state.ruler_start.zip(state.ruler_end) {
            let line = canvas::Path::line(start, end);
            frame.stroke(
                &line,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb8(0xf8, 0xfb, 0xff))
                    .with_width(2.0),
            );

            for point in [start, end] {
                let marker =
                    canvas::Path::circle(point, 5.0);
                frame.fill(&marker, Color::from_rgb8(0xf8, 0xfb, 0xff));
                frame.stroke(
                    &marker,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb8(0x0d, 0x12, 0x1b))
                        .with_width(1.5),
                );
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if matches!(state.drag_mode, Some(CropDragMode::Ruler)) {
            return mouse::Interaction::Crosshair;
        }

        if state.drag_mode.is_some() {
            return mouse::Interaction::Pointer;
        }

        let Some(position) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        let image_rect = fitted_rect(bounds.size(), self.image_size);
        if self.ruler_active && point_in_rect(position, image_rect) {
            return mouse::Interaction::Crosshair;
        }
        let crop_rect = crop_overlay_rect_in_bounds(self.crop, self.image_size, bounds.size());
        if point_in_rect(position, crop_rect)
            || crop_handle_rects(crop_rect)
                .iter()
                .any(|handle| point_in_rect(position, *handle))
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PreviewQuality {
    Interactive,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CurveChannel {
    Luma,
    Red,
    Green,
    Blue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HslBand {
    Reds,
    Oranges,
    Yellows,
    Greens,
    Aquas,
    Blues,
    Purples,
    Magentas,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorGradingZone {
    Shadows,
    Midtones,
    Highlights,
}

#[derive(Debug, Clone)]
struct HistogramData {
    luma: [u32; 256],
    red: [u32; 256],
    green: [u32; 256],
    blue: [u32; 256],
}

#[derive(Debug, Default)]
struct CurveEditorState {
    dragging_index: Option<usize>,
}

#[derive(Debug, Default)]
struct ColorWheelState {
    dragging: bool,
}

#[derive(Debug, Clone, Copy)]
struct CardAnimation {
    expanded: bool,
    progress: f32,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let samples = load_samples();
        let initial_thumbnail_indices = (0..samples.len()).collect::<Vec<_>>();
        let initial_selected_indices = if samples.is_empty() {
            BTreeSet::new()
        } else {
            BTreeSet::from([0])
        };
        let renderer = RapidRawRenderer::new().ok();
        let status_message = if renderer.is_none() {
            Some("GPU preview renderer could not be initialized.".to_string())
        } else {
            None
        };

        let mut app = Self {
            route: Route::Home,
            samples,
            selected_index: 0,
            selected_indices: initial_selected_indices,
            shift_pressed: false,
            command_pressed: false,
            basic_card: CardAnimation {
                expanded: true,
                progress: 1.0,
            },
            curves_card: CardAnimation {
                expanded: false,
                progress: 0.0,
            },
            color_card: CardAnimation {
                expanded: false,
                progress: 0.0,
            },
            details_card: CardAnimation {
                expanded: false,
                progress: 0.0,
            },
            effects_card: CardAnimation {
                expanded: false,
                progress: 0.0,
            },
            active_curve_channel: CurveChannel::Luma,
            active_hsl_band: HslBand::Reds,
            active_color_grading_zone: ColorGradingZone::Midtones,
            current_folder: None,
            is_loading: false,
            status_message,
            basic_adjustments: BasicAdjustments::default(),
            crop_custom_width: String::new(),
            crop_custom_height: String::new(),
            crop_ruler_active: false,
            lut_browser: LutBrowserState::default(),
            rendered_preview: None,
            rendered_preview_size: None,
            preview_generation: 0,
            is_rendering_preview: false,
            pending_preview_quality: None,
            renderer,
            is_exporting: false,
            undo_stack: Vec::new(),
            pending_drag_undo: None,
            sidebar_page: SidebarPage::Adjustments,
            export_settings: ExportSettingsUi::default(),
            export_toggle_animations: ExportToggleAnimations::default(),
        };

        if let Some(sample) = app.samples.first() {
            app.basic_adjustments = sample.adjustments.clone();
            app.sync_crop_custom_inputs();
        }

        let task = if initial_thumbnail_indices.is_empty() {
            Task::none()
        } else {
            app.request_thumbnail_render(initial_thumbnail_indices)
        };

        (app, task)
    }

    fn theme(&self) -> Theme {
        Theme::TokyoNightStorm
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        let modifiers = iced::event::listen_with(|event, _status, _window| match event {
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                Some(Message::ModifiersChanged(modifiers))
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if modifiers.command() && matches_undo_key(&key) =>
            {
                Some(Message::UndoRequested)
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if modifiers.command() && matches_select_all_key(&key) =>
            {
                Some(Message::SelectAllImages)
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if !modifiers.command() && !modifiers.alt() && !modifiers.control() =>
            {
                rating_from_key(&key)
                    .map(Message::SetRating)
                    .or_else(|| arrow_navigation_message(&key))
            }
            _ => None,
        });

        if self.is_animating_cards() || self.is_animating_toggles() {
            Subscription::batch(vec![
                modifiers,
                window::frames().map(Message::AnimationFrame),
            ])
        } else {
            modifiers
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::EnterEditor => {
                self.finish_pending_drag_undo();
                if !self.samples.is_empty() {
                    self.route = Route::Editor;
                    self.basic_adjustments = self
                        .samples
                        .get(self.selected_index)
                        .map(|sample| sample.adjustments.clone())
                        .unwrap_or_default();
                    self.sync_crop_custom_inputs();
                    self.rendered_preview = None;
                    self.rendered_preview_size = None;
                    return self.request_preview_render(PreviewQuality::Full);
                }
            }
            Message::BackToHome => {
                self.finish_pending_drag_undo();
                self.crop_ruler_active = false;
                self.route = Route::Home;
            }
            Message::OpenFolder => {
                self.finish_pending_drag_undo();
                self.crop_ruler_active = false;
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.is_loading = true;
                    self.status_message =
                        Some(format!("Loading images from {}...", path.display()));
                    return Task::perform(load_folder_task(path), Message::FolderLoaded);
                }
            }
            Message::FolderLoaded(result) => {
                self.is_loading = false;
                match result {
                    Ok(folder) => {
                        self.current_folder = Some(folder.path.clone());
                        self.samples = folder.samples;
                        self.selected_index = 0;
                        self.selected_indices = if self.samples.is_empty() {
                            BTreeSet::new()
                        } else {
                            BTreeSet::from([0])
                        };
                        self.rendered_preview = None;
                        self.rendered_preview_size = None;
                        self.pending_preview_quality = None;
                        self.basic_adjustments = self
                            .samples
                            .first()
                            .map(|sample| sample.adjustments.clone())
                            .unwrap_or_default();
                        self.sync_crop_custom_inputs();
                        self.route = if self.samples.is_empty() {
                            Route::Home
                        } else {
                            Route::Editor
                        };
                        self.status_message = if self.samples.is_empty() {
                            Some(
                                "The selected folder did not contain any supported image files."
                                    .to_string(),
                            )
                        } else {
                            Some(format!(
                                "Loaded {} image{} from {}",
                                self.samples.len(),
                                if self.samples.len() == 1 { "" } else { "s" },
                                self.current_folder
                                    .as_ref()
                                    .map(|path| path.display().to_string())
                                    .unwrap_or_default()
                            ))
                        };
                        if !self.samples.is_empty() {
                            return Task::batch(vec![
                                self.request_preview_render(PreviewQuality::Full),
                                self.request_thumbnail_render((0..self.samples.len()).collect()),
                            ]);
                        }
                    }
                    Err(error) => {
                        self.status_message = Some(error);
                    }
                }
            }
            Message::ModifiersChanged(modifiers) => {
                self.shift_pressed = modifiers.shift();
                self.command_pressed = modifiers.command();
            }
            Message::SelectSidebarPage(page) => {
                if self.sidebar_page != page {
                    self.finish_pending_drag_undo();
                    if self.sidebar_page == SidebarPage::Crop && page != SidebarPage::Crop {
                        self.crop_ruler_active = false;
                    }
                    self.sidebar_page = page;
                    self.rendered_preview = None;
                    self.rendered_preview_size = None;
                    if !self.samples.is_empty() {
                        return self.request_preview_render(PreviewQuality::Full);
                    }
                }
            }
            Message::CropPresetSelected(preset) => {
                let new_ratio = match preset {
                    CropPresetKind::Free => None,
                    CropPresetKind::Original => self.active_original_crop_ratio(),
                    CropPresetKind::Ratio(value) => Some(value),
                };
                self.apply_crop_ratio_to_selected(new_ratio);
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::CropCustomWidthChanged(value) => {
                self.crop_custom_width = value;
            }
            Message::CropCustomHeightChanged(value) => {
                self.crop_custom_height = value;
            }
            Message::ApplyCustomCropRatio => {
                if let Some(new_ratio) = parse_custom_crop_ratio(
                    &self.crop_custom_width,
                    &self.crop_custom_height,
                ) {
                    self.apply_crop_ratio_to_selected(Some(new_ratio));
                    return self.request_preview_render(PreviewQuality::Full);
                }
            }
            Message::InvertCropAspectRatio => {
                if let Some(ratio) = self.basic_adjustments.aspect_ratio
                    && ratio > 0.0
                {
                    self.apply_crop_ratio_to_selected(Some(1.0 / ratio));
                    return self.request_preview_render(PreviewQuality::Full);
                }
            }
            Message::RotateLeft => {
                self.rotate_selected_images(-1);
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::RotateRight => {
                self.rotate_selected_images(1);
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::ToggleFlipHorizontal => {
                let value = !self.basic_adjustments.flip_horizontal;
                self.basic_adjustments.flip_horizontal = value;
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    adjustments.flip_horizontal = value;
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::ToggleFlipVertical => {
                let value = !self.basic_adjustments.flip_vertical;
                self.basic_adjustments.flip_vertical = value;
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    adjustments.flip_vertical = value;
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::CropRotationChanged(value) => {
                self.basic_adjustments.rotation = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.rotation = value;
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ApplyRulerRotation(value) => {
                let value = (self.basic_adjustments.rotation + value).clamp(-45.0, 45.0);
                self.crop_ruler_active = false;
                self.basic_adjustments.rotation = value;
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    adjustments.rotation = value;
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::ToggleCropRuler => {
                self.crop_ruler_active = !self.crop_ruler_active;
            }
            Message::ResetCropRotation => {
                self.crop_ruler_active = false;
                self.basic_adjustments.rotation = 0.0;
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    adjustments.rotation = 0.0;
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::CropOverlayChanged(crop) => {
                self.basic_adjustments.crop = Some(crop);
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.crop = Some(crop);
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ResetCropTransform => {
                self.crop_ruler_active = false;
                self.basic_adjustments.aspect_ratio = None;
                self.basic_adjustments.crop = None;
                self.basic_adjustments.rotation = 0.0;
                self.basic_adjustments.flip_horizontal = false;
                self.basic_adjustments.flip_vertical = false;
                self.basic_adjustments.orientation_steps = 0;
                self.sync_crop_custom_inputs();
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    adjustments.aspect_ratio = None;
                    adjustments.crop = None;
                    adjustments.rotation = 0.0;
                    adjustments.flip_horizontal = false;
                    adjustments.flip_vertical = false;
                    adjustments.orientation_steps = 0;
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::ExportFormatChanged(format) => {
                self.export_settings.file_format = format;
            }
            Message::ExportJpegQualityChanged(value) => {
                self.export_settings.jpeg_quality = value;
            }
            Message::ExportResizeEnabledChanged(enabled) => {
                self.export_settings.enable_resize = enabled;
            }
            Message::ExportResizeModeChanged(mode) => {
                self.export_settings.resize_mode = mode;
            }
            Message::ExportResizeValueChanged(value) => {
                self.export_settings.resize_value = value;
            }
            Message::ExportDontEnlargeChanged(enabled) => {
                self.export_settings.dont_enlarge = enabled;
            }
            Message::ExportKeepMetadataChanged(enabled) => {
                self.export_settings.keep_metadata = enabled;
            }
            Message::ExportStripGpsChanged(enabled) => {
                self.export_settings.strip_gps = enabled;
            }
            Message::ExportMasksChanged(enabled) => {
                self.export_settings.export_masks = enabled;
            }
            Message::ExportWatermarkEnabledChanged(enabled) => {
                self.export_settings.enable_watermark = enabled;
            }
            Message::ExportWatermarkPathChanged(value) => {
                self.export_settings.watermark_path = value;
            }
            Message::ExportWatermarkAnchorChanged(anchor) => {
                self.export_settings.watermark_anchor = anchor;
            }
            Message::ExportWatermarkScaleChanged(value) => {
                self.export_settings.watermark_scale = value;
            }
            Message::ExportWatermarkSpacingChanged(value) => {
                self.export_settings.watermark_spacing = value;
            }
            Message::ExportWatermarkOpacityChanged(value) => {
                self.export_settings.watermark_opacity = value;
            }
            Message::TriggerExport => {
                self.finish_pending_drag_undo();
                if self.is_exporting {
                    return Task::none();
                }

                let indices = if self.selected_indices.is_empty() {
                    vec![self.selected_index]
                } else {
                    self.selected_indices.iter().copied().collect::<Vec<_>>()
                };

                if indices.is_empty() {
                    self.status_message = Some("No images selected for export.".to_string());
                    return Task::none();
                }

                let Some(renderer) = self.renderer.clone() else {
                    self.status_message = Some("GPU renderer is unavailable for export.".to_string());
                    return Task::none();
                };

                let Some(output_folder) = rfd::FileDialog::new()
                    .set_title("Choose Export Folder")
                    .pick_folder()
                else {
                    self.status_message = Some("Export canceled.".to_string());
                    return Task::none();
                };

                let jobs = indices
                    .into_iter()
                    .filter_map(|index| {
                        self.samples.get(index).map(|sample| ExportJob {
                            path: sample.path.clone(),
                            is_raw: sample.is_raw,
                            adjustments: sample.adjustments.clone(),
                        })
                    })
                    .collect::<Vec<_>>();

                let export_count = jobs.len();
                if export_count == 0 {
                    self.status_message = Some("No images selected for export.".to_string());
                    return Task::none();
                }

                self.is_exporting = true;
                self.status_message = Some(format!(
                    "Exporting {} image{}...",
                    export_count,
                    if export_count == 1 { "" } else { "s" }
                ));

                return Task::perform(
                    export_images_task(
                        output_folder,
                        jobs,
                        self.export_settings.clone(),
                        renderer,
                    ),
                    Message::ExportFinished,
                );
            }
            Message::ExportFinished(result) => {
                self.is_exporting = false;
                match result {
                    Ok(outcome) => {
                        self.status_message = Some(format!(
                            "Exported {} image{} to {}.",
                            outcome.exported_count,
                            if outcome.exported_count == 1 { "" } else { "s" },
                            outcome.output_folder.display()
                        ));
                    }
                    Err(error) => {
                        self.status_message = Some(error);
                    }
                }
            }
            Message::SelectAllImages => {
                self.finish_pending_drag_undo();
                if self.route == Route::Editor && !self.samples.is_empty() {
                    self.selected_indices = (0..self.samples.len()).collect();
                    self.selected_index = 0;
                    self.basic_adjustments = self.samples[0].adjustments.clone();
                    self.sync_crop_custom_inputs();
                    self.rendered_preview = None;
                    self.rendered_preview_size = None;
                    self.pending_preview_quality = None;
                    self.status_message = Some(format!("Selected {} images.", self.samples.len()));
                    return self.request_preview_render(PreviewQuality::Full);
                }
            }
            Message::SetRating(rating) => {
                self.finish_pending_drag_undo();
                let indices = if self.selected_indices.is_empty() {
                    vec![self.selected_index]
                } else {
                    self.selected_indices.iter().copied().collect::<Vec<_>>()
                };

                for index in indices {
                    if let Some(sample) = self.samples.get_mut(index) {
                        sample.rating = rating;
                        if let Err(error) = save_sample_adjustments(sample) {
                            self.status_message = Some(error);
                        }
                    }
                }

                self.status_message = Some(if rating == 0 {
                    "Cleared rating.".to_string()
                } else {
                    format!(
                        "Set rating to {rating} star{}.",
                        if rating == 1 { "" } else { "s" }
                    )
                });
            }
            Message::UndoRequested => {
                self.finish_pending_drag_undo();
                if let Some(entry) = self.undo_stack.pop() {
                    let indices = entry
                        .changes
                        .iter()
                        .map(|change| change.index)
                        .collect::<Vec<_>>();
                    for change in entry.changes {
                        if let Some(sample) = self.samples.get_mut(change.index) {
                            sample.adjustments = change.adjustments;
                            if let Err(error) = save_sample_adjustments(sample) {
                                self.status_message = Some(error);
                            }
                        }
                    }
                    if let Some(sample) = self.samples.get(self.selected_index) {
                        self.basic_adjustments = sample.adjustments.clone();
                        self.sync_crop_custom_inputs();
                    }
                    self.lut_browser.hovered_index = None;
                    self.status_message = Some("Undid last adjustment.".to_string());
                    return Task::batch(vec![
                        self.request_preview_render(PreviewQuality::Full),
                        self.request_thumbnail_render(indices),
                    ]);
                }
            }
            Message::AnimationFrame(_instant) => {
                step_card_animation(&mut self.basic_card);
                step_card_animation(&mut self.curves_card);
                step_card_animation(&mut self.color_card);
                step_card_animation(&mut self.details_card);
                step_card_animation(&mut self.effects_card);
                step_export_toggle_animation(
                    &mut self.export_toggle_animations.enable_resize,
                    self.export_settings.enable_resize,
                );
                step_export_toggle_animation(
                    &mut self.export_toggle_animations.dont_enlarge,
                    self.export_settings.dont_enlarge,
                );
                step_export_toggle_animation(
                    &mut self.export_toggle_animations.keep_metadata,
                    self.export_settings.keep_metadata,
                );
                step_export_toggle_animation(
                    &mut self.export_toggle_animations.strip_gps,
                    self.export_settings.strip_gps,
                );
                step_export_toggle_animation(
                    &mut self.export_toggle_animations.export_masks,
                    self.export_settings.export_masks,
                );
                step_export_toggle_animation(
                    &mut self.export_toggle_animations.enable_watermark,
                    self.export_settings.enable_watermark,
                );
            }
            Message::ToggleBasicCard => {
                self.basic_card.expanded = !self.basic_card.expanded;
            }
            Message::ToggleCurvesCard => {
                self.curves_card.expanded = !self.curves_card.expanded;
            }
            Message::ToggleColorCard => {
                self.color_card.expanded = !self.color_card.expanded;
            }
            Message::ToggleDetailsCard => {
                self.details_card.expanded = !self.details_card.expanded;
            }
            Message::ToggleEffectsCard => {
                self.effects_card.expanded = !self.effects_card.expanded;
            }
            Message::SelectImage(index) => {
                self.finish_pending_drag_undo();
                self.crop_ruler_active = false;
                if index < self.samples.len() {
                    if self.shift_pressed {
                        self.selected_indices.clear();
                        let start = self.selected_index.min(index);
                        let end = self.selected_index.max(index);
                        self.selected_indices.extend(start..=end);
                    } else if self.command_pressed {
                        self.selected_indices.insert(index);
                    } else {
                        self.selected_indices.clear();
                        self.selected_indices.insert(index);
                    }
                    self.selected_index = index;
                    self.rendered_preview = None;
                    self.rendered_preview_size = None;
                    self.pending_preview_quality = None;
                    self.basic_adjustments = self.samples[index].adjustments.clone();
                    self.sync_crop_custom_inputs();
                    return self.request_preview_render(PreviewQuality::Full);
                }
            }
            Message::NavigateSelection(offset) => {
                self.finish_pending_drag_undo();
                self.crop_ruler_active = false;
                if self.route == Route::Editor && !self.samples.is_empty() {
                    let next_index = (self.selected_index as i32 + offset)
                        .clamp(0, self.samples.len().saturating_sub(1) as i32)
                        as usize;

                    if next_index != self.selected_index {
                        self.selected_indices.clear();
                        self.selected_indices.insert(next_index);
                        self.selected_index = next_index;
                        self.rendered_preview = None;
                        self.rendered_preview_size = None;
                        self.pending_preview_quality = None;
                        self.basic_adjustments = self.samples[next_index].adjustments.clone();
                        self.sync_crop_custom_inputs();
                        return self.request_preview_render(PreviewQuality::Full);
                    }
                }
            }
            Message::ExposureChanged(value) => {
                self.basic_adjustments.exposure = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.exposure = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::BrightnessChanged(value) => {
                self.basic_adjustments.brightness = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.brightness = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ContrastChanged(value) => {
                self.basic_adjustments.contrast = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.contrast = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::HighlightsChanged(value) => {
                self.basic_adjustments.highlights = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.highlights = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ShadowsChanged(value) => {
                self.basic_adjustments.shadows = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.shadows = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::WhitesChanged(value) => {
                self.basic_adjustments.whites = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.whites = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::BlacksChanged(value) => {
                self.basic_adjustments.blacks = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.blacks = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::TemperatureChanged(value) => {
                self.basic_adjustments.temperature = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.temperature = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::TintChanged(value) => {
                self.basic_adjustments.tint = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.tint = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::VibranceChanged(value) => {
                self.basic_adjustments.vibrance = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.vibrance = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::SaturationChanged(value) => {
                self.basic_adjustments.saturation = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.saturation = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::SharpnessChanged(value) => {
                self.basic_adjustments.sharpness = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.sharpness = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ClarityChanged(value) => {
                self.basic_adjustments.clarity = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.clarity = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::DehazeChanged(value) => {
                self.basic_adjustments.dehaze = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.dehaze = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::StructureChanged(value) => {
                self.basic_adjustments.structure = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.structure = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::CentreChanged(value) => {
                self.basic_adjustments.centre = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.centre = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ChromaticAberrationRedCyanChanged(value) => {
                self.basic_adjustments.chromatic_aberration_red_cyan = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.chromatic_aberration_red_cyan = value;
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ChromaticAberrationBlueYellowChanged(value) => {
                self.basic_adjustments.chromatic_aberration_blue_yellow = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.chromatic_aberration_blue_yellow = value;
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::GlowAmountChanged(value) => {
                self.basic_adjustments.glow_amount = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.glow_amount = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::HalationAmountChanged(value) => {
                self.basic_adjustments.halation_amount = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.halation_amount = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::FlareAmountChanged(value) => {
                self.basic_adjustments.flare_amount = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.flare_amount = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::VignetteAmountChanged(value) => {
                self.basic_adjustments.vignette_amount = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.vignette_amount = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::VignetteMidpointChanged(value) => {
                self.basic_adjustments.vignette_midpoint = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.vignette_midpoint = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::VignetteRoundnessChanged(value) => {
                self.basic_adjustments.vignette_roundness = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.vignette_roundness = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::VignetteFeatherChanged(value) => {
                self.basic_adjustments.vignette_feather = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.vignette_feather = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::GrainAmountChanged(value) => {
                self.basic_adjustments.grain_amount = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.grain_amount = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::GrainSizeChanged(value) => {
                self.basic_adjustments.grain_size = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.grain_size = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::GrainRoughnessChanged(value) => {
                self.basic_adjustments.grain_roughness = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.grain_roughness = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::SelectLut => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "LUT Files",
                        &["cube", "3dl", "png", "jpg", "jpeg", "tiff", "tif"],
                    )
                    .pick_file()
                {
                    match parse_lut_metadata(&path) {
                        Ok(lut_size) => {
                            let lut_name = path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("LUT")
                                .to_string();
                            let lut_path = path.to_string_lossy().to_string();
                            self.basic_adjustments.lut_path = Some(lut_path.clone());
                            self.basic_adjustments.lut_name = Some(lut_name.clone());
                            self.basic_adjustments.lut_size = lut_size;
                            self.update_selected_adjustments(
                                UndoBehavior::Immediate,
                                |adjustments| {
                                    adjustments.lut_path = Some(lut_path.clone());
                                    adjustments.lut_name = Some(lut_name.clone());
                                    adjustments.lut_size = lut_size;
                                },
                            );
                            self.status_message =
                                Some(format!("Loaded LUT {} ({lut_size}^3).", lut_name));
                            return self.request_preview_render(PreviewQuality::Full);
                        }
                        Err(error) => {
                            self.status_message = Some(format!("Failed to load LUT: {error}"));
                        }
                    }
                }
            }
            Message::SelectLutFolder => {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.status_message = Some(format!("Loading LUTs from {}...", path.display()));
                    return Task::perform(load_lut_folder_task(path), Message::LutFolderLoaded);
                }
            }
            Message::LutFolderLoaded(result) => match result {
                Ok(browser) => {
                    let count = browser.entries.len();
                    let folder_label = browser
                        .folder
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default();
                    self.lut_browser = browser;
                    self.status_message = Some(format!(
                        "Loaded {count} LUT{} from {folder_label}",
                        if count == 1 { "" } else { "s" }
                    ));
                }
                Err(error) => {
                    self.status_message = Some(error);
                }
            },
            Message::ClearLut => {
                self.basic_adjustments.lut_path = None;
                self.basic_adjustments.lut_name = None;
                self.basic_adjustments.lut_size = 0;
                self.basic_adjustments.lut_intensity = 100.0;
                self.lut_browser.hovered_index = None;
                self.lut_browser.collapsed = false;
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    adjustments.lut_path = None;
                    adjustments.lut_name = None;
                    adjustments.lut_size = 0;
                    adjustments.lut_intensity = 100.0;
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::LutIntensityChanged(value) => {
                self.basic_adjustments.lut_intensity = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.lut_intensity = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::HoverLut(index) => {
                if self.lut_browser.collapsed {
                    return Task::none();
                }
                if self.lut_browser.hovered_index != index {
                    self.lut_browser.hovered_index = index;
                    if !self.samples.is_empty() {
                        return self.request_preview_render(PreviewQuality::Interactive);
                    }
                }
            }
            Message::ApplyLutFromBrowser(index) => {
                if let Some(entry) = self.lut_browser.entries.get(index).cloned() {
                    let lut_path = entry.path.to_string_lossy().to_string();
                    let already_selected =
                        self.basic_adjustments.lut_path.as_deref() == Some(lut_path.as_str());
                    if already_selected {
                        self.lut_browser.collapsed = !self.lut_browser.collapsed;
                        self.lut_browser.hovered_index = None;
                        return Task::none();
                    }
                    self.basic_adjustments.lut_path = Some(lut_path.clone());
                    self.basic_adjustments.lut_name = Some(entry.name.clone());
                    self.basic_adjustments.lut_size = entry.size;
                    self.lut_browser.hovered_index = None;
                    self.lut_browser.collapsed = true;
                    self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                        adjustments.lut_path = Some(lut_path.clone());
                        adjustments.lut_name = Some(entry.name.clone());
                        adjustments.lut_size = entry.size;
                    });
                    self.status_message = Some(format!("Applied LUT {}.", entry.name));
                    return self.request_preview_render(PreviewQuality::Full);
                }
            }
            Message::ToneMapperChanged(value) => {
                self.basic_adjustments.tone_mapper = value;
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    adjustments.tone_mapper = value
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::ActiveCurveChannelChanged(channel) => {
                self.active_curve_channel = channel;
            }
            Message::CurveChanged(channel, points) => {
                let points = sanitize_curve_points(points);
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    curve_points_mut(&mut adjustments.curves, channel).clone_from(&points);
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ResetCurveChannel(channel) => {
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    *curve_points_mut(&mut adjustments.curves, channel) = default_curve_points();
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::ResetBasicAdjustments => {
                self.basic_adjustments = BasicAdjustments::default();
                self.sync_crop_custom_inputs();
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    *adjustments = BasicAdjustments::default()
                });
                if !self.samples.is_empty() {
                    return self.request_preview_render(PreviewQuality::Full);
                }
            }
            Message::ActiveHslBandChanged(band) => {
                self.active_hsl_band = band;
            }
            Message::HslHueChanged(value) => {
                let band = self.active_hsl_band;
                set_hsl_value(
                    hsl_band_mut(&mut self.basic_adjustments.hsl, band),
                    HslField::Hue,
                    value,
                );
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    set_hsl_value(
                        hsl_band_mut(&mut adjustments.hsl, band),
                        HslField::Hue,
                        value,
                    );
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::HslSaturationChanged(value) => {
                let band = self.active_hsl_band;
                set_hsl_value(
                    hsl_band_mut(&mut self.basic_adjustments.hsl, band),
                    HslField::Saturation,
                    value,
                );
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    set_hsl_value(
                        hsl_band_mut(&mut adjustments.hsl, band),
                        HslField::Saturation,
                        value,
                    );
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::HslLuminanceChanged(value) => {
                let band = self.active_hsl_band;
                set_hsl_value(
                    hsl_band_mut(&mut self.basic_adjustments.hsl, band),
                    HslField::Luminance,
                    value,
                );
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    set_hsl_value(
                        hsl_band_mut(&mut adjustments.hsl, band),
                        HslField::Luminance,
                        value,
                    );
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ColorGradingWheelChanged(zone, value) => {
                self.active_color_grading_zone = zone;
                *color_grading_zone_mut(&mut self.basic_adjustments.color_grading, zone) = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    *color_grading_zone_mut(&mut adjustments.color_grading, zone) = value;
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ColorGradingZoneLuminanceChanged(zone, value) => {
                self.active_color_grading_zone = zone;
                color_grading_zone_mut(&mut self.basic_adjustments.color_grading, zone).luminance =
                    value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    color_grading_zone_mut(&mut adjustments.color_grading, zone).luminance = value;
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ColorGradingBlendingChanged(value) => {
                self.basic_adjustments.color_grading.blending = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.color_grading.blending = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ColorGradingBalanceChanged(value) => {
                self.basic_adjustments.color_grading.balance = value;
                self.update_selected_adjustments(UndoBehavior::Coalesced, |adjustments| {
                    adjustments.color_grading.balance = value
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ResetColorAdjustments => {
                let hsl = HslSettings::default();
                let grading = ColorGradingSettingsUi::default();
                let calibration = ColorCalibrationSettingsUi::default();
                self.basic_adjustments.temperature = 0.0;
                self.basic_adjustments.tint = 0.0;
                self.basic_adjustments.vibrance = 0.0;
                self.basic_adjustments.saturation = 0.0;
                self.basic_adjustments.hsl = hsl.clone();
                self.basic_adjustments.color_grading = grading.clone();
                self.basic_adjustments.color_calibration = calibration.clone();
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    adjustments.temperature = 0.0;
                    adjustments.tint = 0.0;
                    adjustments.vibrance = 0.0;
                    adjustments.saturation = 0.0;
                    adjustments.hsl = hsl.clone();
                    adjustments.color_grading = grading.clone();
                    adjustments.color_calibration = calibration.clone();
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::ResetDetailsAdjustments => {
                self.basic_adjustments.sharpness = 0.0;
                self.basic_adjustments.clarity = 0.0;
                self.basic_adjustments.dehaze = 0.0;
                self.basic_adjustments.structure = 0.0;
                self.basic_adjustments.centre = 0.0;
                self.basic_adjustments.chromatic_aberration_red_cyan = 0.0;
                self.basic_adjustments.chromatic_aberration_blue_yellow = 0.0;
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    adjustments.sharpness = 0.0;
                    adjustments.clarity = 0.0;
                    adjustments.dehaze = 0.0;
                    adjustments.structure = 0.0;
                    adjustments.centre = 0.0;
                    adjustments.chromatic_aberration_red_cyan = 0.0;
                    adjustments.chromatic_aberration_blue_yellow = 0.0;
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::ResetEffectsAdjustments => {
                self.basic_adjustments.glow_amount = 0.0;
                self.basic_adjustments.halation_amount = 0.0;
                self.basic_adjustments.flare_amount = 0.0;
                self.basic_adjustments.lut_path = None;
                self.basic_adjustments.lut_name = None;
                self.basic_adjustments.lut_size = 0;
                self.basic_adjustments.lut_intensity = 100.0;
                self.basic_adjustments.vignette_amount = 0.0;
                self.basic_adjustments.vignette_midpoint = 50.0;
                self.basic_adjustments.vignette_roundness = 0.0;
                self.basic_adjustments.vignette_feather = 50.0;
                self.basic_adjustments.grain_amount = 0.0;
                self.basic_adjustments.grain_size = 25.0;
                self.basic_adjustments.grain_roughness = 50.0;
                self.update_selected_adjustments(UndoBehavior::Immediate, |adjustments| {
                    adjustments.glow_amount = 0.0;
                    adjustments.halation_amount = 0.0;
                    adjustments.flare_amount = 0.0;
                    adjustments.lut_path = None;
                    adjustments.lut_name = None;
                    adjustments.lut_size = 0;
                    adjustments.lut_intensity = 100.0;
                    adjustments.vignette_amount = 0.0;
                    adjustments.vignette_midpoint = 50.0;
                    adjustments.vignette_roundness = 0.0;
                    adjustments.vignette_feather = 50.0;
                    adjustments.grain_amount = 0.0;
                    adjustments.grain_size = 25.0;
                    adjustments.grain_roughness = 50.0;
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::CommitPreviewRender => {
                self.finish_pending_drag_undo();
                if !self.samples.is_empty() {
                    return self.request_preview_render(PreviewQuality::Full);
                }
            }
            Message::PreviewRendered { generation, result } => {
                if generation == self.preview_generation {
                    match result {
                        Ok(rendered) => {
                            self.rendered_preview = Some(rendered.handle.clone());
                            self.rendered_preview_size = Some((rendered.width, rendered.height));
                            if rendered.updates_thumbnail
                                && let Some(sample) = self.samples.get_mut(self.selected_index)
                            {
                                if let image::Handle::Rgba {
                                    width,
                                    height,
                                    pixels,
                                    ..
                                } = &rendered.handle
                                {
                                    if let Some(rgba) = ::image::RgbaImage::from_raw(
                                        *width,
                                        *height,
                                        pixels.to_vec(),
                                    ) {
                                        let image = DynamicImage::ImageRgba8(rgba);
                                        let thumbnail = resize_for_bound(&image, 320);
                                        sample.thumbnail = make_rounded_thumbnail_handle(
                                            &thumbnail,
                                            FILMSTRIP_IMAGE_RADIUS_PX,
                                        );
                                        sample.thumbnail_dimensions =
                                            (rendered.width, rendered.height);
                                    }
                                }
                            }
                            self.status_message = Some(if rendered.changed {
                                "Preview updated.".to_string()
                            } else {
                                "Preview render completed, but the pixels were unchanged."
                                    .to_string()
                            });
                        }
                        Err(error) => {
                            self.status_message = Some(error);
                        }
                    }
                    self.is_rendering_preview = false;
                    if let Some(quality) = self.pending_preview_quality.take() {
                        return self.start_preview_render(quality);
                    }
                }
            }
            Message::ThumbnailsRendered { results } => {
                for (index, result) in results {
                    if let Some(sample) = self.samples.get_mut(index) {
                        if let Ok(rendered) = result {
                            sample.thumbnail = rendered.handle;
                            sample.thumbnail_dimensions = (rendered.width, rendered.height);
                        }
                    }
                }
            }
        }

        Task::none()
    }

    fn is_animating_cards(&self) -> bool {
        (self.basic_card.progress - if self.basic_card.expanded { 1.0 } else { 0.0 }).abs() > 0.01
            || (self.curves_card.progress - if self.curves_card.expanded { 1.0 } else { 0.0 }).abs()
                > 0.01
            || (self.color_card.progress - if self.color_card.expanded { 1.0 } else { 0.0 }).abs()
                > 0.01
            || (self.details_card.progress - if self.details_card.expanded { 1.0 } else { 0.0 })
                .abs()
                > 0.01
            || (self.effects_card.progress - if self.effects_card.expanded { 1.0 } else { 0.0 })
                .abs()
                > 0.01
    }

    fn is_animating_toggles(&self) -> bool {
        (self.export_toggle_animations.enable_resize
            - bool_to_progress(self.export_settings.enable_resize))
        .abs()
            > 0.01
            || (self.export_toggle_animations.dont_enlarge
                - bool_to_progress(self.export_settings.dont_enlarge))
            .abs()
                > 0.01
            || (self.export_toggle_animations.keep_metadata
                - bool_to_progress(self.export_settings.keep_metadata))
            .abs()
                > 0.01
            || (self.export_toggle_animations.strip_gps
                - bool_to_progress(self.export_settings.strip_gps))
            .abs()
                > 0.01
            || (self.export_toggle_animations.export_masks
                - bool_to_progress(self.export_settings.export_masks))
            .abs()
                > 0.01
            || (self.export_toggle_animations.enable_watermark
                - bool_to_progress(self.export_settings.enable_watermark))
            .abs()
                > 0.01
    }

    fn update_selected_adjustments(
        &mut self,
        undo_behavior: UndoBehavior,
        update: impl Fn(&mut BasicAdjustments),
    ) {
        if matches!(undo_behavior, UndoBehavior::Immediate) {
            self.finish_pending_drag_undo();
        }

        let selected: Vec<usize> = if self.selected_indices.is_empty() {
            vec![self.selected_index]
        } else {
            self.selected_indices.iter().copied().collect()
        };

        let mut undo_changes = Vec::new();

        for index in selected {
            if let Some(sample) = self.samples.get_mut(index) {
                let previous = sample.adjustments.clone();
                update(&mut sample.adjustments);
                if sample.adjustments != previous {
                    undo_changes.push(UndoChange {
                        index,
                        adjustments: previous,
                    });
                }
                if let Err(error) = save_sample_adjustments(sample) {
                    self.status_message = Some(error);
                }
            }
        }

        if !undo_changes.is_empty() {
            match undo_behavior {
                UndoBehavior::Immediate => self.push_undo_entry(UndoEntry {
                    changes: undo_changes,
                }),
                UndoBehavior::Coalesced => {
                    if self.pending_drag_undo.is_none() {
                        self.pending_drag_undo = Some(UndoEntry {
                            changes: undo_changes,
                        });
                    }
                }
            }
        }

        if let Some(sample) = self.samples.get(self.selected_index) {
            self.basic_adjustments = sample.adjustments.clone();
            self.sync_crop_custom_inputs();
        }
    }

    fn sync_crop_custom_inputs(&mut self) {
        if let Some(ratio) = self.basic_adjustments.aspect_ratio {
            let height = 100.0;
            let width = ratio * height;
            self.crop_custom_width = trim_ratio_value(width);
            self.crop_custom_height = trim_ratio_value(height);
        } else {
            self.crop_custom_width.clear();
            self.crop_custom_height.clear();
        }
    }

    fn active_original_crop_ratio(&self) -> Option<f32> {
        let sample = self.samples.get(self.selected_index)?;
        effective_source_dimensions(
            sample.source_width,
            sample.source_height,
            self.basic_adjustments.orientation_steps,
        )
        .map(|(width, height)| width as f32 / height.max(1) as f32)
    }

    fn apply_crop_ratio_to_selected(&mut self, new_ratio: Option<f32>) {
        self.finish_pending_drag_undo();
        let selected = if self.selected_indices.is_empty() {
            vec![self.selected_index]
        } else {
            self.selected_indices.iter().copied().collect::<Vec<_>>()
        };

        let mut undo_changes = Vec::new();

        for index in selected {
            if let Some(sample) = self.samples.get_mut(index) {
                let previous = sample.adjustments.clone();
                sample.adjustments.aspect_ratio = new_ratio;
                let dims = effective_source_dimensions(
                    sample.source_width,
                    sample.source_height,
                    sample.adjustments.orientation_steps,
                );
                sample.adjustments.crop = match new_ratio {
                    Some(_) => crop_rect_for_ratio(dims, new_ratio),
                    None => full_crop_rect(dims),
                };
                if sample.adjustments != previous {
                    undo_changes.push(UndoChange {
                        index,
                        adjustments: previous,
                    });
                }
                if let Err(error) = save_sample_adjustments(sample) {
                    self.status_message = Some(error);
                }
            }
        }

        if !undo_changes.is_empty() {
            self.push_undo_entry(UndoEntry {
                changes: undo_changes,
            });
        }

        if let Some(sample) = self.samples.get(self.selected_index) {
            self.basic_adjustments = sample.adjustments.clone();
            self.sync_crop_custom_inputs();
        }
    }

    fn rotate_selected_images(&mut self, direction: i32) {
        self.finish_pending_drag_undo();
        let selected = if self.selected_indices.is_empty() {
            vec![self.selected_index]
        } else {
            self.selected_indices.iter().copied().collect::<Vec<_>>()
        };
        let mut undo_changes = Vec::new();

        for index in selected {
            if let Some(sample) = self.samples.get_mut(index) {
                let previous = sample.adjustments.clone();
                let increment = if direction >= 0 { 1 } else { 3 };
                sample.adjustments.orientation_steps =
                    ((sample.adjustments.orientation_steps as i32 + increment) % 4) as u8;
                sample.adjustments.rotation = 0.0;
                let dims = effective_source_dimensions(
                    sample.source_width,
                    sample.source_height,
                    sample.adjustments.orientation_steps,
                );
                sample.adjustments.crop =
                    crop_rect_for_ratio(dims, sample.adjustments.aspect_ratio);

                if sample.adjustments != previous {
                    undo_changes.push(UndoChange {
                        index,
                        adjustments: previous,
                    });
                }
                if let Err(error) = save_sample_adjustments(sample) {
                    self.status_message = Some(error);
                }
            }
        }

        if !undo_changes.is_empty() {
            self.push_undo_entry(UndoEntry {
                changes: undo_changes,
            });
        }

        if let Some(sample) = self.samples.get(self.selected_index) {
            self.basic_adjustments = sample.adjustments.clone();
            self.sync_crop_custom_inputs();
        }
    }

    fn crop_overlay_for_selected(&self) -> Option<CropOverlay> {
        if self.sidebar_page != SidebarPage::Crop {
            return None;
        }

        let sample = self.samples.get(self.selected_index)?;
        let image_size = effective_source_dimensions(
            sample.source_width,
            sample.source_height,
            self.basic_adjustments.orientation_steps,
        )?;
        let crop = self
            .basic_adjustments
            .crop
            .or_else(|| full_crop_rect(Some(image_size)))?;

        Some(CropOverlay {
            crop,
            image_size,
            locked_ratio: self.basic_adjustments.aspect_ratio,
            ruler_active: self.crop_ruler_active,
        })
    }

    fn push_undo_entry(&mut self, entry: UndoEntry) {
        self.undo_stack.push(entry);
        if self.undo_stack.len() > 200 {
            let overflow = self.undo_stack.len() - 200;
            self.undo_stack.drain(0..overflow);
        }
    }

    fn finish_pending_drag_undo(&mut self) {
        if let Some(entry) = self.pending_drag_undo.take() {
            self.push_undo_entry(entry);
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let content = match self.route {
            Route::Home => self.view_home(),
            Route::Editor => self.view_editor(),
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| {
                container::Style::default()
                    .background(Background::Color(Color::from_rgb8(0x10, 0x14, 0x1d)))
                    .color(Color::WHITE)
            })
            .into()
    }

    fn view_home(&self) -> Element<'_, Message> {
        let hero = column![
            text("RapidRAW Native POC").size(42),
            text("A first Iced prototype with folder import, an editor viewport, and a filmstrip.")
                .size(20)
                .color(Color::from_rgb8(0xd0, 0xd8, 0xe8)),
            Space::with_height(Length::Fixed(12.0)),
            row![
                button(text("Open Folder").size(18))
                    .padding([14, 24])
                    .on_press_maybe((!self.is_loading).then_some(Message::OpenFolder)),
                button(text("Open Current Project").size(18))
                    .padding([14, 24])
                    .on_press_maybe((!self.samples.is_empty()).then_some(Message::EnterEditor)),
            ]
            .spacing(12),
            text(
                self.status_message
                    .as_deref()
                    .unwrap_or("Pick a folder of RAW or standard image files, or explore the bundled sample project."),
            )
            .size(16)
            .color(Color::from_rgb8(0xa9, 0xb4, 0xc9)),
            if self.is_loading {
                text("Loading previews...").size(16).color(Color::from_rgb8(0xd5, 0xe6, 0xff))
            } else {
                text("").size(1)
            },
        ]
        .spacing(8);

        let preview_cards = self
            .samples
            .iter()
            .take(3)
            .fold(row![].spacing(20), |row, sample| {
                row.push(self.sample_card(sample, false))
            });

        let project_label = self
            .current_folder
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Bundled sample images from /public".to_string());

        let body = if self.samples.is_empty() {
            column![
                hero,
                Space::with_height(Length::Fixed(24.0)),
                container(
                    text("No images are loaded yet. Use Open Folder to import a directory of RAW or standard image files.")
                        .size(18)
                        .color(Color::from_rgb8(0xc3, 0xcc, 0xdd))
                )
                .padding(24)
                .style(panel_style),
            ]
        } else {
            column![
                hero,
                Space::with_height(Length::Fixed(24.0)),
                text("Current Project").size(24),
                text(project_label)
                    .size(16)
                    .color(Color::from_rgb8(0xa9, 0xb4, 0xc9)),
                Space::with_height(Length::Fixed(8.0)),
                preview_cards,
            ]
        };

        container(body.spacing(10).max_width(1200))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([48, 56])
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Top)
            .into()
    }

    fn view_editor(&self) -> Element<'_, Message> {
        let Some(selected) = self.samples.get(self.selected_index) else {
            return self.view_home();
        };
        let (image_width, image_height) = selected.full_preview_image.dimensions();
        let image_badge = container(
            row![
                text(&selected.name)
                    .size(16)
                    .color(Color::from_rgb8(0xe7, 0xec, 0xf6)),
                text(format!("{image_width}×{image_height}"))
                    .size(14)
                    .color(Color::from_rgb8(0x8d, 0x98, 0xae)),
            ]
            .align_y(iced::alignment::Vertical::Center)
            .spacing(10),
        )
        .padding([8, 12])
        .style(|_| container::Style {
            text_color: Some(Color::WHITE),
            background: Some(Background::Color(Color::from_rgb8(0x1b, 0x22, 0x2f))),
            border: Border::default().rounded(999.0),
            ..container::Style::default()
        });

        let top_bar = row![
            top_bar_icon_button(AppIcon::ArrowLeft, Some(Message::BackToHome), "Back"),
            top_bar_icon_button(
                AppIcon::FolderOpen,
                (!self.is_loading).then_some(Message::OpenFolder),
                "Open folder",
            ),
            Space::with_width(Length::Fixed(APP_SPACING)),
            row![
                image_badge,
                text(format!("{} selected", self.selected_indices.len().max(1)))
                    .size(13)
                    .color(Color::from_rgb8(0x8d, 0x98, 0xae)),
                if self.is_rendering_preview {
                    text("Rendering preview...")
                        .size(13)
                        .color(Color::from_rgb8(0xd5, 0xe6, 0xff))
                        .into()
                } else if let Some(status) = &self.status_message {
                    header_status(status)
                } else {
                    text(" ").size(13).color(Color::TRANSPARENT).into()
                },
            ]
            .align_y(iced::alignment::Vertical::Center)
            .spacing(APP_SPACING),
        ]
        .align_y(iced::alignment::Vertical::Center)
        .spacing(APP_SPACING);

        let preview_image: Element<'_, Message> = image(
            self.rendered_preview
                .clone()
                .unwrap_or_else(|| selected.preview.clone()),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .content_fit(iced::ContentFit::Contain)
        .into();

        let preview_content: Element<'_, Message> = if let Some(crop) = self.crop_overlay_for_selected() {
            stack![
                preview_image,
                canvas::Canvas::new(crop)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .into()
        } else {
            preview_image
        };

        let preview = container(preview_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(panel_style);

        let filmstrip_items = self
            .samples
            .iter()
            .enumerate()
            .fold(row![].spacing(16), |row, (index, sample)| {
                row.push(self.filmstrip_item(index, sample))
            });

        let filmstrip = container(
            container(
                scrollable(
                    container(filmstrip_items)
                        .padding([8, 6])
                        .width(Length::Shrink),
                )
                .width(Length::Fill)
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new()
                        .width(4)
                        .margin(1)
                        .scroller_width(4),
                ))
                .style(discrete_scrollbar_style),
            )
            .clip(true)
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fixed(128.0))
        .style(panel_style);

        let editor_body = row![
            preview,
            container(self.view_right_panel())
                .width(Length::Fixed(330.0))
                .height(Length::Fill)
        ]
        .spacing(APP_SPACING)
        .height(Length::Fill);

        let layout = column![top_bar, editor_body, filmstrip]
            .spacing(APP_SPACING)
            .height(Length::Fill);

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(APP_SPACING)
            .into()
    }

    fn sample_card<'a>(&'a self, sample: &'a SampleImage, selected: bool) -> Element<'a, Message> {
        let background = if selected {
            Color::from_rgb8(0x1d, 0x26, 0x34)
        } else {
            Color::from_rgb8(0x17, 0x1d, 0x28)
        };

        container(
            column![
                image(sample.thumbnail.clone())
                    .width(Length::Fixed(320.0))
                    .height(Length::Fixed(190.0))
                    .content_fit(iced::ContentFit::Cover),
                Space::with_height(Length::Fixed(12.0)),
                text(&sample.name).size(20),
                text(
                    sample
                        .path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                )
                .size(14)
                .color(Color::from_rgb8(0xa8, 0xb2, 0xc8)),
            ]
            .spacing(2),
        )
        .padding(16)
        .style(move |_| container::Style {
            text_color: Some(Color::WHITE),
            background: Some(Background::Color(background)),
            border: iced::Border::default().rounded(18.0),
            ..container::Style::default()
        })
        .into()
    }

    fn filmstrip_item<'a>(&'a self, index: usize, sample: &'a SampleImage) -> Element<'a, Message> {
        let is_active = index == self.selected_index;
        let is_selected = self.selected_indices.contains(&index);
        let background = if is_active || is_selected {
            Color::from_rgb8(0x1a, 0x20, 0x2b)
        } else {
            Color::from_rgb8(0x16, 0x1b, 0x25)
        };
        let (width, height) = sample.thumbnail_dimensions;
        let thumb_height = 92.0;
        let thumb_width = if height == 0 {
            thumb_height
        } else {
            ((width as f32 / height as f32) * thumb_height)
                .max(54.0)
                .min(220.0)
        };

        let image_frame = canvas::Canvas::new(ThumbnailCanvas {
            handle: sample.thumbnail.clone(),
            corner_radius: FILMSTRIP_IMAGE_RADIUS,
        })
        .width(Length::Fixed(thumb_width))
        .height(Length::Fixed(thumb_height));

        let card = container(image_frame)
            .padding(FILMSTRIP_CARD_PADDING)
            .clip(true)
            .style(move |_| container::Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(background)),
                border: Border {
                    color: if is_active || is_selected {
                        Color::WHITE
                    } else {
                        Color::TRANSPARENT
                    },
                    width: if is_active || is_selected { 1.5 } else { 0.0 },
                    radius: FILMSTRIP_CARD_RADIUS.into(),
                },
                ..container::Style::default()
            });

        let content: Element<'a, Message> = if sample.rating > 0 {
            let base: Element<'a, Message> = card.into();
            let badge: Element<'a, Message> = container(
                container(
                    row![
                        text(sample.rating.to_string()).size(11).color(Color::WHITE),
                        app_icon(AppIcon::Star, 11.0, Color::WHITE),
                    ]
                    .spacing(4)
                    .align_y(iced::alignment::Vertical::Center),
                )
                .padding([4, 8])
                .style(|_| container::Style {
                    text_color: Some(Color::WHITE),
                    background: Some(Background::Color(Color::from_rgba8(0x0d, 0x12, 0x1b, 0.58))),
                    border: Border::default().rounded(999.0),
                    ..container::Style::default()
                })
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::alignment::Vertical::Top),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([10, 10])
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Top)
            .into();

            stack![base, badge].into()
        } else {
            card.into()
        };

        let file_name = sample
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&sample.name);

        tooltip(
            mouse_area(content)
                .on_press(Message::SelectImage(index))
                .interaction(iced::mouse::Interaction::Pointer),
            container(
                text(file_name)
                    .size(12)
                    .color(Color::from_rgb8(0xe2, 0xe8, 0xf0)),
            )
            .padding([6, 10])
            .style(|_| container::Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(Color::from_rgb8(0x0f, 0x14, 0x1d))),
                border: Border::default().rounded(10.0),
                ..container::Style::default()
            }),
            tooltip::Position::Top,
        )
        .gap(8)
        .into()
    }

    fn view_right_panel(&self) -> Element<'_, Message> {
        let top_tabs = row![
            sidebar_tab_button(
                AppIcon::Crop,
                self.sidebar_page == SidebarPage::Crop,
                Message::SelectSidebarPage(SidebarPage::Crop),
                "Crop",
            ),
            sidebar_tab_button(
                AppIcon::SlidersHorizontal,
                self.sidebar_page == SidebarPage::Adjustments,
                Message::SelectSidebarPage(SidebarPage::Adjustments),
                "Adjustments",
            ),
            sidebar_tab_button(
                AppIcon::Share,
                self.sidebar_page == SidebarPage::Export,
                Message::SelectSidebarPage(SidebarPage::Export),
                "Export",
            ),
        ]
        .spacing(APP_SPACING);

        let divider = container(Space::with_height(Length::Fixed(3.0)))
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgb8(0x12, 0x17, 0x22))),
                ..container::Style::default()
            });

        let page_content: Element<'_, Message> = match self.sidebar_page {
            SidebarPage::Crop => self.view_crop_page(),
            SidebarPage::Adjustments => self.view_adjustments_page(),
            SidebarPage::Export => self.view_export_page(),
        };

        let scroll_content: Element<'_, Message> = scrollable(page_content)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new()
                    .width(4)
                    .margin(0)
                    .scroller_width(4),
            ))
            .style(discrete_scrollbar_style)
            .height(Length::Fill)
            .into();

        container(
            column![
                container(top_tabs).padding([14, 14]),
                divider,
                container(scroll_content)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .height(Length::Fill)
            .spacing(0),
        )
        .height(Length::Fill)
        .style(panel_style)
        .into()
    }

    fn view_crop_page(&self) -> Element<'_, Message> {
        let ratio = self.basic_adjustments.aspect_ratio;
        let active_original = self.active_original_crop_ratio();
        let is_original_active =
            matches!((ratio, active_original), (Some(current), Some(original)) if (current - original).abs() < CROP_RATIO_TOLERANCE);
        let invert_icon = if self.basic_adjustments.aspect_ratio.unwrap_or(1.0) >= 1.0 {
            AppIcon::RectangleVertical
        } else {
            AppIcon::RectangleHorizontal
        };

        let controls = column![
            card_section(
                "Crop & Transform",
                column![
                    row![
                        text("Aspect Ratio")
                            .size(14)
                            .color(Color::from_rgb8(0xe7, 0xec, 0xf6)),
                        Space::with_width(Length::Fill),
                        top_bar_icon_button(
                            invert_icon,
                            self.basic_adjustments
                                .aspect_ratio
                                .map(|_| Message::InvertCropAspectRatio),
                            "Invert aspect ratio",
                        ),
                        icon_button(
                            AppIcon::RotateCcw,
                            Message::ResetCropTransform,
                            "Reset crop & transform",
                        ),
                    ]
                    .align_y(iced::alignment::Vertical::Center),
                    row![
                        crop_choice_button(
                            "Free",
                            ratio.is_none(),
                            Message::CropPresetSelected(CropPresetKind::Free),
                        ),
                        crop_choice_button(
                            "Original",
                            is_original_active,
                            Message::CropPresetSelected(CropPresetKind::Original),
                        ),
                        crop_choice_button(
                            "1:1",
                            ratio_matches(ratio, 1.0),
                            Message::CropPresetSelected(CropPresetKind::Ratio(1.0)),
                        ),
                    ]
                    .spacing(8),
                    row![
                        crop_choice_button(
                            "5:4",
                            ratio_matches(ratio, 5.0 / 4.0),
                            Message::CropPresetSelected(CropPresetKind::Ratio(5.0 / 4.0)),
                        ),
                        crop_choice_button(
                            "4:3",
                            ratio_matches(ratio, 4.0 / 3.0),
                            Message::CropPresetSelected(CropPresetKind::Ratio(4.0 / 3.0)),
                        ),
                        crop_choice_button(
                            "3:2",
                            ratio_matches(ratio, 3.0 / 2.0),
                            Message::CropPresetSelected(CropPresetKind::Ratio(3.0 / 2.0)),
                        ),
                    ]
                    .spacing(8),
                    row![
                        crop_choice_button(
                            "16:9",
                            ratio_matches(ratio, 16.0 / 9.0),
                            Message::CropPresetSelected(CropPresetKind::Ratio(16.0 / 9.0)),
                        ),
                        crop_choice_button(
                            "21:9",
                            ratio_matches(ratio, 21.0 / 9.0),
                            Message::CropPresetSelected(CropPresetKind::Ratio(21.0 / 9.0)),
                        ),
                        crop_choice_button(
                            "Golden",
                            ratio_matches(ratio, BASE_CROP_RATIO),
                            Message::CropPresetSelected(CropPresetKind::Ratio(BASE_CROP_RATIO)),
                        ),
                    ]
                    .spacing(8),
                    row![
                        text_input("W", &self.crop_custom_width)
                            .on_input(Message::CropCustomWidthChanged)
                            .on_submit(Message::ApplyCustomCropRatio)
                            .padding([10, 12])
                            .size(14)
                            .width(Length::Fill),
                        text("×").size(14).color(Color::from_rgb8(0x8d, 0x98, 0xae)),
                        text_input("H", &self.crop_custom_height)
                            .on_input(Message::CropCustomHeightChanged)
                            .on_submit(Message::ApplyCustomCropRatio)
                            .padding([10, 12])
                            .size(14)
                            .width(Length::Fill),
                    ]
                    .spacing(8)
                    .align_y(iced::alignment::Vertical::Center),
                ]
                .spacing(12)
                .into(),
            ),
            card_section("Rotation", self.view_crop_rotation_card()),
            card_section(
                "Orientation",
                row![
                    orientation_action_button(
                        AppIcon::RotateCcw,
                        "Rotate L",
                        Message::RotateLeft,
                        false,
                    ),
                    orientation_action_button(
                        AppIcon::RotateCw,
                        "Rotate R",
                        Message::RotateRight,
                        false,
                    ),
                    orientation_action_button(
                        AppIcon::FlipHorizontal,
                        "Flip H",
                        Message::ToggleFlipHorizontal,
                        self.basic_adjustments.flip_horizontal,
                    ),
                    orientation_action_button(
                        AppIcon::FlipVertical,
                        "Flip V",
                        Message::ToggleFlipVertical,
                        self.basic_adjustments.flip_vertical,
                    ),
                ]
                .spacing(8)
                .into(),
            ),
        ]
        .spacing(16);

        container(controls)
            .padding([20, 14])
            .width(Length::Fill)
            .into()
    }

    fn view_crop_rotation_card(&self) -> Element<'_, Message> {
        let angle_text = format!("{:.1}°", self.basic_adjustments.rotation);

        column![
            row![
                text(angle_text)
                    .size(30)
                    .color(Color::from_rgb8(0xf5, 0xf8, 0xfe)),
                Space::with_width(Length::Fill),
                crop_rotation_icon_button(
                    AppIcon::Ruler,
                    self.crop_ruler_active,
                    Message::ToggleCropRuler,
                    "Straighten with ruler",
                ),
                crop_rotation_icon_button(
                    AppIcon::RotateCcw,
                    false,
                    Message::ResetCropRotation,
                    "Reset rotation",
                ),
            ]
            .spacing(8)
            .align_y(iced::alignment::Vertical::Center),
            slider(
                -45.0..=45.0,
                self.basic_adjustments.rotation,
                Message::CropRotationChanged,
            )
            .on_release(Message::CommitPreviewRender)
            .step(0.05)
            .width(Length::Fill),
        ]
        .spacing(18)
        .into()
    }

    fn view_adjustments_page(&self) -> Element<'_, Message> {
        let basic_body = column![
            row![
                Space::with_width(Length::Fill),
                icon_button(
                    AppIcon::RotateCcw,
                    Message::ResetBasicAdjustments,
                    "Reset basic adjustments"
                ),
            ]
            .align_y(iced::alignment::Vertical::Center),
            column![
                text("Tone Mapper")
                    .size(14)
                    .color(Color::from_rgb8(0xa8, 0xb2, 0xc8)),
                row![
                    tone_mapper_button(
                        "Basic",
                        ToneMapper::Basic,
                        self.basic_adjustments.tone_mapper
                    ),
                    tone_mapper_button("AgX", ToneMapper::AgX, self.basic_adjustments.tone_mapper),
                ]
                .spacing(8),
            ]
            .spacing(8),
            basic_slider(
                "Exposure",
                -5.0,
                5.0,
                self.basic_adjustments.exposure,
                Message::ExposureChanged
            ),
            basic_slider(
                "Brightness",
                -5.0,
                5.0,
                self.basic_adjustments.brightness,
                Message::BrightnessChanged
            ),
            basic_slider(
                "Contrast",
                -100.0,
                100.0,
                self.basic_adjustments.contrast,
                Message::ContrastChanged
            ),
            basic_slider(
                "Highlights",
                -100.0,
                100.0,
                self.basic_adjustments.highlights,
                Message::HighlightsChanged
            ),
            basic_slider(
                "Shadows",
                -100.0,
                100.0,
                self.basic_adjustments.shadows,
                Message::ShadowsChanged
            ),
            basic_slider(
                "Whites",
                -100.0,
                100.0,
                self.basic_adjustments.whites,
                Message::WhitesChanged
            ),
            basic_slider(
                "Blacks",
                -100.0,
                100.0,
                self.basic_adjustments.blacks,
                Message::BlacksChanged
            ),
        ]
        .spacing(14);

        let selected = self.samples.get(self.selected_index);
        let curves_body: Element<'_, Message> = if let Some(sample) = selected {
            let active_curve =
                curve_points(&self.basic_adjustments.curves, self.active_curve_channel);
            let histogram = histogram_bins(&sample.histogram, self.active_curve_channel);
            let curve_editor = canvas::Canvas::new(CurveEditor {
                channel: self.active_curve_channel,
                points: active_curve.to_vec(),
                histogram,
                color: curve_channel_color(self.active_curve_channel),
            })
            .width(Length::Fill)
            .height(Length::Fixed(240.0));

            column![
                row![
                    Space::with_width(Length::Fill),
                    icon_button(
                        AppIcon::RotateCcw,
                        Message::ResetCurveChannel(self.active_curve_channel),
                        "Reset curve channel"
                    ),
                ]
                .align_y(iced::alignment::Vertical::Center),
                row![
                    curve_channel_button("L", CurveChannel::Luma, self.active_curve_channel),
                    curve_channel_button("R", CurveChannel::Red, self.active_curve_channel),
                    curve_channel_button("G", CurveChannel::Green, self.active_curve_channel),
                    curve_channel_button("B", CurveChannel::Blue, self.active_curve_channel),
                ]
                .spacing(8),
                curve_editor,
            ]
            .spacing(12)
            .into()
        } else {
            text("Curves unavailable").into()
        };

        let current_hsl = *hsl_band(&self.basic_adjustments.hsl, self.active_hsl_band);
        let color_body = column![
            row![
                Space::with_width(Length::Fill),
                icon_button(
                    AppIcon::RotateCcw,
                    Message::ResetColorAdjustments,
                    "Reset color adjustments"
                ),
            ]
            .align_y(iced::alignment::Vertical::Center),
            card_section(
                "White Balance",
                column![
                    basic_slider(
                        "Temperature",
                        -100.0,
                        100.0,
                        self.basic_adjustments.temperature,
                        Message::TemperatureChanged,
                    ),
                    basic_slider(
                        "Tint",
                        -100.0,
                        100.0,
                        self.basic_adjustments.tint,
                        Message::TintChanged
                    ),
                ]
                .spacing(12)
                .into(),
            ),
            card_section(
                "Presence",
                column![
                    basic_slider(
                        "Vibrance",
                        -100.0,
                        100.0,
                        self.basic_adjustments.vibrance,
                        Message::VibranceChanged,
                    ),
                    basic_slider(
                        "Saturation",
                        -100.0,
                        100.0,
                        self.basic_adjustments.saturation,
                        Message::SaturationChanged,
                    ),
                ]
                .spacing(12)
                .into(),
            ),
            card_section(
                "Color Grading",
                column![
                    color_grading_wheel_panel(
                        "Midtones",
                        ColorGradingZone::Midtones,
                        self.basic_adjustments.color_grading.midtones,
                        150.0,
                    ),
                    row![
                        color_grading_wheel_panel(
                            "Shadows",
                            ColorGradingZone::Shadows,
                            self.basic_adjustments.color_grading.shadows,
                            116.0,
                        ),
                        color_grading_wheel_panel(
                            "Highlights",
                            ColorGradingZone::Highlights,
                            self.basic_adjustments.color_grading.highlights,
                            116.0,
                        ),
                    ]
                    .spacing(14),
                    basic_slider(
                        "Blending",
                        0.0,
                        100.0,
                        self.basic_adjustments.color_grading.blending,
                        Message::ColorGradingBlendingChanged,
                    ),
                    basic_slider(
                        "Balance",
                        -100.0,
                        100.0,
                        self.basic_adjustments.color_grading.balance,
                        Message::ColorGradingBalanceChanged,
                    ),
                ]
                .spacing(12)
                .into(),
            ),
            card_section(
                "Color Mixer",
                column![
                    row![
                        color_swatch_button(HslBand::Reds, self.active_hsl_band),
                        color_swatch_button(HslBand::Oranges, self.active_hsl_band),
                        color_swatch_button(HslBand::Yellows, self.active_hsl_band),
                        color_swatch_button(HslBand::Greens, self.active_hsl_band),
                        color_swatch_button(HslBand::Aquas, self.active_hsl_band),
                        color_swatch_button(HslBand::Blues, self.active_hsl_band),
                        color_swatch_button(HslBand::Purples, self.active_hsl_band),
                        color_swatch_button(HslBand::Magentas, self.active_hsl_band),
                    ]
                    .spacing(0)
                    .width(Length::Fill),
                    basic_slider(
                        "Hue",
                        -100.0,
                        100.0,
                        current_hsl.hue,
                        Message::HslHueChanged
                    ),
                    basic_slider(
                        "Saturation",
                        -100.0,
                        100.0,
                        current_hsl.saturation,
                        Message::HslSaturationChanged,
                    ),
                    basic_slider(
                        "Luminance",
                        -100.0,
                        100.0,
                        current_hsl.luminance,
                        Message::HslLuminanceChanged,
                    ),
                ]
                .spacing(12)
                .into(),
            ),
        ]
        .spacing(14);

        let details_body = column![
            row![
                Space::with_width(Length::Fill),
                icon_button(
                    AppIcon::RotateCcw,
                    Message::ResetDetailsAdjustments,
                    "Reset details adjustments"
                ),
            ]
            .align_y(iced::alignment::Vertical::Center),
            card_section(
                "Sharpening",
                column![basic_slider(
                    "Sharpness",
                    -100.0,
                    100.0,
                    self.basic_adjustments.sharpness,
                    Message::SharpnessChanged,
                )]
                .spacing(12)
                .into(),
            ),
            card_section(
                "Presence",
                column![
                    basic_slider(
                        "Clarity",
                        -100.0,
                        100.0,
                        self.basic_adjustments.clarity,
                        Message::ClarityChanged,
                    ),
                    basic_slider(
                        "Dehaze",
                        -100.0,
                        100.0,
                        self.basic_adjustments.dehaze,
                        Message::DehazeChanged,
                    ),
                    basic_slider(
                        "Structure",
                        -100.0,
                        100.0,
                        self.basic_adjustments.structure,
                        Message::StructureChanged,
                    ),
                    basic_slider(
                        "Centre",
                        -100.0,
                        100.0,
                        self.basic_adjustments.centre,
                        Message::CentreChanged,
                    ),
                ]
                .spacing(12)
                .into(),
            ),
            card_section(
                "Chromatic Aberration",
                column![
                    basic_slider(
                        "Red/Cyan",
                        -100.0,
                        100.0,
                        self.basic_adjustments.chromatic_aberration_red_cyan,
                        Message::ChromaticAberrationRedCyanChanged,
                    ),
                    basic_slider(
                        "Blue/Yellow",
                        -100.0,
                        100.0,
                        self.basic_adjustments.chromatic_aberration_blue_yellow,
                        Message::ChromaticAberrationBlueYellowChanged,
                    ),
                ]
                .spacing(12)
                .into(),
            ),
        ]
        .spacing(14);

        let effects_body = column![
            row![
                Space::with_width(Length::Fill),
                icon_button(
                    AppIcon::RotateCcw,
                    Message::ResetEffectsAdjustments,
                    "Reset effects adjustments"
                ),
            ]
            .align_y(iced::alignment::Vertical::Center),
            card_section(
                "Creative",
                column![
                    basic_slider(
                        "Glow",
                        0.0,
                        100.0,
                        self.basic_adjustments.glow_amount,
                        Message::GlowAmountChanged,
                    ),
                    basic_slider(
                        "Halation",
                        0.0,
                        100.0,
                        self.basic_adjustments.halation_amount,
                        Message::HalationAmountChanged,
                    ),
                    basic_slider(
                        "Light Flares",
                        0.0,
                        100.0,
                        self.basic_adjustments.flare_amount,
                        Message::FlareAmountChanged,
                    ),
                ]
                .spacing(12)
                .into(),
            ),
            card_section(
                "LUT",
                column![
                    row![
                        lut_picker_button(
                            self.basic_adjustments
                                .lut_name
                                .as_deref()
                                .unwrap_or("Select LUT"),
                            self.basic_adjustments.lut_name.is_some(),
                        ),
                        icon_button(
                            AppIcon::FolderOpen,
                            Message::SelectLutFolder,
                            "Choose LUT folder"
                        ),
                        if self.basic_adjustments.lut_name.is_some() {
                            icon_button(AppIcon::X, Message::ClearLut, "Clear LUT")
                        } else {
                            Space::with_width(Length::Shrink).into()
                        },
                    ]
                    .spacing(8)
                    .align_y(iced::alignment::Vertical::Center),
                    if let Some(lut_name) = &self.basic_adjustments.lut_name {
                        muted_line(format!(
                            "{} • {}^3",
                            lut_name, self.basic_adjustments.lut_size
                        ))
                    } else {
                        muted_line("Load a .cube, .3dl, or HALD image LUT.")
                    },
                    if self.basic_adjustments.lut_name.is_some() {
                        basic_slider(
                            "Intensity",
                            0.0,
                            100.0,
                            self.basic_adjustments.lut_intensity,
                            Message::LutIntensityChanged,
                        )
                    } else {
                        Element::from(Space::with_height(Length::Shrink))
                    },
                    if self.lut_browser.folder.is_some() {
                        lut_browser_list(
                            &self.lut_browser,
                            self.basic_adjustments.lut_path.as_deref(),
                        )
                    } else {
                        Element::from(Space::with_height(Length::Shrink))
                    },
                ]
                .spacing(10)
                .into(),
            ),
            card_section(
                "Vignette",
                column![
                    basic_slider(
                        "Amount",
                        -100.0,
                        100.0,
                        self.basic_adjustments.vignette_amount,
                        Message::VignetteAmountChanged,
                    ),
                    basic_slider(
                        "Midpoint",
                        0.0,
                        100.0,
                        self.basic_adjustments.vignette_midpoint,
                        Message::VignetteMidpointChanged,
                    ),
                    basic_slider(
                        "Roundness",
                        -100.0,
                        100.0,
                        self.basic_adjustments.vignette_roundness,
                        Message::VignetteRoundnessChanged,
                    ),
                    basic_slider(
                        "Feather",
                        0.0,
                        100.0,
                        self.basic_adjustments.vignette_feather,
                        Message::VignetteFeatherChanged,
                    ),
                ]
                .spacing(12)
                .into(),
            ),
            card_section(
                "Grain",
                column![
                    basic_slider(
                        "Amount",
                        0.0,
                        100.0,
                        self.basic_adjustments.grain_amount,
                        Message::GrainAmountChanged,
                    ),
                    basic_slider(
                        "Size",
                        0.0,
                        100.0,
                        self.basic_adjustments.grain_size,
                        Message::GrainSizeChanged,
                    ),
                    basic_slider(
                        "Roughness",
                        0.0,
                        100.0,
                        self.basic_adjustments.grain_roughness,
                        Message::GrainRoughnessChanged,
                    ),
                ]
                .spacing(12)
                .into(),
            ),
        ]
        .spacing(14);

        let controls = column![
            adjustment_card(
                "Basic",
                self.basic_card,
                Message::ToggleBasicCard,
                basic_body.into(),
                430.0
            ),
            adjustment_card(
                "Curves",
                self.curves_card,
                Message::ToggleCurvesCard,
                curves_body,
                320.0
            ),
            adjustment_card(
                "Color",
                self.color_card,
                Message::ToggleColorCard,
                color_body.into(),
                1180.0
            ),
            adjustment_card(
                "Details",
                self.details_card,
                Message::ToggleDetailsCard,
                details_body.into(),
                620.0
            ),
            adjustment_card(
                "Effects",
                self.effects_card,
                Message::ToggleEffectsCard,
                effects_body.into(),
                700.0
            ),
        ]
        .spacing(16);

        container(controls)
            .padding([20, 14])
            .width(Length::Fill)
            .into()
    }

    fn view_export_page(&self) -> Element<'_, Message> {
        let settings = &self.export_settings;
        let selection_count = self.selected_indices.len().max(1);
        let estimated_bytes = self
            .samples
            .get(self.selected_index)
            .map(|sample| estimate_export_bytes(sample, settings) * selection_count as u64)
            .unwrap_or(0);

        let controls = column![
            card_section(
                "File Settings",
                column![
                    export_option_row(
                        "Format",
                        row![
                            export_choice_button("JPEG", settings.file_format == ExportFileFormat::Jpeg, Message::ExportFormatChanged(ExportFileFormat::Jpeg)),
                            export_choice_button("PNG", settings.file_format == ExportFileFormat::Png, Message::ExportFormatChanged(ExportFileFormat::Png)),
                            export_choice_button("TIFF", settings.file_format == ExportFileFormat::Tiff, Message::ExportFormatChanged(ExportFileFormat::Tiff)),
                            export_choice_button("WebP", settings.file_format == ExportFileFormat::Webp, Message::ExportFormatChanged(ExportFileFormat::Webp)),
                        ]
                        .spacing(8)
                        .into()
                    ),
                    if settings.file_format == ExportFileFormat::Jpeg {
                        basic_slider(
                            "Quality",
                            1.0,
                            100.0,
                            settings.jpeg_quality,
                            Message::ExportJpegQualityChanged,
                        )
                    } else {
                        Element::from(Space::with_height(Length::Shrink))
                    },
                ]
                .spacing(12)
                .into(),
            ),
            card_section(
                "Image Sizing",
                column![
                    export_toggle_row(
                        "Resize",
                        settings.enable_resize,
                        self.export_toggle_animations.enable_resize,
                        Message::ExportResizeEnabledChanged(!settings.enable_resize),
                    ),
                    if settings.enable_resize {
                        column![
                            export_option_row(
                                "Mode",
                                row![
                                    export_choice_button("Long Edge", settings.resize_mode == ExportResizeMode::LongEdge, Message::ExportResizeModeChanged(ExportResizeMode::LongEdge)),
                                    export_choice_button("Short Edge", settings.resize_mode == ExportResizeMode::ShortEdge, Message::ExportResizeModeChanged(ExportResizeMode::ShortEdge)),
                                ]
                                .spacing(8)
                                .into()
                            ),
                            row![
                                export_choice_button("Width", settings.resize_mode == ExportResizeMode::Width, Message::ExportResizeModeChanged(ExportResizeMode::Width)),
                                export_choice_button("Height", settings.resize_mode == ExportResizeMode::Height, Message::ExportResizeModeChanged(ExportResizeMode::Height)),
                            ]
                            .spacing(8),
                            basic_slider(
                                "Target Size",
                                256.0,
                                6000.0,
                                settings.resize_value,
                                Message::ExportResizeValueChanged,
                            ),
                            export_toggle_row(
                                "Don't Enlarge",
                                settings.dont_enlarge,
                                self.export_toggle_animations.dont_enlarge,
                                Message::ExportDontEnlargeChanged(!settings.dont_enlarge),
                            ),
                        ]
                        .spacing(12)
                        .into()
                    } else {
                        Element::from(Space::with_height(Length::Shrink))
                    },
                ]
                .spacing(12)
                .into(),
            ),
            card_section(
                "Metadata",
                column![
                    export_toggle_row(
                        "Keep Metadata",
                        settings.keep_metadata,
                        self.export_toggle_animations.keep_metadata,
                        Message::ExportKeepMetadataChanged(!settings.keep_metadata),
                    ),
                    export_toggle_row(
                        "Strip GPS",
                        settings.strip_gps,
                        self.export_toggle_animations.strip_gps,
                        Message::ExportStripGpsChanged(!settings.strip_gps),
                    ),
                    export_toggle_row(
                        "Export Masks",
                        settings.export_masks,
                        self.export_toggle_animations.export_masks,
                        Message::ExportMasksChanged(!settings.export_masks),
                    ),
                ]
                .spacing(12)
                .into(),
            ),
            card_section(
                "Watermark",
                column![
                    export_toggle_row(
                        "Enable Watermark",
                        settings.enable_watermark,
                        self.export_toggle_animations.enable_watermark,
                        Message::ExportWatermarkEnabledChanged(!settings.enable_watermark),
                    ),
                    if settings.enable_watermark {
                        column![
                            export_option_row(
                                "Watermark Path",
                                text_input("Path to watermark", &settings.watermark_path)
                                    .on_input(Message::ExportWatermarkPathChanged)
                                    .padding([10, 12])
                                    .size(14)
                                    .into(),
                            ),
                            export_option_row(
                                "Anchor",
                                column![
                                    row![
                                        export_choice_button("TL", settings.watermark_anchor == WatermarkAnchor::TopLeft, Message::ExportWatermarkAnchorChanged(WatermarkAnchor::TopLeft)),
                                        export_choice_button("TC", settings.watermark_anchor == WatermarkAnchor::TopCenter, Message::ExportWatermarkAnchorChanged(WatermarkAnchor::TopCenter)),
                                        export_choice_button("TR", settings.watermark_anchor == WatermarkAnchor::TopRight, Message::ExportWatermarkAnchorChanged(WatermarkAnchor::TopRight)),
                                    ]
                                    .spacing(8),
                                    row![
                                        export_choice_button("CL", settings.watermark_anchor == WatermarkAnchor::CenterLeft, Message::ExportWatermarkAnchorChanged(WatermarkAnchor::CenterLeft)),
                                        export_choice_button("C", settings.watermark_anchor == WatermarkAnchor::Center, Message::ExportWatermarkAnchorChanged(WatermarkAnchor::Center)),
                                        export_choice_button("CR", settings.watermark_anchor == WatermarkAnchor::CenterRight, Message::ExportWatermarkAnchorChanged(WatermarkAnchor::CenterRight)),
                                    ]
                                    .spacing(8),
                                    row![
                                        export_choice_button("BL", settings.watermark_anchor == WatermarkAnchor::BottomLeft, Message::ExportWatermarkAnchorChanged(WatermarkAnchor::BottomLeft)),
                                        export_choice_button("BC", settings.watermark_anchor == WatermarkAnchor::BottomCenter, Message::ExportWatermarkAnchorChanged(WatermarkAnchor::BottomCenter)),
                                        export_choice_button("BR", settings.watermark_anchor == WatermarkAnchor::BottomRight, Message::ExportWatermarkAnchorChanged(WatermarkAnchor::BottomRight)),
                                    ]
                                    .spacing(8),
                                ]
                                .spacing(8)
                                .into(),
                            ),
                            basic_slider(
                                "Scale",
                                1.0,
                                100.0,
                                settings.watermark_scale,
                                Message::ExportWatermarkScaleChanged,
                            ),
                            basic_slider(
                                "Spacing",
                                0.0,
                                50.0,
                                settings.watermark_spacing,
                                Message::ExportWatermarkSpacingChanged,
                            ),
                            basic_slider(
                                "Opacity",
                                0.0,
                                100.0,
                                settings.watermark_opacity,
                                Message::ExportWatermarkOpacityChanged,
                            ),
                        ]
                        .spacing(12)
                        .into()
                    } else {
                        Element::from(Space::with_height(Length::Shrink))
                    },
                ]
                .spacing(12)
                .into(),
            ),
            card_section(
                "Export",
                column![
                    row![
                        column![
                            text("Estimated Size")
                                .size(14)
                                .color(Color::from_rgb8(0xa8, 0xb2, 0xc8)),
                            text(format_estimated_size(estimated_bytes))
                                .size(18)
                                .color(Color::from_rgb8(0xe7, 0xec, 0xf6)),
                        ]
                        .spacing(4),
                        Space::with_width(Length::Fill),
                        text(format!("{} image{}", selection_count, if selection_count == 1 { "" } else { "s" }))
                            .size(13)
                            .color(Color::from_rgb8(0x8d, 0x98, 0xae)),
                    ]
                    .align_y(iced::alignment::Vertical::Center),
                    {
                        let mut export_button = button(
                        row![
                            app_icon(AppIcon::Share, 16.0, Color::WHITE),
                            text(if self.is_exporting { "Exporting..." } else { "Export" })
                                .size(14)
                                .color(Color::WHITE),
                        ]
                        .spacing(8)
                        .align_y(iced::alignment::Vertical::Center),
                    )
                    .width(Length::Fill)
                    .padding([10, 12])
                    .style(|theme, status| {
                        let mut style = iced::widget::button::secondary(theme, status);
                        style.background = Some(Background::Color(Color::from_rgb8(0x24, 0x5d, 0x88)));
                        style.text_color = Color::WHITE;
                        style.border.radius = 12.0.into();
                        style
                    });
                        if !self.is_exporting {
                            export_button = export_button.on_press(Message::TriggerExport);
                        }
                        export_button
                    },
                ]
                .spacing(12)
                .into(),
            ),
        ]
        .spacing(16);

        container(controls)
            .padding([20, 14])
            .width(Length::Fill)
            .into()
    }

    fn request_preview_render(&mut self, quality: PreviewQuality) -> Task<Message> {
        let thumbnail_task = if matches!(quality, PreviewQuality::Full)
            && self.sidebar_page != SidebarPage::Crop
        {
            self.request_selected_thumbnail_render()
        } else {
            Task::none()
        };

        if self.is_rendering_preview {
            self.pending_preview_quality = Some(
                self.pending_preview_quality
                    .map_or(quality, |pending| pending.max(quality)),
            );
            return thumbnail_task;
        }

        Task::batch(vec![self.start_preview_render(quality), thumbnail_task])
    }

    fn start_preview_render(&mut self, quality: PreviewQuality) -> Task<Message> {
        let Some(sample) = self.samples.get(self.selected_index) else {
            return Task::none();
        };
        let Some(renderer) = self.renderer.clone() else {
            self.status_message = Some("GPU preview renderer is unavailable.".to_string());
            return Task::none();
        };

        self.preview_generation = self.preview_generation.wrapping_add(1);
        let generation = self.preview_generation;
        let base_image = match quality {
            PreviewQuality::Interactive => sample.interactive_preview_image.clone(),
            PreviewQuality::Full => sample.full_preview_image.clone(),
        };
        let mut adjustments = sample.adjustments.clone();
        if let Some(entry) = self
            .lut_browser
            .hovered_index
            .and_then(|index| self.lut_browser.entries.get(index))
        {
            adjustments.lut_path = Some(entry.path.to_string_lossy().to_string());
            adjustments.lut_name = Some(entry.name.clone());
            adjustments.lut_size = entry.size;
        }
        let is_raw = sample.is_raw;
        let source_dimensions = (sample.source_width, sample.source_height);
        let apply_crop = self.sidebar_page != SidebarPage::Crop;
        self.is_rendering_preview = true;
        self.status_message = None;

        Task::perform(
            async move {
                let base_rgba = base_image.to_rgba8().into_raw();
                let result = panic::catch_unwind(AssertUnwindSafe(|| {
                    let transformed = apply_adjustment_transformations(
                        base_image.as_ref(),
                        &adjustments,
                        source_dimensions,
                        apply_crop,
                    );
                    renderer
                        .render(&transformed, &adjustments, is_raw)
                        .map(|image| {
                            let rendered_rgba = image.to_rgba8().into_raw();
                            let changed = rendered_rgba != base_rgba;
                            RenderedPreview {
                                handle: image::Handle::from_rgba(
                                    image.width(),
                                    image.height(),
                                    rendered_rgba,
                                ),
                                width: image.width(),
                                height: image.height(),
                                changed,
                                updates_thumbnail: apply_crop,
                            }
                        })
                }))
                .map_err(|payload| {
                    if let Some(message) = payload.downcast_ref::<&str>() {
                        format!("Preview render panicked: {message}")
                    } else if let Some(message) = payload.downcast_ref::<String>() {
                        format!("Preview render panicked: {message}")
                    } else {
                        "Preview render panicked".to_string()
                    }
                })
                .and_then(|result| result);
                Message::PreviewRendered { generation, result }
            },
            |message| message,
        )
    }

    fn request_thumbnail_render(&self, indices: Vec<usize>) -> Task<Message> {
        let Some(renderer) = self.renderer.clone() else {
            return Task::none();
        };

        let jobs = indices
            .into_iter()
            .filter_map(|index| {
                self.samples.get(index).map(|sample| {
                    (
                        index,
                        sample.interactive_preview_image.clone(),
                        sample.adjustments.clone(),
                        sample.is_raw,
                        (sample.source_width, sample.source_height),
                    )
                })
            })
            .collect::<Vec<_>>();

        if jobs.is_empty() {
            return Task::none();
        }

        Task::perform(
            async move {
                let mut results = Vec::with_capacity(jobs.len());
                for (index, base_image, adjustments, is_raw, source_dimensions) in jobs {
                    let transformed = apply_adjustment_transformations(
                        base_image.as_ref(),
                        &adjustments,
                        source_dimensions,
                        true,
                    );
                    let rendered = renderer.render(&transformed, &adjustments, is_raw);
                    let result = rendered.map(|image| {
                        let (width, height) = image.dimensions();
                        let thumbnail = resize_for_bound(&image, 320);
                        RenderedThumbnail {
                            handle: make_rounded_thumbnail_handle(
                                &thumbnail,
                                FILMSTRIP_IMAGE_RADIUS_PX,
                            ),
                            width,
                            height,
                        }
                    });
                    results.push((index, result));
                }
                Message::ThumbnailsRendered { results }
            },
            |message| message,
        )
    }

    fn request_selected_thumbnail_render(&self) -> Task<Message> {
        let indices = if self.selected_indices.is_empty() {
            vec![self.selected_index]
        } else {
            self.selected_indices.iter().copied().collect()
        };
        self.request_thumbnail_render(indices)
    }
}

fn load_samples() -> Vec<SampleImage> {
    let public_dir = repo_root().join("public");
    let mut files = fs::read_dir(public_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| is_supported_image(path))
        .collect::<Vec<_>>();

    files.sort();

    files
        .into_iter()
        .filter_map(|path| build_sample_image(path).ok())
        .collect()
}

async fn load_folder_task(path: PathBuf) -> Result<LoadedFolder, String> {
    load_folder(path)
}

async fn load_lut_folder_task(path: PathBuf) -> Result<LutBrowserState, String> {
    load_lut_folder(path)
}

fn load_folder(path: PathBuf) -> Result<LoadedFolder, String> {
    let mut files = fs::read_dir(&path)
        .map_err(|error| format!("Failed to read folder {}: {}", path.display(), error))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|candidate| is_supported_image(candidate) || is_supported_raw(candidate))
        .collect::<Vec<_>>();

    files.sort();

    let mut samples = Vec::new();
    for file in files {
        samples.push(build_sample_image(file)?);
    }

    Ok(LoadedFolder { path, samples })
}

fn load_lut_folder(path: PathBuf) -> Result<LutBrowserState, String> {
    let mut files = fs::read_dir(&path)
        .map_err(|error| format!("Failed to read LUT folder {}: {}", path.display(), error))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|candidate| is_supported_lut(candidate))
        .collect::<Vec<_>>();

    files.sort();

    let mut entries = Vec::new();
    for file in files {
        match parse_lut_metadata(&file) {
            Ok(size) => {
                let name = file
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("Untitled LUT")
                    .replace(['-', '_'], " ");
                entries.push(LutListEntry {
                    name: title_case(&name),
                    path: file,
                    size,
                });
            }
            Err(_error) => {}
        }
    }

    Ok(LutBrowserState {
        folder: Some(path),
        entries,
        hovered_index: None,
        collapsed: false,
    })
}

fn build_sample_image(path: PathBuf) -> Result<SampleImage, String> {
    let is_raw = is_supported_raw(&path);
    let (
        source_width,
        source_height,
        interactive_preview_image,
        full_preview_image,
        preview,
        thumbnail,
    ) =
        load_preview_handles(&path)?;
    let histogram = build_histogram(full_preview_image.as_ref());
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Untitled")
        .replace(['-', '_'], " ");
    let adjustments = load_sample_adjustments(&path).unwrap_or_default();
    let rating = load_sample_rating(&path).unwrap_or(0);

    Ok(SampleImage {
        name: title_case(&name),
        path,
        source_width,
        source_height,
        thumbnail_dimensions: full_preview_image.dimensions(),
        interactive_preview_image,
        full_preview_image,
        preview,
        thumbnail,
        is_raw,
        rating,
        adjustments,
        histogram,
    })
}

fn load_preview_handles(
    path: &Path,
) -> Result<
    (
        u32,
        u32,
        Arc<DynamicImage>,
        Arc<DynamicImage>,
        image::Handle,
        image::Handle,
    ),
    String,
> {
    let image = if is_supported_raw(path) {
        decode_raw_preview(path)?
    } else {
        open_image(path).map_err(|error| error.to_string())?
    };
    let (source_width, source_height) = image.dimensions();

    let full_preview_image = resize_for_bound(&image, 1800);
    let interactive_preview_image = resize_for_bound(&full_preview_image, 1100);
    let preview = make_rgba_handle(&full_preview_image);
    let thumbnail = make_rounded_thumbnail_handle(
        &resize_for_bound(&full_preview_image, 320),
        FILMSTRIP_IMAGE_RADIUS_PX,
    );

    Ok((
        source_width,
        source_height,
        Arc::new(interactive_preview_image),
        Arc::new(full_preview_image),
        preview,
        thumbnail,
    ))
}

fn make_rgba_handle(image: &DynamicImage) -> image::Handle {
    let rgba = image.to_rgba8();
    image::Handle::from_rgba(rgba.width(), rgba.height(), rgba.into_raw())
}

fn make_rounded_thumbnail_handle(image: &DynamicImage, radius: u32) -> image::Handle {
    let mut rgba: RgbaImage = image.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let radius = radius.min(width / 2).min(height / 2) as f32;

    if radius > 0.0 {
        for y in 0..height {
            for x in 0..width {
                let cx = if x < radius as u32 {
                    radius - 0.5
                } else if x >= width.saturating_sub(radius as u32) {
                    width as f32 - radius - 0.5
                } else {
                    continue;
                };

                let cy = if y < radius as u32 {
                    radius - 0.5
                } else if y >= height.saturating_sub(radius as u32) {
                    height as f32 - radius - 0.5
                } else {
                    continue;
                };

                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                if dx * dx + dy * dy > radius * radius {
                    rgba.get_pixel_mut(x, y).0[3] = 0;
                }
            }
        }
    }

    image::Handle::from_rgba(rgba.width(), rgba.height(), rgba.into_raw())
}

fn resize_for_bound(image: &DynamicImage, bound: u32) -> DynamicImage {
    let (width, height) = image.dimensions();
    if width <= bound && height <= bound {
        return image.clone();
    }

    image.resize(bound, bound, FilterType::Lanczos3)
}

fn decode_raw_preview(path: &Path) -> Result<DynamicImage, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let source = RawSource::new_from_slice(&bytes);
    let decoder = rawler::get_decoder(&source).map_err(|error| error.to_string())?;
    let raw_image = decoder
        .raw_image(&source, &RawDecodeParams::default(), false)
        .map_err(|error| error.to_string())?;
    let metadata = decoder
        .raw_metadata(&source, &RawDecodeParams::default())
        .map_err(|error| error.to_string())?;

    let orientation = metadata
        .exif
        .orientation
        .map(Orientation::from_u16)
        .unwrap_or(Orientation::Normal);

    let mut developer = RawDevelop::default();
    developer.demosaic_algorithm = DemosaicAlgorithm::Speed;
    developer.steps.retain(|&step| step != ProcessingStep::SRgb);

    let developed = developer
        .develop_intermediate(&raw_image)
        .map_err(|error| error.to_string())?;

    let dynamic = intermediate_to_dynamic_image(developed)?;

    Ok(apply_orientation(dynamic, orientation))
}

fn apply_orientation(image: DynamicImage, orientation: Orientation) -> DynamicImage {
    match orientation {
        Orientation::Normal => image,
        Orientation::HorizontalFlip => image.fliph(),
        Orientation::Rotate180 => image.rotate180(),
        Orientation::VerticalFlip => image.flipv(),
        Orientation::Transpose => image.rotate90().fliph(),
        Orientation::Rotate90 => image.rotate90(),
        Orientation::Transverse => image.rotate270().fliph(),
        Orientation::Rotate270 => image.rotate270(),
        _ => image,
    }
}

fn intermediate_to_dynamic_image(intermediate: Intermediate) -> Result<DynamicImage, String> {
    let image = intermediate
        .to_dynamic_image()
        .ok_or_else(|| "Failed to convert RAW intermediate to image".to_string())?;

    Ok(match image {
        DynamicImage::ImageRgb16(rgb) => DynamicImage::ImageRgb16(rgb),
        DynamicImage::ImageRgba16(rgba) => DynamicImage::ImageRgba16(rgba),
        DynamicImage::ImageLuma16(luma) => {
            let rgb = RgbImage::from_fn(luma.width(), luma.height(), |x, y| {
                let value = (luma.get_pixel(x, y)[0] >> 8) as u8;
                ::image::Rgb([value, value, value])
            });
            DynamicImage::ImageRgb8(rgb)
        }
        other => other,
    })
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .to_path_buf()
}

fn is_supported_image(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
    )
}

fn is_supported_raw(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()),
        Some(ext)
            if matches!(
                ext.as_str(),
                "arw"
                    | "cr2"
                    | "cr3"
                    | "dng"
                    | "erf"
                    | "iiq"
                    | "kdc"
                    | "mef"
                    | "mos"
                    | "mrw"
                    | "nef"
                    | "nrw"
                    | "orf"
                    | "pef"
                    | "raf"
                    | "raw"
                    | "rw2"
                    | "srw"
                    | "x3f"
            )
    )
}

fn is_supported_lut(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if matches!(ext.as_str(), "cube" | "3dl" | "png" | "jpg" | "jpeg" | "tiff" | "tif")
    )
}

fn title_case(input: &str) -> String {
    input
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut out = String::new();
                    out.extend(first.to_uppercase());
                    out.push_str(chars.as_str());
                    out
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn matches_undo_key(key: &keyboard::Key) -> bool {
    match key.as_ref() {
        keyboard::Key::Character(character) => character.eq_ignore_ascii_case("z"),
        _ => false,
    }
}

fn matches_select_all_key(key: &keyboard::Key) -> bool {
    match key.as_ref() {
        keyboard::Key::Character(character) => character.eq_ignore_ascii_case("a"),
        _ => false,
    }
}

fn arrow_navigation_message(key: &keyboard::Key) -> Option<Message> {
    match key.as_ref() {
        keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
            Some(Message::NavigateSelection(-1))
        }
        keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
            Some(Message::NavigateSelection(1))
        }
        _ => None,
    }
}

fn rating_from_key(key: &keyboard::Key) -> Option<u8> {
    match key.as_ref() {
        keyboard::Key::Character(character) if character.len() == 1 => {
            match character.chars().next()? {
                '0' => Some(0),
                '1' => Some(1),
                '2' => Some(2),
                '3' => Some(3),
                '4' => Some(4),
                '5' => Some(5),
                _ => None,
            }
        }
        _ => None,
    }
}

fn sidecar_path_for_image(path: &Path) -> PathBuf {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{file_name}.rrdata"))
}

fn load_sample_adjustments(path: &Path) -> Result<BasicAdjustments, String> {
    let sidecar_path = sidecar_path_for_image(path);
    if !sidecar_path.exists() {
        return Ok(BasicAdjustments::default());
    }

    let content = fs::read_to_string(&sidecar_path)
        .map_err(|error| format!("Failed to read {}: {}", sidecar_path.display(), error))?;
    let metadata: ImageMetadata = serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse {}: {}", sidecar_path.display(), error))?;

    Ok(adjustments_from_value(&metadata.adjustments))
}

fn load_sample_rating(path: &Path) -> Result<u8, String> {
    let sidecar_path = sidecar_path_for_image(path);
    if !sidecar_path.exists() {
        return Ok(0);
    }

    let content = fs::read_to_string(&sidecar_path)
        .map_err(|error| format!("Failed to read {}: {}", sidecar_path.display(), error))?;
    let metadata: ImageMetadata = serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse {}: {}", sidecar_path.display(), error))?;

    Ok(metadata.rating.min(5))
}

fn save_sample_adjustments(sample: &SampleImage) -> Result<(), String> {
    let sidecar_path = sidecar_path_for_image(&sample.path);
    let mut metadata = if sidecar_path.exists() {
        fs::read_to_string(&sidecar_path)
            .ok()
            .and_then(|content| serde_json::from_str::<ImageMetadata>(&content).ok())
            .unwrap_or_default()
    } else {
        ImageMetadata::default()
    };

    metadata.rating = sample.rating.min(5);
    merge_basic_adjustments(&mut metadata.adjustments, &sample.adjustments);

    let json_string = serde_json::to_string_pretty(&metadata)
        .map_err(|error| format!("Failed to serialize {}: {}", sidecar_path.display(), error))?;
    fs::write(&sidecar_path, json_string)
        .map_err(|error| format!("Failed to write {}: {}", sidecar_path.display(), error))
}

fn adjustments_from_value(value: &Value) -> BasicAdjustments {
    if !value.is_object() {
        return BasicAdjustments::default();
    }

    let tone_mapper = match value
        .get("toneMapper")
        .and_then(Value::as_str)
        .unwrap_or("agx")
        .to_ascii_lowercase()
        .as_str()
    {
        "basic" => ToneMapper::Basic,
        _ => ToneMapper::AgX,
    };

    BasicAdjustments {
        aspect_ratio: value
            .get("aspectRatio")
            .and_then(Value::as_f64)
            .map(|value| value as f32),
        crop: crop_rect_from_value(value.get("crop").unwrap_or(&Value::Null)),
        rotation: value.get("rotation").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        flip_horizontal: value
            .get("flipHorizontal")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        flip_vertical: value
            .get("flipVertical")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        orientation_steps: value
            .get("orientationSteps")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8,
        exposure: value.get("exposure").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        brightness: value
            .get("brightness")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        contrast: value.get("contrast").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        highlights: value
            .get("highlights")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        shadows: value.get("shadows").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        whites: value.get("whites").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        blacks: value.get("blacks").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        saturation: value
            .get("saturation")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        temperature: value
            .get("temperature")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        tint: value.get("tint").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        vibrance: value.get("vibrance").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        sharpness: value
            .get("sharpness")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        luma_noise_reduction: value
            .get("lumaNoiseReduction")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        color_noise_reduction: value
            .get("colorNoiseReduction")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        clarity: value.get("clarity").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        dehaze: value.get("dehaze").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        structure: value
            .get("structure")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        centre: value.get("centré").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        chromatic_aberration_red_cyan: value
            .get("chromaticAberrationRedCyan")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        chromatic_aberration_blue_yellow: value
            .get("chromaticAberrationBlueYellow")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        vignette_amount: value
            .get("vignetteAmount")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        vignette_midpoint: value
            .get("vignetteMidpoint")
            .and_then(Value::as_f64)
            .unwrap_or(50.0) as f32,
        vignette_roundness: value
            .get("vignetteRoundness")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        vignette_feather: value
            .get("vignetteFeather")
            .and_then(Value::as_f64)
            .unwrap_or(50.0) as f32,
        grain_amount: value
            .get("grainAmount")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        grain_size: value
            .get("grainSize")
            .and_then(Value::as_f64)
            .unwrap_or(25.0) as f32,
        grain_roughness: value
            .get("grainRoughness")
            .and_then(Value::as_f64)
            .unwrap_or(50.0) as f32,
        glow_amount: value
            .get("glowAmount")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        halation_amount: value
            .get("halationAmount")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        flare_amount: value
            .get("flareAmount")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        lut_path: value
            .get("lutPath")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        lut_name: value
            .get("lutName")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        lut_size: value.get("lutSize").and_then(Value::as_u64).unwrap_or(0) as u32,
        lut_intensity: value
            .get("lutIntensity")
            .and_then(Value::as_f64)
            .unwrap_or(100.0) as f32,
        tone_mapper,
        hsl: hsl_from_value(value.get("hsl").unwrap_or(&Value::Null)),
        color_grading: color_grading_from_value(value.get("colorGrading").unwrap_or(&Value::Null)),
        color_calibration: color_calibration_from_value(
            value.get("colorCalibration").unwrap_or(&Value::Null),
        ),
        curves: curves_from_value(value.get("curves").unwrap_or(&Value::Null)),
    }
}

fn adjustments_to_value(adjustments: &BasicAdjustments) -> Value {
    json!({
        "aspectRatio": adjustments.aspect_ratio,
        "crop": crop_rect_to_value(adjustments.crop),
        "rotation": adjustments.rotation,
        "flipHorizontal": adjustments.flip_horizontal,
        "flipVertical": adjustments.flip_vertical,
        "orientationSteps": adjustments.orientation_steps,
        "exposure": adjustments.exposure,
        "brightness": adjustments.brightness,
        "contrast": adjustments.contrast,
        "highlights": adjustments.highlights,
        "shadows": adjustments.shadows,
        "whites": adjustments.whites,
        "blacks": adjustments.blacks,
        "saturation": adjustments.saturation,
        "temperature": adjustments.temperature,
        "tint": adjustments.tint,
        "vibrance": adjustments.vibrance,
        "sharpness": adjustments.sharpness,
        "lumaNoiseReduction": adjustments.luma_noise_reduction,
        "colorNoiseReduction": adjustments.color_noise_reduction,
        "clarity": adjustments.clarity,
        "dehaze": adjustments.dehaze,
        "structure": adjustments.structure,
        "centré": adjustments.centre,
        "chromaticAberrationRedCyan": adjustments.chromatic_aberration_red_cyan,
        "chromaticAberrationBlueYellow": adjustments.chromatic_aberration_blue_yellow,
        "vignetteAmount": adjustments.vignette_amount,
        "vignetteMidpoint": adjustments.vignette_midpoint,
        "vignetteRoundness": adjustments.vignette_roundness,
        "vignetteFeather": adjustments.vignette_feather,
        "grainAmount": adjustments.grain_amount,
        "grainSize": adjustments.grain_size,
        "grainRoughness": adjustments.grain_roughness,
        "glowAmount": adjustments.glow_amount,
        "halationAmount": adjustments.halation_amount,
        "flareAmount": adjustments.flare_amount,
        "lutPath": adjustments.lut_path.clone(),
        "lutName": adjustments.lut_name.clone(),
        "lutSize": adjustments.lut_size,
        "lutIntensity": adjustments.lut_intensity,
        "toneMapper": match adjustments.tone_mapper {
            ToneMapper::Basic => "basic",
            ToneMapper::AgX => "agx",
        },
    })
}

fn merge_basic_adjustments(target: &mut Value, adjustments: &BasicAdjustments) {
    if !target.is_object() {
        *target = json!({});
    }

    if let Some(object) = target.as_object_mut() {
        let basic = adjustments_to_value(adjustments);
        if let Some(basic_object) = basic.as_object() {
            for (key, value) in basic_object {
                object.insert(key.clone(), value.clone());
            }
        }
        object.insert("hsl".to_string(), hsl_to_value(&adjustments.hsl));
        object.insert(
            "colorGrading".to_string(),
            color_grading_to_value(&adjustments.color_grading),
        );
        object.insert(
            "colorCalibration".to_string(),
            color_calibration_to_value(&adjustments.color_calibration),
        );
        object.insert("curves".to_string(), curves_to_value(&adjustments.curves));
    }
}

fn hsl_from_value(value: &Value) -> HslSettings {
    HslSettings {
        reds: hue_sat_lum_from_value(value.get("reds").unwrap_or(&Value::Null)),
        oranges: hue_sat_lum_from_value(value.get("oranges").unwrap_or(&Value::Null)),
        yellows: hue_sat_lum_from_value(value.get("yellows").unwrap_or(&Value::Null)),
        greens: hue_sat_lum_from_value(value.get("greens").unwrap_or(&Value::Null)),
        aquas: hue_sat_lum_from_value(value.get("aquas").unwrap_or(&Value::Null)),
        blues: hue_sat_lum_from_value(value.get("blues").unwrap_or(&Value::Null)),
        purples: hue_sat_lum_from_value(value.get("purples").unwrap_or(&Value::Null)),
        magentas: hue_sat_lum_from_value(value.get("magentas").unwrap_or(&Value::Null)),
    }
}

fn color_grading_from_value(value: &Value) -> ColorGradingSettingsUi {
    ColorGradingSettingsUi {
        shadows: hue_sat_lum_from_value(value.get("shadows").unwrap_or(&Value::Null)),
        midtones: hue_sat_lum_from_value(value.get("midtones").unwrap_or(&Value::Null)),
        highlights: hue_sat_lum_from_value(value.get("highlights").unwrap_or(&Value::Null)),
        blending: value
            .get("blending")
            .and_then(Value::as_f64)
            .unwrap_or(50.0) as f32,
        balance: value.get("balance").and_then(Value::as_f64).unwrap_or(0.0) as f32,
    }
}

fn color_calibration_from_value(value: &Value) -> ColorCalibrationSettingsUi {
    ColorCalibrationSettingsUi {
        shadows_tint: value
            .get("shadowsTint")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        red_hue: value.get("redHue").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        red_saturation: value
            .get("redSaturation")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        green_hue: value.get("greenHue").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        green_saturation: value
            .get("greenSaturation")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        blue_hue: value.get("blueHue").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        blue_saturation: value
            .get("blueSaturation")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
    }
}

fn hue_sat_lum_from_value(value: &Value) -> HueSatLum {
    HueSatLum {
        hue: value.get("hue").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        saturation: value
            .get("saturation")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        luminance: value
            .get("luminance")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
    }
}

fn crop_rect_from_value(value: &Value) -> Option<CropRect> {
    Some(CropRect {
        x: value.get("x")?.as_f64()? as f32,
        y: value.get("y")?.as_f64()? as f32,
        width: value.get("width")?.as_f64()? as f32,
        height: value.get("height")?.as_f64()? as f32,
    })
}

fn crop_rect_to_value(value: Option<CropRect>) -> Value {
    match value {
        Some(crop) => json!({
            "x": crop.x,
            "y": crop.y,
            "width": crop.width,
            "height": crop.height,
        }),
        None => Value::Null,
    }
}

fn hsl_to_value(value: &HslSettings) -> Value {
    json!({
        "reds": hue_sat_lum_to_value(value.reds),
        "oranges": hue_sat_lum_to_value(value.oranges),
        "yellows": hue_sat_lum_to_value(value.yellows),
        "greens": hue_sat_lum_to_value(value.greens),
        "aquas": hue_sat_lum_to_value(value.aquas),
        "blues": hue_sat_lum_to_value(value.blues),
        "purples": hue_sat_lum_to_value(value.purples),
        "magentas": hue_sat_lum_to_value(value.magentas),
    })
}

fn color_grading_to_value(value: &ColorGradingSettingsUi) -> Value {
    json!({
        "shadows": hue_sat_lum_to_value(value.shadows),
        "midtones": hue_sat_lum_to_value(value.midtones),
        "highlights": hue_sat_lum_to_value(value.highlights),
        "blending": value.blending,
        "balance": value.balance,
    })
}

fn color_calibration_to_value(value: &ColorCalibrationSettingsUi) -> Value {
    json!({
        "shadowsTint": value.shadows_tint,
        "redHue": value.red_hue,
        "redSaturation": value.red_saturation,
        "greenHue": value.green_hue,
        "greenSaturation": value.green_saturation,
        "blueHue": value.blue_hue,
        "blueSaturation": value.blue_saturation,
    })
}

fn hue_sat_lum_to_value(value: HueSatLum) -> Value {
    json!({
        "hue": value.hue,
        "saturation": value.saturation,
        "luminance": value.luminance,
    })
}

fn curves_from_value(value: &Value) -> CurvesSettings {
    CurvesSettings {
        luma: parse_curve_channel(value.get("luma")),
        red: parse_curve_channel(value.get("red")),
        green: parse_curve_channel(value.get("green")),
        blue: parse_curve_channel(value.get("blue")),
    }
}

fn parse_curve_channel(value: Option<&Value>) -> Vec<CurvePoint> {
    let Some(points) = value.and_then(Value::as_array) else {
        return default_curve_points();
    };

    let parsed = points
        .iter()
        .filter_map(|point| {
            Some(CurvePoint {
                x: point.get("x")?.as_f64()? as f32,
                y: point.get("y")?.as_f64()? as f32,
            })
        })
        .collect::<Vec<_>>();

    sanitize_curve_points(parsed)
}

fn curves_to_value(curves: &CurvesSettings) -> Value {
    json!({
        "luma": curve_points_to_json(&curves.luma),
        "red": curve_points_to_json(&curves.red),
        "green": curve_points_to_json(&curves.green),
        "blue": curve_points_to_json(&curves.blue),
    })
}

fn curve_points_to_json(points: &[CurvePoint]) -> Vec<Value> {
    points
        .iter()
        .map(|point| json!({ "x": point.x, "y": point.y }))
        .collect()
}

fn curve_points(curves: &CurvesSettings, channel: CurveChannel) -> &[CurvePoint] {
    match channel {
        CurveChannel::Luma => &curves.luma,
        CurveChannel::Red => &curves.red,
        CurveChannel::Green => &curves.green,
        CurveChannel::Blue => &curves.blue,
    }
}

fn curve_points_mut(curves: &mut CurvesSettings, channel: CurveChannel) -> &mut Vec<CurvePoint> {
    match channel {
        CurveChannel::Luma => &mut curves.luma,
        CurveChannel::Red => &mut curves.red,
        CurveChannel::Green => &mut curves.green,
        CurveChannel::Blue => &mut curves.blue,
    }
}

fn sanitize_curve_points(mut points: Vec<CurvePoint>) -> Vec<CurvePoint> {
    if points.len() < 2 {
        return default_curve_points();
    }

    for point in &mut points {
        point.x = point.x.clamp(0.0, 255.0);
        point.y = point.y.clamp(0.0, 255.0);
    }

    points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    points.dedup_by(|a, b| (a.x - b.x).abs() < 0.01);

    if let Some(first) = points.first_mut() {
        first.x = 0.0;
    }
    if let Some(last) = points.last_mut() {
        last.x = 255.0;
    }

    if points.len() < 2 {
        default_curve_points()
    } else if points.len() > 16 {
        let mut reduced = Vec::with_capacity(16);
        for (index, point) in points.into_iter().enumerate() {
            if index == 0 || index == 15 || index % ((index.max(1) + 15) / 14) == 0 {
                reduced.push(point);
                if reduced.len() == 16 {
                    break;
                }
            }
        }
        sanitize_curve_points(reduced)
    } else {
        points
    }
}

fn build_histogram(image: &DynamicImage) -> HistogramData {
    let mut histogram = HistogramData {
        luma: [0; 256],
        red: [0; 256],
        green: [0; 256],
        blue: [0; 256],
    };

    for pixel in image.to_rgb8().pixels() {
        let r = pixel[0] as usize;
        let g = pixel[1] as usize;
        let b = pixel[2] as usize;
        let luma = (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32)
            .round()
            .clamp(0.0, 255.0) as usize;
        histogram.red[r] += 1;
        histogram.green[g] += 1;
        histogram.blue[b] += 1;
        histogram.luma[luma] += 1;
    }

    histogram
}

fn histogram_bins(histogram: &HistogramData, channel: CurveChannel) -> [u32; 256] {
    match channel {
        CurveChannel::Luma => histogram.luma,
        CurveChannel::Red => histogram.red,
        CurveChannel::Green => histogram.green,
        CurveChannel::Blue => histogram.blue,
    }
}

fn curve_channel_color(channel: CurveChannel) -> Color {
    match channel {
        CurveChannel::Luma => Color::from_rgb8(0xd8, 0xe1, 0xf0),
        CurveChannel::Red => Color::from_rgb8(0xff, 0x6b, 0x6b),
        CurveChannel::Green => Color::from_rgb8(0x6b, 0xcb, 0x77),
        CurveChannel::Blue => Color::from_rgb8(0x4d, 0x96, 0xff),
    }
}

#[derive(Debug, Clone, Copy)]
enum HslField {
    Hue,
    Saturation,
    Luminance,
}

fn hsl_band(settings: &HslSettings, band: HslBand) -> &HueSatLum {
    match band {
        HslBand::Reds => &settings.reds,
        HslBand::Oranges => &settings.oranges,
        HslBand::Yellows => &settings.yellows,
        HslBand::Greens => &settings.greens,
        HslBand::Aquas => &settings.aquas,
        HslBand::Blues => &settings.blues,
        HslBand::Purples => &settings.purples,
        HslBand::Magentas => &settings.magentas,
    }
}

fn hsl_band_mut(settings: &mut HslSettings, band: HslBand) -> &mut HueSatLum {
    match band {
        HslBand::Reds => &mut settings.reds,
        HslBand::Oranges => &mut settings.oranges,
        HslBand::Yellows => &mut settings.yellows,
        HslBand::Greens => &mut settings.greens,
        HslBand::Aquas => &mut settings.aquas,
        HslBand::Blues => &mut settings.blues,
        HslBand::Purples => &mut settings.purples,
        HslBand::Magentas => &mut settings.magentas,
    }
}

fn set_hsl_value(value: &mut HueSatLum, field: HslField, amount: f32) {
    match field {
        HslField::Hue => value.hue = amount,
        HslField::Saturation => value.saturation = amount,
        HslField::Luminance => value.luminance = amount,
    }
}

fn estimate_export_bytes(sample: &SampleImage, settings: &ExportSettingsUi) -> u64 {
    let (mut width, mut height) = sample.full_preview_image.dimensions();

    if settings.enable_resize {
        let max_dimension = settings.resize_value.max(1.0);
        let (long_edge, short_edge) = if width >= height {
            (width as f32, height as f32)
        } else {
            (height as f32, width as f32)
        };

        let resize_ratio = match settings.resize_mode {
            ExportResizeMode::LongEdge => (max_dimension / long_edge).min(1.0),
            ExportResizeMode::ShortEdge => (max_dimension / short_edge).min(1.0),
            ExportResizeMode::Width => (max_dimension / width as f32).min(1.0),
            ExportResizeMode::Height => (max_dimension / height as f32).min(1.0),
        };

        let ratio = if settings.dont_enlarge {
            resize_ratio.min(1.0)
        } else {
            resize_ratio
        };

        width = ((width as f32 * ratio).round() as u32).max(1);
        height = ((height as f32 * ratio).round() as u32).max(1);
    }

    let pixels = width as f64 * height as f64;

    let bytes = match settings.file_format {
        ExportFileFormat::Jpeg => pixels * (0.22 + (settings.jpeg_quality as f64 / 100.0) * 0.95),
        ExportFileFormat::Png => pixels * 1.45,
        ExportFileFormat::Tiff => pixels * 6.0,
        ExportFileFormat::Webp => pixels * 0.18,
    };

    bytes.round() as u64
}

fn effective_source_dimensions(
    width: u32,
    height: u32,
    orientation_steps: u8,
) -> Option<(u32, u32)> {
    if width == 0 || height == 0 {
        return None;
    }

    if orientation_steps % 2 == 1 {
        Some((height, width))
    } else {
        Some((width, height))
    }
}

fn crop_rect_for_ratio(dimensions: Option<(u32, u32)>, ratio: Option<f32>) -> Option<CropRect> {
    let (width, height) = dimensions?;
    let ratio = ratio?;
    if width == 0 || height == 0 || ratio <= 0.0 {
        return None;
    }

    let image_ratio = width as f32 / height as f32;
    if image_ratio > ratio {
        let crop_width = height as f32 * ratio;
        Some(CropRect {
            x: ((width as f32 - crop_width) * 0.5).round().clamp(0.0, width as f32),
            y: 0.0,
            width: crop_width.round().clamp(1.0, width as f32),
            height: height as f32,
        })
    } else {
        let crop_height = width as f32 / ratio;
        Some(CropRect {
            x: 0.0,
            y: ((height as f32 - crop_height) * 0.5).round().clamp(0.0, height as f32),
            width: width as f32,
            height: crop_height.round().clamp(1.0, height as f32),
        })
    }
}

fn full_crop_rect(dimensions: Option<(u32, u32)>) -> Option<CropRect> {
    let (width, height) = dimensions?;
    if width == 0 || height == 0 {
        return None;
    }

    Some(CropRect {
        x: 0.0,
        y: 0.0,
        width: width as f32,
        height: height as f32,
    })
}

fn parse_custom_crop_ratio(width: &str, height: &str) -> Option<f32> {
    let width = width.trim().parse::<f32>().ok()?;
    let height = height.trim().parse::<f32>().ok()?;
    if width > 0.0 && height > 0.0 {
        Some(width / height)
    } else {
        None
    }
}

fn trim_ratio_value(value: f32) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if (rounded.fract()).abs() < 0.001 {
        format!("{}", rounded.round() as i32)
    } else {
        format!("{rounded:.1}")
    }
}

fn ratio_matches(current: Option<f32>, target: f32) -> bool {
    matches!(current, Some(value) if (value - target).abs() < CROP_RATIO_TOLERANCE)
        || matches!(current, Some(value) if (value - (1.0 / target)).abs() < CROP_RATIO_TOLERANCE)
}

fn scale_crop_rect(crop: CropRect, source_dimensions: (u32, u32), image_dimensions: (u32, u32)) -> CropRect {
    let scale_x = image_dimensions.0 as f32 / source_dimensions.0.max(1) as f32;
    let scale_y = image_dimensions.1 as f32 / source_dimensions.1.max(1) as f32;

    CropRect {
        x: crop.x * scale_x,
        y: crop.y * scale_y,
        width: crop.width * scale_x,
        height: crop.height * scale_y,
    }
}

fn fitted_rect(bounds: Size, image_size: (u32, u32)) -> Rectangle {
    let image_width = image_size.0.max(1) as f32;
    let image_height = image_size.1.max(1) as f32;
    let scale = (bounds.width / image_width).min(bounds.height / image_height);
    let width = image_width * scale;
    let height = image_height * scale;
    Rectangle {
        x: (bounds.width - width) * 0.5,
        y: (bounds.height - height) * 0.5,
        width,
        height,
    }
}

fn crop_overlay_rect_in_bounds(crop: CropRect, image_size: (u32, u32), bounds: Size) -> Rectangle {
    let image_rect = fitted_rect(bounds, image_size);
    let scale_x = image_rect.width / image_size.0.max(1) as f32;
    let scale_y = image_rect.height / image_size.1.max(1) as f32;
    Rectangle {
        x: image_rect.x + crop.x * scale_x,
        y: image_rect.y + crop.y * scale_y,
        width: crop.width * scale_x,
        height: crop.height * scale_y,
    }
}

fn crop_rect_from_overlay_rect(rect: Rectangle, image_size: (u32, u32), bounds: Size) -> CropRect {
    let image_rect = fitted_rect(bounds, image_size);
    let scale_x = image_size.0.max(1) as f32 / image_rect.width.max(1.0);
    let scale_y = image_size.1.max(1) as f32 / image_rect.height.max(1.0);

    let min_x = rect.x.max(image_rect.x);
    let min_y = rect.y.max(image_rect.y);
    let max_x = (rect.x + rect.width).min(image_rect.x + image_rect.width);
    let max_y = (rect.y + rect.height).min(image_rect.y + image_rect.height);
    let overlay_width = (max_x - min_x).max(0.0);
    let overlay_height = (max_y - min_y).max(0.0);

    CropRect {
        x: safe_clamp((min_x - image_rect.x) * scale_x, 0.0, image_size.0 as f32),
        y: safe_clamp((min_y - image_rect.y) * scale_y, 0.0, image_size.1 as f32),
        width: safe_clamp(overlay_width * scale_x, 1.0, image_size.0 as f32),
        height: safe_clamp(overlay_height * scale_y, 1.0, image_size.1 as f32),
    }
}

fn point_in_rect(point: Point, rect: Rectangle) -> bool {
    point.x >= rect.x
        && point.x <= rect.x + rect.width
        && point.y >= rect.y
        && point.y <= rect.y + rect.height
}

fn clamp_point_to_rect(point: Point, rect: Rectangle) -> Point {
    Point::new(
        safe_clamp(point.x, rect.x, rect.x + rect.width),
        safe_clamp(point.y, rect.y, rect.y + rect.height),
    )
}

fn ruler_rotation_from_line(start: Point, end: Point) -> Option<f32> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    if (dx * dx + dy * dy).sqrt() < 8.0 {
        return None;
    }

    let angle_degrees = dy.atan2(dx).to_degrees();
    Some((-angle_degrees).clamp(-45.0, 45.0))
}

fn move_crop_overlay_rect(rect: Rectangle, image_rect: Rectangle, delta: Point) -> Rectangle {
    let mut x = rect.x + delta.x;
    let mut y = rect.y + delta.y;
    x = safe_clamp(x, image_rect.x, image_rect.x + image_rect.width - rect.width);
    y = safe_clamp(y, image_rect.y, image_rect.y + image_rect.height - rect.height);
    Rectangle { x, y, ..rect }
}

fn resize_crop_overlay_rect(
    rect: Rectangle,
    image_rect: Rectangle,
    handle: usize,
    delta: Point,
    locked_ratio: Option<f32>,
) -> Rectangle {
    if let Some(ratio) = locked_ratio.filter(|ratio| *ratio > 0.0) {
        return resize_crop_overlay_rect_locked(rect, image_rect, handle, delta, ratio);
    }

    let min_size = 40.0_f32
        .min(image_rect.width.max(1.0))
        .min(image_rect.height.max(1.0));
    let mut left = rect.x;
    let mut top = rect.y;
    let mut right = rect.x + rect.width;
    let mut bottom = rect.y + rect.height;

    match handle {
        0 => {
            left += delta.x;
            top += delta.y;
        }
        1 => {
            top += delta.y;
        }
        2 => {
            right += delta.x;
            top += delta.y;
        }
        3 => {
            left += delta.x;
        }
        4 => {
            right += delta.x;
        }
        5 => {
            left += delta.x;
            bottom += delta.y;
        }
        6 => {
            bottom += delta.y;
        }
        7 => {
            right += delta.x;
            bottom += delta.y;
        }
        _ => {}
    }

    let max_left = (right - min_size).max(image_rect.x);
    let max_top = (bottom - min_size).max(image_rect.y);
    left = safe_clamp(left, image_rect.x, max_left);
    top = safe_clamp(top, image_rect.y, max_top);

    let min_right = (left + min_size).min(image_rect.x + image_rect.width);
    let min_bottom = (top + min_size).min(image_rect.y + image_rect.height);
    right = safe_clamp(right, min_right, image_rect.x + image_rect.width);
    bottom = safe_clamp(bottom, min_bottom, image_rect.y + image_rect.height);

    Rectangle {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

fn resize_crop_overlay_rect_locked(
    rect: Rectangle,
    image_rect: Rectangle,
    handle: usize,
    delta: Point,
    ratio: f32,
) -> Rectangle {
    let ratio = ratio.max(0.001);
    let min_width = 40.0_f32.max(40.0 * ratio).min(image_rect.width.max(1.0));
    let min_height = 40.0_f32.max(40.0 / ratio).min(image_rect.height.max(1.0));

    let left = rect.x;
    let top = rect.y;
    let right = rect.x + rect.width;
    let bottom = rect.y + rect.height;
    let center_x = left + rect.width * 0.5;
    let center_y = top + rect.height * 0.5;

    match handle {
        0 | 2 | 5 | 7 => resize_crop_overlay_rect_locked_corner(
            image_rect,
            handle,
            Point::new(left, top),
            Point::new(right, bottom),
            delta,
            ratio,
            min_width,
            min_height,
        ),
        1 => {
            let proposed_top = top + delta.y;
            let max_half_width = (center_x - image_rect.x)
                .min(image_rect.x + image_rect.width - center_x)
                .max(0.0);
            let max_width = (max_half_width * 2.0).max(1.0);
            let max_height = (bottom - image_rect.y).min(max_width / ratio);
            let height = safe_clamp(bottom - proposed_top, min_height, max_height);
            let width = safe_clamp(height * ratio, min_width, max_width);
            clamp_rect_to_image(
                Rectangle {
                x: center_x - width * 0.5,
                y: bottom - height,
                width,
                height,
            },
                image_rect,
            )
        }
        6 => {
            let proposed_bottom = bottom + delta.y;
            let max_half_width = (center_x - image_rect.x)
                .min(image_rect.x + image_rect.width - center_x)
                .max(0.0);
            let max_width = (max_half_width * 2.0).max(1.0);
            let max_height = (image_rect.y + image_rect.height - top).min(max_width / ratio);
            let height = safe_clamp(proposed_bottom - top, min_height, max_height);
            let width = safe_clamp(height * ratio, min_width, max_width);
            clamp_rect_to_image(
                Rectangle {
                x: center_x - width * 0.5,
                y: top,
                width,
                height,
            },
                image_rect,
            )
        }
        3 => {
            let proposed_left = left + delta.x;
            let max_half_height = (center_y - image_rect.y)
                .min(image_rect.y + image_rect.height - center_y)
                .max(0.0);
            let max_height = (max_half_height * 2.0).max(1.0);
            let max_width = (right - image_rect.x).min(max_height * ratio);
            let width = safe_clamp(right - proposed_left, min_width, max_width);
            let height = safe_clamp(width / ratio, min_height, max_height);
            clamp_rect_to_image(
                Rectangle {
                x: right - width,
                y: center_y - height * 0.5,
                width,
                height,
            },
                image_rect,
            )
        }
        4 => {
            let proposed_right = right + delta.x;
            let max_half_height = (center_y - image_rect.y)
                .min(image_rect.y + image_rect.height - center_y)
                .max(0.0);
            let max_height = (max_half_height * 2.0).max(1.0);
            let max_width = (image_rect.x + image_rect.width - left).min(max_height * ratio);
            let width = safe_clamp(proposed_right - left, min_width, max_width);
            let height = safe_clamp(width / ratio, min_height, max_height);
            clamp_rect_to_image(
                Rectangle {
                x: left,
                y: center_y - height * 0.5,
                width,
                height,
            },
                image_rect,
            )
        }
        _ => rect,
    }
}

fn resize_crop_overlay_rect_locked_corner(
    image_rect: Rectangle,
    handle: usize,
    top_left: Point,
    bottom_right: Point,
    delta: Point,
    ratio: f32,
    min_width: f32,
    min_height: f32,
) -> Rectangle {
    let (anchor_x, anchor_y, proposed_x, proposed_y, anchor_is_left, anchor_is_top) = match handle {
        0 => (
            bottom_right.x,
            bottom_right.y,
            top_left.x + delta.x,
            top_left.y + delta.y,
            false,
            false,
        ),
        2 => (
            top_left.x,
            bottom_right.y,
            bottom_right.x + delta.x,
            top_left.y + delta.y,
            true,
            false,
        ),
        5 => (
            bottom_right.x,
            top_left.y,
            top_left.x + delta.x,
            bottom_right.y + delta.y,
            false,
            true,
        ),
        7 => (
            top_left.x,
            top_left.y,
            bottom_right.x + delta.x,
            bottom_right.y + delta.y,
            true,
            true,
        ),
        _ => {
            return Rectangle {
                x: top_left.x,
                y: top_left.y,
                width: bottom_right.x - top_left.x,
                height: bottom_right.y - top_left.y,
            };
        }
    };

    let desired_width = if anchor_is_left {
        proposed_x - anchor_x
    } else {
        anchor_x - proposed_x
    };
    let desired_height = if anchor_is_top {
        proposed_y - anchor_y
    } else {
        anchor_y - proposed_y
    };

    let max_width = if anchor_is_left {
        (image_rect.x + image_rect.width - anchor_x).max(1.0)
    } else {
        (anchor_x - image_rect.x).max(1.0)
    };
    let max_height = if anchor_is_top {
        (image_rect.y + image_rect.height - anchor_y).max(1.0)
    } else {
        (anchor_y - image_rect.y).max(1.0)
    };
    let width_limit = max_width.min(max_height * ratio);
    let height_limit = max_height.min(max_width / ratio);

    // Project the cursor onto the aspect-locked diagonal so the dragged corner
    // stays visually attached to the mouse instead of drifting away.
    let projected_height =
        ((desired_width * ratio) + desired_height) / (ratio * ratio + 1.0);
    let height = safe_clamp(projected_height, min_height, height_limit);
    let width = safe_clamp(height * ratio, min_width, width_limit);
    let height = safe_clamp(width / ratio, min_height, height_limit);

    let left = if anchor_is_left { anchor_x } else { anchor_x - width };
    let top = if anchor_is_top { anchor_y } else { anchor_y - height };

    clamp_rect_to_image(
        Rectangle {
        x: left,
        y: top,
        width,
        height,
    },
        image_rect,
    )
}

fn safe_clamp(value: f32, min: f32, max: f32) -> f32 {
    if !value.is_finite() {
        return min.max(0.0);
    }

    let lower = min.min(max);
    let upper = min.max(max);
    value.clamp(lower, upper)
}

fn clamp_rect_to_image(rect: Rectangle, image_rect: Rectangle) -> Rectangle {
    let width = rect.width.min(image_rect.width).max(1.0);
    let height = rect.height.min(image_rect.height).max(1.0);
    let x = safe_clamp(rect.x, image_rect.x, image_rect.x + image_rect.width - width);
    let y = safe_clamp(rect.y, image_rect.y, image_rect.y + image_rect.height - height);

    Rectangle {
        x,
        y,
        width,
        height,
    }
}

fn crop_handle_rects(crop_rect: Rectangle) -> [Rectangle; 8] {
    let size = 10.0;
    let half = size * 0.5;
    let mid_x = crop_rect.x + crop_rect.width * 0.5;
    let mid_y = crop_rect.y + crop_rect.height * 0.5;

    [
        Rectangle { x: crop_rect.x, y: crop_rect.y, width: size, height: size },
        Rectangle { x: mid_x - half, y: crop_rect.y, width: size, height: size },
        Rectangle { x: crop_rect.x + crop_rect.width - size, y: crop_rect.y, width: size, height: size },
        Rectangle { x: crop_rect.x, y: mid_y - half, width: size, height: size },
        Rectangle { x: crop_rect.x + crop_rect.width - size, y: mid_y - half, width: size, height: size },
        Rectangle { x: crop_rect.x, y: crop_rect.y + crop_rect.height - size, width: size, height: size },
        Rectangle { x: mid_x - half, y: crop_rect.y + crop_rect.height - size, width: size, height: size },
        Rectangle { x: crop_rect.x + crop_rect.width - size, y: crop_rect.y + crop_rect.height - size, width: size, height: size },
    ]
}

fn apply_adjustment_transformations(
    image: &DynamicImage,
    adjustments: &BasicAdjustments,
    source_dimensions: (u32, u32),
    apply_crop: bool,
) -> DynamicImage {
    let mut transformed = match adjustments.orientation_steps % 4 {
        1 => image.rotate90(),
        2 => image.rotate180(),
        3 => image.rotate270(),
        _ => image.clone(),
    };

    if adjustments.flip_horizontal {
        transformed = transformed.fliph();
    }
    if adjustments.flip_vertical {
        transformed = transformed.flipv();
    }

    if adjustments.rotation.abs() > 0.01 {
        let rgba = transformed.to_rgba32f();
        let rotated = rotate_about_center(
            &rgba,
            adjustments.rotation.to_radians(),
            Interpolation::Bilinear,
            Rgba([0.0, 0.0, 0.0, 0.0]),
        );
        transformed = DynamicImage::ImageRgba32F(rotated);
    }

    if apply_crop {
        if let Some(crop) = adjustments.crop {
        if let Some(oriented_source_dimensions) = effective_source_dimensions(
            source_dimensions.0,
            source_dimensions.1,
            adjustments.orientation_steps,
        ) {
            let scaled = scale_crop_rect(crop, oriented_source_dimensions, transformed.dimensions());
            let x = scaled.x.round().max(0.0) as u32;
            let y = scaled.y.round().max(0.0) as u32;
            let width = scaled.width.round().max(1.0) as u32;
            let height = scaled.height.round().max(1.0) as u32;
            let max_width = transformed.width().saturating_sub(x);
            let max_height = transformed.height().saturating_sub(y);
            if max_width > 0 && max_height > 0 {
                transformed = transformed.crop_imm(x, y, width.min(max_width), height.min(max_height));
            }
        }
    }
    }

    transformed
}

async fn export_images_task(
    output_folder: PathBuf,
    jobs: Vec<ExportJob>,
    settings: ExportSettingsUi,
    renderer: RapidRawRenderer,
) -> Result<ExportOutcome, String> {
    export_images(&output_folder, &jobs, &settings, &renderer)?;
    Ok(ExportOutcome {
        exported_count: jobs.len(),
        output_folder,
    })
}

fn export_images(
    output_folder: &Path,
    jobs: &[ExportJob],
    settings: &ExportSettingsUi,
    renderer: &RapidRawRenderer,
) -> Result<(), String> {
    fs::create_dir_all(output_folder).map_err(|error| {
        format!(
            "Failed to create export folder {}: {}",
            output_folder.display(),
            error
        )
    })?;

    for job in jobs {
        let source_image = load_full_resolution_image(&job.path, job.is_raw)?;
        let source_dimensions = source_image.dimensions();
        let transformed = apply_adjustment_transformations(
            &source_image,
            &job.adjustments,
            source_dimensions,
            true,
        );
        let rendered = renderer.render(&transformed, &job.adjustments, job.is_raw)?;
        let resized = apply_export_resize(&rendered, settings);
        let final_image = apply_export_watermark(resized, settings)?;
        let output_path = make_export_output_path(output_folder, &job.path, settings.file_format);
        save_export_image(&final_image, &output_path, settings)?;
    }

    Ok(())
}

fn load_full_resolution_image(path: &Path, is_raw: bool) -> Result<DynamicImage, String> {
    if is_raw {
        decode_raw_preview(path)
    } else {
        open_image(path).map_err(|error| format!("Failed to open {}: {}", path.display(), error))
    }
}

fn apply_export_resize(image: &DynamicImage, settings: &ExportSettingsUi) -> DynamicImage {
    if !settings.enable_resize {
        return image.clone();
    }

    let (current_w, current_h) = image.dimensions();
    let (target_w, target_h) = calculate_export_target(current_w, current_h, settings);
    if target_w == current_w && target_h == current_h {
        image.clone()
    } else {
        image.resize(target_w, target_h, FilterType::Lanczos3)
    }
}

fn calculate_export_target(
    current_w: u32,
    current_h: u32,
    settings: &ExportSettingsUi,
) -> (u32, u32) {
    let value = settings.resize_value.round().clamp(1.0, 8192.0) as u32;

    if settings.dont_enlarge {
        let exceeds = match settings.resize_mode {
            ExportResizeMode::LongEdge => current_w.max(current_h) > value,
            ExportResizeMode::ShortEdge => current_w.min(current_h) > value,
            ExportResizeMode::Width => current_w > value,
            ExportResizeMode::Height => current_h > value,
        };
        if !exceeds {
            return (current_w, current_h);
        }
    }

    let fix_width = match settings.resize_mode {
        ExportResizeMode::LongEdge => current_w >= current_h,
        ExportResizeMode::ShortEdge => current_w <= current_h,
        ExportResizeMode::Width => true,
        ExportResizeMode::Height => false,
    };

    if fix_width {
        let target_h = (value as f32 * (current_h as f32 / current_w.max(1) as f32)).round() as u32;
        (value.max(1), target_h.max(1))
    } else {
        let target_w = (value as f32 * (current_w as f32 / current_h.max(1) as f32)).round() as u32;
        (target_w.max(1), value.max(1))
    }
}

fn apply_export_watermark(
    mut image: DynamicImage,
    settings: &ExportSettingsUi,
) -> Result<DynamicImage, String> {
    if !settings.enable_watermark || settings.watermark_path.trim().is_empty() {
        return Ok(image);
    }

    let watermark_image = open_image(Path::new(settings.watermark_path.trim())).map_err(|error| {
        format!(
            "Failed to open watermark image {}: {}",
            settings.watermark_path.trim(),
            error
        )
    })?;

    let (base_w, base_h) = image.dimensions();
    let base_min_dim = base_w.min(base_h) as f32;
    let scale = (base_min_dim * (settings.watermark_scale / 100.0))
        / watermark_image.width().max(1) as f32;
    let target_w = (watermark_image.width() as f32 * scale).round() as u32;
    let target_h = (watermark_image.height() as f32 * scale).round() as u32;

    if target_w == 0 || target_h == 0 {
        return Ok(image);
    }

    let resized = watermark_image.resize_exact(target_w, target_h, FilterType::Lanczos3);
    let mut watermark_rgba = resized.to_rgba8();
    let opacity = (settings.watermark_opacity / 100.0).clamp(0.0, 1.0);
    for pixel in watermark_rgba.pixels_mut() {
        pixel[3] = (pixel[3] as f32 * opacity).round().clamp(0.0, 255.0) as u8;
    }

    let watermark = DynamicImage::ImageRgba8(watermark_rgba);
    let spacing = (base_min_dim * (settings.watermark_spacing / 100.0)).round() as i64;
    let (wm_w, wm_h) = watermark.dimensions();

    let x = match settings.watermark_anchor {
        WatermarkAnchor::TopLeft | WatermarkAnchor::CenterLeft | WatermarkAnchor::BottomLeft => {
            spacing
        }
        WatermarkAnchor::TopCenter | WatermarkAnchor::Center | WatermarkAnchor::BottomCenter => {
            (base_w as i64 - wm_w as i64) / 2
        }
        WatermarkAnchor::TopRight | WatermarkAnchor::CenterRight | WatermarkAnchor::BottomRight => {
            base_w as i64 - wm_w as i64 - spacing
        }
    };

    let y = match settings.watermark_anchor {
        WatermarkAnchor::TopLeft | WatermarkAnchor::TopCenter | WatermarkAnchor::TopRight => {
            spacing
        }
        WatermarkAnchor::CenterLeft | WatermarkAnchor::Center | WatermarkAnchor::CenterRight => {
            (base_h as i64 - wm_h as i64) / 2
        }
        WatermarkAnchor::BottomLeft
        | WatermarkAnchor::BottomCenter
        | WatermarkAnchor::BottomRight => base_h as i64 - wm_h as i64 - spacing,
    };

    ::image::imageops::overlay(&mut image, &watermark, x, y);
    Ok(image)
}

fn make_export_output_path(
    output_folder: &Path,
    source_path: &Path,
    format: ExportFileFormat,
) -> PathBuf {
    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("untitled");
    let extension = export_extension(format);
    let mut candidate = output_folder.join(format!("{stem}.{extension}"));
    let mut suffix = 2;

    while candidate.exists() {
        candidate = output_folder.join(format!("{stem}-{suffix}.{extension}"));
        suffix += 1;
    }

    candidate
}

fn export_extension(format: ExportFileFormat) -> &'static str {
    match format {
        ExportFileFormat::Jpeg => "jpg",
        ExportFileFormat::Png => "png",
        ExportFileFormat::Tiff => "tiff",
        ExportFileFormat::Webp => "webp",
    }
}

fn save_export_image(
    image: &DynamicImage,
    output_path: &Path,
    settings: &ExportSettingsUi,
) -> Result<(), String> {
    let mut encoded = Vec::new();

    match settings.file_format {
        ExportFileFormat::Jpeg => {
            let mut encoder = ::image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut encoded,
                settings.jpeg_quality.round().clamp(1.0, 100.0) as u8,
            );
            encoder
                .encode_image(image)
                .map_err(|error| format!("Failed to encode JPEG: {}", error))?;
        }
        ExportFileFormat::Png => {
            let mut cursor = std::io::Cursor::new(&mut encoded);
            image
                .write_to(&mut cursor, ::image::ImageFormat::Png)
                .map_err(|error| format!("Failed to encode PNG: {}", error))?;
        }
        ExportFileFormat::Tiff => {
            let mut cursor = std::io::Cursor::new(&mut encoded);
            image
                .write_to(&mut cursor, ::image::ImageFormat::Tiff)
                .map_err(|error| format!("Failed to encode TIFF: {}", error))?;
        }
        ExportFileFormat::Webp => {
            let mut cursor = std::io::Cursor::new(&mut encoded);
            image
                .write_to(&mut cursor, ::image::ImageFormat::WebP)
                .map_err(|error| format!("Failed to encode WebP: {}", error))?;
        }
    }

    fs::write(output_path, encoded)
        .map_err(|error| format!("Failed to write {}: {}", output_path.display(), error))
}

fn format_estimated_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let bytes_f = bytes as f64;
    if bytes_f >= GB {
        format!("{:.1} GB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.1} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.0} KB", bytes_f / KB)
    } else {
        format!("{bytes} B")
    }
}

fn color_grading_zone_mut(
    settings: &mut ColorGradingSettingsUi,
    zone: ColorGradingZone,
) -> &mut HueSatLum {
    match zone {
        ColorGradingZone::Shadows => &mut settings.shadows,
        ColorGradingZone::Midtones => &mut settings.midtones,
        ColorGradingZone::Highlights => &mut settings.highlights,
    }
}

fn hsl_band_color(band: HslBand) -> Color {
    match band {
        HslBand::Reds => Color::from_rgb8(0xf8, 0x71, 0x71),
        HslBand::Oranges => Color::from_rgb8(0xfb, 0x92, 0x3c),
        HslBand::Yellows => Color::from_rgb8(0xfa, 0xcc, 0x15),
        HslBand::Greens => Color::from_rgb8(0x4a, 0xde, 0x80),
        HslBand::Aquas => Color::from_rgb8(0x2d, 0xd4, 0xbf),
        HslBand::Blues => Color::from_rgb8(0x60, 0xa5, 0xfa),
        HslBand::Purples => Color::from_rgb8(0xa7, 0x8b, 0xfa),
        HslBand::Magentas => Color::from_rgb8(0xf4, 0x72, 0xb6),
    }
}

fn hsl_band_label(band: HslBand) -> &'static str {
    match band {
        HslBand::Reds => "Reds",
        HslBand::Oranges => "Oranges",
        HslBand::Yellows => "Yellows",
        HslBand::Greens => "Greens",
        HslBand::Aquas => "Aquas",
        HslBand::Blues => "Blues",
        HslBand::Purples => "Purples",
        HslBand::Magentas => "Magentas",
    }
}

fn panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(Color::WHITE),
        background: Some(Background::Color(Color::from_rgb8(0x14, 0x19, 0x23))),
        border: iced::Border::default().rounded(20.0),
        ..container::Style::default()
    }
}

fn discrete_scrollbar_style(_theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let scroller_color = match status {
        scrollable::Status::Active => Color::from_rgba8(0xff, 0xff, 0xff, 0.18),
        scrollable::Status::Hovered { .. } => Color::from_rgba8(0xff, 0xff, 0xff, 0.28),
        scrollable::Status::Dragged { .. } => Color::from_rgba8(0xff, 0xff, 0xff, 0.38),
    };

    let rail = scrollable::Rail {
        background: Some(Background::Color(Color::from_rgba8(0xff, 0xff, 0xff, 0.05))),
        border: Border::default().rounded(999.0),
        scroller: scrollable::Scroller {
            color: scroller_color,
            border: Border::default().rounded(999.0),
        },
    };

    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
    }
}

fn basic_slider<'a, F>(
    label: &'a str,
    min: f32,
    max: f32,
    value: f32,
    on_change: F,
) -> Element<'a, Message>
where
    F: 'a + Fn(f32) -> Message,
{
    column![
        row![
            text(label).size(15),
            Space::with_width(Length::Fill),
            text(format!("{value:.2}"))
                .size(14)
                .color(Color::from_rgb8(0xa8, 0xb2, 0xc8)),
        ]
        .align_y(iced::alignment::Vertical::Center),
        slider(min..=max, value, on_change)
            .step(0.01)
            .on_release(Message::CommitPreviewRender),
    ]
    .spacing(6)
    .into()
}

fn export_option_row<'a>(label: &'a str, control: Element<'a, Message>) -> Element<'a, Message> {
    container(
        column![
        text(label)
            .size(14)
            .color(Color::from_rgb8(0xa8, 0xb2, 0xc8)),
        control,
    ]
    .spacing(8),
    )
    .width(Length::Fill)
    .into()
}

fn export_choice_button<'a>(label: &'a str, active: bool, message: Message) -> Element<'a, Message> {
    button(
        text(label)
            .size(13)
            .color(if active {
                Color::WHITE
            } else {
                Color::from_rgb8(0xc2, 0xcb, 0xdd)
            }),
    )
    .padding([8, 10])
    .style(move |theme, status| {
        let mut style = iced::widget::button::secondary(theme, status);
        style.background = Some(Background::Color(if active {
            Color::from_rgb8(0x24, 0x5d, 0x88)
        } else {
            match status {
                iced::widget::button::Status::Hovered => Color::from_rgb8(0x2a, 0x33, 0x42),
                _ => Color::from_rgb8(0x24, 0x2d, 0x3a),
            }
        }));
        style.border.radius = 10.0.into();
        style.text_color = Color::WHITE;
        style
    })
    .on_press(message)
    .into()
}

fn crop_choice_button<'a>(label: &'a str, active: bool, message: Message) -> Element<'a, Message> {
    button(
        text(label)
            .size(13)
            .color(if active {
                Color::WHITE
            } else {
                Color::from_rgb8(0xc2, 0xcb, 0xdd)
            }),
    )
    .padding([8, 10])
    .width(Length::Fill)
    .style(move |theme, status| {
        let mut style = iced::widget::button::secondary(theme, status);
        style.background = Some(Background::Color(if active {
            Color::from_rgb8(0x24, 0x5d, 0x88)
        } else {
            match status {
                iced::widget::button::Status::Hovered => Color::from_rgb8(0x2a, 0x33, 0x42),
                _ => Color::from_rgb8(0x24, 0x2d, 0x3a),
            }
        }));
        style.border.radius = 10.0.into();
        style.text_color = Color::WHITE;
        style
    })
    .on_press(message)
    .into()
}

fn orientation_action_button<'a>(
    icon: AppIcon,
    label: &'a str,
    message: Message,
    active: bool,
) -> Element<'a, Message> {
    button(
        container(
            column![
                app_icon(icon, 18.0, Color::WHITE),
                text(label)
                    .size(11)
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center)
                    .color(if active {
                        Color::WHITE
                    } else {
                        Color::from_rgb8(0xc2, 0xcb, 0xdd)
                    }),
            ]
            .spacing(6)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center),
        )
        .width(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Shrink),
    )
    .width(Length::Fill)
    .height(Length::Fixed(92.0))
    .padding([10, 8])
    .style(move |theme, status| {
        let mut style = iced::widget::button::secondary(theme, status);
        style.background = Some(Background::Color(if active {
            Color::from_rgb8(0x24, 0x5d, 0x88)
        } else {
            match status {
                iced::widget::button::Status::Hovered => Color::from_rgb8(0x2a, 0x33, 0x42),
                _ => Color::from_rgb8(0x24, 0x2d, 0x3a),
            }
        }));
        style.border.radius = 12.0.into();
        style.text_color = Color::WHITE;
        style
    })
    .on_press(message)
    .into()
}

fn export_toggle_row<'a>(
    label: &'a str,
    _enabled: bool,
    progress: f32,
    message: Message,
) -> Element<'a, Message> {
    let progress = ease_in_out(progress);
    let left_space = 16.0 * progress;
    let right_space = 16.0 - left_space;
    let thumb: Element<'a, Message> = container(Space::with_width(Length::Fixed(14.0)))
        .width(Length::Fixed(14.0))
        .height(Length::Fixed(14.0))
        .style(|_| container::Style {
            background: Some(Background::Color(Color::WHITE)),
            border: Border::default().rounded(999.0),
            ..container::Style::default()
        })
        .into();

    let switch_row = row![
        Space::with_width(Length::Fixed(left_space)),
        thumb,
        Space::with_width(Length::Fixed(right_space))
    ]
    .align_y(iced::alignment::Vertical::Center);

    let track_color = lerp_color(
        Color::from_rgb8(0x2a, 0x31, 0x3d),
        Color::from_rgb8(0x24, 0x5d, 0x88),
        progress,
    );

    button(
        row![
            text(label)
                .size(14)
                .color(Color::from_rgb8(0xe7, 0xec, 0xf6)),
            Space::with_width(Length::Fill),
            container(switch_row)
                .width(Length::Fixed(38.0))
                .height(Length::Fixed(22.0))
                .padding(4)
                .style(move |_| container::Style {
                    background: Some(Background::Color(track_color)),
                    border: Border::default().rounded(999.0),
                    ..container::Style::default()
                }),
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .padding(0)
    .style(|theme, status| {
        let mut style = iced::widget::button::text(theme, status);
        if matches!(status, iced::widget::button::Status::Hovered) {
            style.background = Some(Background::Color(Color::from_rgb8(0x1d, 0x24, 0x31)));
        }
        style.border.radius = 10.0.into();
        style
    })
    .on_press(message)
    .into()
}

fn tone_mapper_button<'a>(
    label: &'a str,
    value: ToneMapper,
    selected: ToneMapper,
) -> Element<'a, Message> {
    let active = value == selected;
    button(text(label).size(14))
        .padding([10, 16])
        .style(move |theme, status| {
            let mut style = iced::widget::button::secondary(theme, status);
            style.background = Some(Background::Color(if active {
                Color::from_rgb8(0x6d, 0xb7, 0xff)
            } else {
                Color::from_rgb8(0x21, 0x28, 0x35)
            }));
            style.text_color = if active {
                Color::from_rgb8(0x08, 0x12, 0x20)
            } else {
                Color::WHITE
            };
            style.border.radius = 12.0.into();
            style
        })
        .on_press(Message::ToneMapperChanged(value))
        .into()
}

fn curve_channel_button<'a>(
    label: &'a str,
    channel: CurveChannel,
    selected: CurveChannel,
) -> Element<'a, Message> {
    let active = channel == selected;
    button(text(label).size(13))
        .padding([8, 12])
        .style(move |theme, status| {
            let mut style = iced::widget::button::secondary(theme, status);
            style.background = Some(Background::Color(if active {
                curve_channel_color(channel)
            } else {
                Color::from_rgb8(0x21, 0x28, 0x35)
            }));
            style.text_color = if active {
                Color::from_rgb8(0x08, 0x12, 0x20)
            } else {
                Color::WHITE
            };
            style.border.radius = 999.0.into();
            style
        })
        .on_press(Message::ActiveCurveChannelChanged(channel))
        .into()
}

fn adjustment_card<'a>(
    title: &'a str,
    card: CardAnimation,
    toggle_message: Message,
    body: Element<'a, Message>,
    expanded_height: f32,
) -> Element<'a, Message> {
    let expanded = card.expanded;
    let eased_progress = ease_in_out(card.progress);
    let body_spacing = if card.progress > 0.01 { 12 } else { 0 };
    let body_content: Element<'a, Message> = if card.progress >= 0.99 {
        container(body).into()
    } else if card.progress > 0.01 {
        container(body)
            .height(Length::Fixed((expanded_height * eased_progress).max(1.0)))
            .into()
    } else {
        Space::with_height(Length::Shrink).into()
    };

    let is_collapsed = card.progress <= 0.01;

    let header = button(
        container(
            row![
                text(title).size(20),
                Space::with_width(Length::Fill),
                app_icon(
                    if expanded {
                        AppIcon::ChevronUp
                    } else {
                        AppIcon::ChevronDown
                    },
                    18.0,
                    Color::from_rgb8(0xa8, 0xb2, 0xc8),
                ),
            ]
            .align_y(iced::alignment::Vertical::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(iced::alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(52.0))
    .padding([0, 12])
    .style(|theme, status| {
        let mut style = iced::widget::button::text(theme, status);
        let background = match status {
            iced::widget::button::Status::Active => Color::from_rgb8(0x1d, 0x24, 0x31),
            iced::widget::button::Status::Hovered => Color::from_rgb8(0x23, 0x2b, 0x3a),
            iced::widget::button::Status::Pressed => Color::from_rgb8(0x26, 0x30, 0x40),
            iced::widget::button::Status::Disabled => Color::from_rgb8(0x1d, 0x24, 0x31),
        };
        style.background = Some(Background::Color(background));
        style.border.radius = 16.0.into();
        style
    })
    .on_press(toggle_message);

    container(
        column![
            header,
            container(body_content).padding(if is_collapsed { 0 } else { 16 })
        ]
        .spacing(body_spacing),
    )
    .style(|_| container::Style {
        text_color: Some(Color::WHITE),
        background: Some(Background::Color(Color::from_rgb8(0x17, 0x1c, 0x27))),
        border: Border::default().rounded(18.0),
        ..container::Style::default()
    })
    .into()
}

fn app_icon<'a>(icon: AppIcon, size: f32, color: Color) -> Element<'a, Message> {
    text(char::from(lucide_icon(icon)).to_string())
        .font(Font::with_name("lucide"))
        .size(size)
        .color(color)
        .into()
}

fn lucide_icon(icon: AppIcon) -> LucideIcon {
    match icon {
        AppIcon::ArrowLeft => LucideIcon::ArrowLeft,
        AppIcon::Check => LucideIcon::Check,
        AppIcon::ChevronDown => LucideIcon::ChevronDown,
        AppIcon::ChevronUp => LucideIcon::ChevronUp,
        AppIcon::Crop => LucideIcon::Scaling,
        AppIcon::SlidersHorizontal => LucideIcon::SlidersHorizontal,
        AppIcon::Share => LucideIcon::Share,
        AppIcon::FolderOpen => LucideIcon::FolderOpen,
        AppIcon::RotateCcw => LucideIcon::Undo2,
        AppIcon::RotateCw => LucideIcon::RotateCw,
        AppIcon::Ruler => LucideIcon::Ruler,
        AppIcon::FlipHorizontal => LucideIcon::FlipHorizontal2,
        AppIcon::FlipVertical => LucideIcon::FlipVertical2,
        AppIcon::RectangleHorizontal => LucideIcon::RectangleHorizontal,
        AppIcon::RectangleVertical => LucideIcon::RectangleVertical,
        AppIcon::Star => LucideIcon::Star,
        AppIcon::X => LucideIcon::X,
    }
}

fn sidebar_tab_button<'a>(
    icon: AppIcon,
    active: bool,
    message: Message,
    tip: &'a str,
) -> Element<'a, Message> {
    let button = button(
        container(app_icon(
            icon,
            16.0,
            if active {
                Color::WHITE
            } else {
                Color::from_rgb8(0xa8, 0xb2, 0xc8)
            },
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center),
    )
    .width(Length::Fixed(36.0))
    .height(Length::Fixed(36.0))
    .padding(0)
    .style(move |theme, status| {
        let mut style = iced::widget::button::secondary(theme, status);
        style.background = Some(Background::Color(if active {
            Color::from_rgb8(0x24, 0x2d, 0x3c)
        } else {
            match status {
                iced::widget::button::Status::Hovered => Color::from_rgb8(0x1d, 0x24, 0x31),
                _ => Color::TRANSPARENT,
            }
        }));
        style.border.radius = 12.0.into();
        style.text_color = Color::WHITE;
        style
    })
    .on_press(message);

    tooltip(
        button,
        container(text(tip).size(12).color(Color::from_rgb8(0xe2, 0xe8, 0xf0)))
            .padding([6, 10])
            .style(|_| container::Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(Color::from_rgb8(0x0f, 0x14, 0x1d))),
                border: Border::default().rounded(10.0),
                ..container::Style::default()
            }),
        tooltip::Position::Bottom,
    )
    .gap(8)
    .into()
}

fn icon_button<'a>(icon: AppIcon, message: Message, _title: &'a str) -> Element<'a, Message> {
    button(
        container(app_icon(icon, 16.0, Color::from_rgb8(0xe2, 0xe8, 0xf0)))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .width(Length::Fixed(34.0))
    .height(Length::Fixed(34.0))
    .padding(0)
    .style(move |theme, status| {
        let mut style = iced::widget::button::secondary(theme, status);
        style.background = Some(Background::Color(Color::from_rgb8(0x21, 0x28, 0x35)));
        style.text_color = Color::from_rgb8(0xe2, 0xe8, 0xf0);
        style.border.radius = 999.0.into();
        style
    })
    .on_press(message)
    .into()
}

fn crop_rotation_icon_button<'a>(
    icon: AppIcon,
    active: bool,
    message: Message,
    tip: &'a str,
) -> Element<'a, Message> {
    let button = button(
        container(app_icon(
            icon,
            18.0,
            if active {
                Color::WHITE
            } else {
                Color::from_rgb8(0xb1, 0xba, 0xce)
            },
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center),
    )
    .width(Length::Fixed(34.0))
    .height(Length::Fixed(34.0))
    .padding(0)
    .style(move |theme, status| {
        let mut style = iced::widget::button::secondary(theme, status);
        let background = if active {
            Color::from_rgb8(0x24, 0x5d, 0x88)
        } else {
            match status {
                iced::widget::button::Status::Hovered => Color::from_rgb8(0x22, 0x28, 0x34),
                iced::widget::button::Status::Pressed => Color::from_rgb8(0x26, 0x2d, 0x39),
                _ => Color::from_rgb8(0x1b, 0x21, 0x2c),
            }
        };
        style.background = Some(Background::Color(background));
        style.border.radius = 999.0.into();
        style.text_color = Color::WHITE;
        style
    })
    .on_press(message);

    tooltip(
        button,
        container(text(tip).size(12).color(Color::from_rgb8(0xe2, 0xe8, 0xf0)))
            .padding([6, 10])
            .style(|_| container::Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(Color::from_rgb8(0x0f, 0x14, 0x1d))),
                border: Border::default().rounded(10.0),
                ..container::Style::default()
            }),
        tooltip::Position::Bottom,
    )
    .gap(8)
    .into()
}

fn lut_picker_button<'a>(label: &'a str, has_lut: bool) -> Element<'a, Message> {
    let button = button(text(label).size(14).color(if has_lut {
        Color::from_rgb8(0xe7, 0xec, 0xf6)
    } else {
        Color::from_rgb8(0xa8, 0xb2, 0xc8)
    }))
    .width(Length::Fill)
    .padding([10, 12])
    .style(|theme, status| {
        let mut style = iced::widget::button::secondary(theme, status);
        style.background = Some(Background::Color(Color::from_rgb8(0x21, 0x28, 0x35)));
        style.text_color = Color::WHITE;
        style.border.radius = 12.0.into();
        style
    })
    .on_press(Message::SelectLut);

    tooltip(
        button,
        container(
            text(if has_lut {
                "Choose a different LUT"
            } else {
                "Select a LUT file"
            })
            .size(12)
            .color(Color::from_rgb8(0xe2, 0xe8, 0xf0)),
        )
        .padding([6, 10])
        .style(|_| container::Style {
            text_color: Some(Color::WHITE),
            background: Some(Background::Color(Color::from_rgb8(0x0f, 0x14, 0x1d))),
            border: Border::default().rounded(10.0),
            ..container::Style::default()
        }),
        tooltip::Position::Top,
    )
    .gap(8)
    .into()
}

fn lut_browser_list<'a>(
    browser: &'a LutBrowserState,
    selected_lut_path: Option<&'a str>,
) -> Element<'a, Message> {
    let folder_label = browser
        .folder
        .as_ref()
        .and_then(|path| path.file_name().and_then(|name| name.to_str()))
        .unwrap_or("LUT Folder");

    let visible_entries = if browser.collapsed {
        browser
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| selected_lut_path == Some(entry.path.to_string_lossy().as_ref()))
            .collect::<Vec<_>>()
    } else {
        browser.entries.iter().enumerate().collect::<Vec<_>>()
    };

    visible_entries
        .into_iter()
        .fold(
            column![muted_line(format!(
                "{folder_label} • {} LUTs",
                browser.entries.len()
            ))]
            .spacing(6),
            |column, (index, entry)| {
                column.push(lut_browser_item(
                    index,
                    entry,
                    selected_lut_path == Some(entry.path.to_string_lossy().as_ref()),
                    !browser.collapsed && browser.hovered_index == Some(index),
                    browser.collapsed,
                ))
            },
        )
        .into()
}

fn lut_browser_item<'a>(
    index: usize,
    entry: &'a LutListEntry,
    selected: bool,
    hovered: bool,
    collapsed: bool,
) -> Element<'a, Message> {
    let background = if hovered {
        Color::from_rgb8(0x2a, 0x34, 0x46)
    } else if selected {
        Color::from_rgb8(0x1f, 0x2a, 0x3a)
    } else {
        Color::from_rgb8(0x1b, 0x22, 0x2f)
    };

    let content = container(
        row![
            text(&entry.name)
                .size(14)
                .color(Color::from_rgb8(0xe7, 0xec, 0xf6)),
            Space::with_width(Length::Fill),
            if selected && collapsed {
                lut_selected_check()
            } else {
                Space::with_width(Length::Shrink).into()
            },
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .padding([9, 12])
    .style(move |_| container::Style {
        text_color: Some(Color::WHITE),
        background: Some(Background::Color(background)),
        border: Border::default().rounded(12.0),
        ..container::Style::default()
    });

    mouse_area(content)
        .on_enter(Message::HoverLut(Some(index)))
        .on_exit(Message::HoverLut(None))
        .on_press(Message::ApplyLutFromBrowser(index))
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

fn card_section<'a>(title: &'a str, body: Element<'a, Message>) -> Element<'a, Message> {
    container(
        column![
            text(title)
                .size(15)
                .color(Color::from_rgb8(0xe7, 0xec, 0xf6)),
            body,
        ]
        .spacing(10),
    )
    .width(Length::Fill)
    .padding(12)
    .style(|_| container::Style {
        text_color: Some(Color::WHITE),
        background: Some(Background::Color(Color::from_rgb8(0x1d, 0x23, 0x2f))),
        border: Border::default().rounded(14.0),
        ..container::Style::default()
    })
    .into()
}

fn muted_line<'a>(content: impl Into<String>) -> Element<'a, Message> {
    text(content.into())
        .size(13)
        .color(Color::from_rgb8(0x8d, 0x98, 0xae))
        .into()
}

fn lut_selected_check<'a>() -> Element<'a, Message> {
    app_icon(AppIcon::ChevronDown, 14.0, Color::WHITE)
}

fn color_swatch_button<'a>(band: HslBand, selected: HslBand) -> Element<'a, Message> {
    let active = band == selected;
    let fill = hsl_band_color(band);
    let swatch = button(
        container(Space::with_width(Length::Shrink))
            .width(Length::Fixed(18.0))
            .height(Length::Fixed(18.0))
            .style(move |_| container::Style {
                text_color: None,
                background: Some(Background::Color(Color {
                    a: 1.0,
                    ..fill.scale_alpha(0.9)
                })),
                border: Border {
                    color: if active {
                        Color::WHITE
                    } else {
                        Color::TRANSPARENT
                    },
                    width: 2.0,
                    radius: 999.0.into(),
                },
                ..container::Style::default()
            }),
    )
    .width(Length::Fill)
    .height(Length::Fixed(28.0))
    .style(move |theme, status| {
        let mut style = iced::widget::button::secondary(theme, status);
        style.background = Some(Background::Color(Color::TRANSPARENT));
        style.border = Border::default().rounded(999.0);
        style
    })
    .on_press(Message::ActiveHslBandChanged(band));

    tooltip(
        swatch,
        container(
            text(hsl_band_label(band))
                .size(12)
                .color(Color::from_rgb8(0xe2, 0xe8, 0xf0)),
        )
        .padding([6, 10])
        .style(|_| container::Style {
            text_color: Some(Color::WHITE),
            background: Some(Background::Color(Color::from_rgb8(0x0f, 0x14, 0x1d))),
            border: Border::default().rounded(10.0),
            ..container::Style::default()
        }),
        tooltip::Position::Top,
    )
    .gap(8)
    .into()
}

fn color_grading_wheel_panel<'a>(
    label: &'a str,
    zone: ColorGradingZone,
    value: HueSatLum,
    wheel_size: f32,
) -> Element<'a, Message> {
    let wheel = canvas::Canvas::new(ColorWheelEditor { zone, value })
        .width(Length::Fixed(wheel_size))
        .height(Length::Fixed(wheel_size));
    let wheel_background = image(color_wheel_handle(wheel_size.round() as u32))
        .width(Length::Fixed(wheel_size))
        .height(Length::Fixed(wheel_size));

    let content = column![
        text(label)
            .size(15)
            .color(Color::from_rgb8(0xd8, 0xe1, 0xf0)),
        stack![wheel_background, wheel],
        basic_slider("Luminance", -100.0, 100.0, value.luminance, move |amount| {
            Message::ColorGradingZoneLuminanceChanged(zone, amount)
        },),
    ]
    .spacing(10)
    .align_x(iced::alignment::Horizontal::Center);

    container(content).width(Length::Fill).into()
}

fn top_bar_icon_button<'a>(
    icon: AppIcon,
    message: Option<Message>,
    tip: &'a str,
) -> Element<'a, Message> {
    let button = button(app_icon(icon, 18.0, Color::from_rgb8(0xe2, 0xe8, 0xf0)))
        .padding([10, 12])
        .style(|theme, status| {
            let mut style = iced::widget::button::secondary(theme, status);
            style.background = Some(Background::Color(Color::from_rgb8(0x1b, 0x22, 0x2f)));
            style.text_color = Color::from_rgb8(0xe2, 0xe8, 0xf0);
            style.border.radius = 14.0.into();
            style
        })
        .on_press_maybe(message);

    tooltip(
        button,
        container(text(tip).size(12).color(Color::from_rgb8(0xe2, 0xe8, 0xf0)))
            .padding([6, 10])
            .style(|_| container::Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(Color::from_rgb8(0x0f, 0x14, 0x1d))),
                border: Border {
                    color: Color::from_rgba8(0xff, 0xff, 0xff, 0.08),
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..container::Style::default()
            }),
        tooltip::Position::Bottom,
    )
    .gap(8)
    .into()
}

fn header_status<'a>(status: &'a str) -> Element<'a, Message> {
    if status == "Preview updated." {
        row![
            app_icon(AppIcon::Check, 13.0, Color::from_rgb8(0x86, 0xef, 0xac)),
            text(status)
                .size(13)
                .color(Color::from_rgb8(0x86, 0xef, 0xac)),
        ]
        .spacing(6)
        .align_y(iced::alignment::Vertical::Center)
        .into()
    } else if status == "Preview render completed, but the pixels were unchanged." {
        text(status)
            .size(13)
            .color(Color::from_rgb8(0xf5, 0xd0, 0x7a))
            .into()
    } else if is_error_status(status) {
        text(status)
            .size(13)
            .color(Color::from_rgb8(0xff, 0xb4, 0xb4))
            .into()
    } else if is_info_status(status) {
        text(status)
            .size(13)
            .color(Color::from_rgb8(0xd5, 0xe6, 0xff))
            .into()
    } else {
        row![
            app_icon(AppIcon::Check, 13.0, Color::from_rgb8(0x86, 0xef, 0xac)),
            text(status)
                .size(13)
                .color(Color::from_rgb8(0x86, 0xef, 0xac)),
        ]
        .spacing(6)
        .align_y(iced::alignment::Vertical::Center)
        .into()
    }
}

fn is_error_status(status: &str) -> bool {
    let lower = status.to_ascii_lowercase();
    lower.contains("failed")
        || lower.contains("unavailable")
        || lower.contains("could not")
        || lower.contains("error")
}

fn is_info_status(status: &str) -> bool {
    let lower = status.to_ascii_lowercase();
    lower.starts_with("loading ") || lower.starts_with("rendering ") || lower.starts_with("loaded ")
}

fn step_card_animation(card: &mut CardAnimation) {
    let target = if card.expanded { 1.0 } else { 0.0 };
    let step = 0.10;

    if (target - card.progress).abs() <= step {
        card.progress = target;
    } else if card.expanded {
        card.progress = (card.progress + step).clamp(0.0, 1.0);
    } else {
        card.progress = (card.progress - step).clamp(0.0, 1.0);
    }
}

fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn step_export_toggle_animation(progress: &mut f32, enabled: bool) {
    let target = bool_to_progress(enabled);
    let step = 0.18;

    if (target - *progress).abs() <= step {
        *progress = target;
    } else if target > *progress {
        *progress = (*progress + step).clamp(0.0, 1.0);
    } else {
        *progress = (*progress - step).clamp(0.0, 1.0);
    }
}

fn bool_to_progress(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

fn lerp_color(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: from.r + (to.r - from.r) * t,
        g: from.g + (to.g - from.g) * t,
        b: from.b + (to.b - from.b) * t,
        a: from.a + (to.a - from.a) * t,
    }
}

#[derive(Debug, Clone)]
struct CurveEditor {
    channel: CurveChannel,
    points: Vec<CurvePoint>,
    histogram: [u32; 256],
    color: Color,
}

impl canvas::Program<Message> for CurveEditor {
    type State = CurveEditorState;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        let Some(position) = cursor.position_in(bounds) else {
            if matches!(
                event,
                canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            ) {
                state.dragging_index = None;
            }
            return (canvas::event::Status::Ignored, None);
        };

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let point = point_from_position(position, bounds);
                let hit = self
                    .points
                    .iter()
                    .enumerate()
                    .find(|(_, existing)| distance(existing, &point) < 10.0);

                if let Some((index, _)) = hit {
                    state.dragging_index = Some(index);
                    return (canvas::event::Status::Captured, None);
                }

                let mut points = self.points.clone();
                points.push(point);
                let points = sanitize_curve_points(points);
                let index = nearest_curve_index(&points, point.x);
                state.dragging_index = Some(index);
                (
                    canvas::event::Status::Captured,
                    Some(Message::CurveChanged(self.channel, points)),
                )
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(index) = state.dragging_index {
                    let mut points = self.points.clone();
                    let mut point = point_from_position(position, bounds);
                    let min_x = if index == 0 {
                        0.0
                    } else {
                        points[index - 1].x + 0.5
                    };
                    let max_x = if index + 1 >= points.len() {
                        255.0
                    } else {
                        points[index + 1].x - 0.5
                    };
                    point.x = point.x.clamp(min_x, max_x);
                    if index == 0 {
                        point.x = 0.0;
                    }
                    if index + 1 == points.len() {
                        point.x = 255.0;
                    }
                    points[index] = point;
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::CurveChanged(
                            self.channel,
                            sanitize_curve_points(points),
                        )),
                    );
                }
                (canvas::event::Status::Ignored, None)
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging_index = None;
                (
                    canvas::event::Status::Captured,
                    Some(Message::CommitPreviewRender),
                )
            }
            _ => (canvas::event::Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let bg = canvas::Path::rectangle(Point::ORIGIN, bounds.size());
        frame.fill(&bg, Color::from_rgb8(0x10, 0x14, 0x1d));

        for step in [0.25_f32, 0.5, 0.75] {
            let x = bounds.width * step;
            let y = bounds.height * step;
            frame.stroke(
                &canvas::Path::line(Point::new(x, 0.0), Point::new(x, bounds.height)),
                canvas::Stroke::default()
                    .with_color(Color::from_rgba8(0xff, 0xff, 0xff, 0.06))
                    .with_width(1.0),
            );
            frame.stroke(
                &canvas::Path::line(Point::new(0.0, y), Point::new(bounds.width, y)),
                canvas::Stroke::default()
                    .with_color(Color::from_rgba8(0xff, 0xff, 0xff, 0.06))
                    .with_width(1.0),
            );
        }

        let max_bin = self.histogram.iter().copied().max().unwrap_or(1) as f32;
        let histogram_path = canvas::Path::new(|builder| {
            builder.move_to(Point::new(0.0, bounds.height));
            for (index, value) in self.histogram.iter().enumerate() {
                let x = index as f32 / 255.0 * bounds.width;
                let y = bounds.height - (*value as f32 / max_bin) * bounds.height;
                builder.line_to(Point::new(x, y));
            }
            builder.line_to(Point::new(bounds.width, bounds.height));
            builder.close();
        });
        frame.fill(
            &histogram_path,
            Color {
                a: 0.18,
                ..self.color
            },
        );

        let curve_path = canvas::Path::new(|builder| {
            for step in 0..=255 {
                let x = step as f32;
                let y = evaluate_curve(&self.points, x);
                let px = x / 255.0 * bounds.width;
                let py = bounds.height - (y / 255.0 * bounds.height);
                if step == 0 {
                    builder.move_to(Point::new(px, py));
                } else {
                    builder.line_to(Point::new(px, py));
                }
            }
        });
        frame.stroke(
            &curve_path,
            canvas::Stroke::default()
                .with_color(self.color)
                .with_width(2.5),
        );

        let hovered = cursor
            .position_in(bounds)
            .map(|position| point_from_position(position, bounds));
        for (index, point) in self.points.iter().enumerate() {
            let center = Point::new(
                point.x / 255.0 * bounds.width,
                bounds.height - (point.y / 255.0 * bounds.height),
            );
            let active = state.dragging_index == Some(index)
                || hovered
                    .as_ref()
                    .map(|hover| distance(point, hover) < 10.0)
                    .unwrap_or(false);
            let radius = if active { 5.5 } else { 4.0 };
            let circle = canvas::Path::circle(center, radius);
            frame.fill(&circle, self.color);
            frame.stroke(
                &circle,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb8(0x08, 0x12, 0x20))
                    .with_width(1.0),
            );
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging_index.is_some() || cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ColorWheelEditor {
    zone: ColorGradingZone,
    value: HueSatLum,
}

impl canvas::Program<Message> for ColorWheelEditor {
    type State = ColorWheelState;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        let position = cursor.position_in(bounds);

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = position
                    && let Some(updated) =
                        color_grading_value_from_position(position, bounds, self.value)
                {
                    state.dragging = true;
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::ColorGradingWheelChanged(self.zone, updated)),
                    );
                }
                (canvas::event::Status::Ignored, None)
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging
                    && let Some(position) = position
                    && let Some(updated) =
                        color_grading_value_from_position(position, bounds, self.value)
                {
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::ColorGradingWheelChanged(self.zone, updated)),
                    );
                }
                (canvas::event::Status::Ignored, None)
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging {
                    state.dragging = false;
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::CommitPreviewRender),
                    );
                }
                (canvas::event::Status::Ignored, None)
            }
            _ => (canvas::event::Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let radius = (bounds.width.min(bounds.height) * 0.5) - 6.0;
        let center = Point::new(bounds.width * 0.5, bounds.height * 0.5);

        frame.stroke(
            &canvas::Path::circle(center, radius),
            canvas::Stroke::default()
                .with_width(1.0)
                .with_color(Color::from_rgba8(0xff, 0xff, 0xff, 0.08)),
        );

        let marker = color_grading_marker_point(self.value, bounds);
        let shadow = canvas::Path::circle(marker, 11.0);
        frame.fill(&shadow, Color::from_rgba8(0x00, 0x00, 0x00, 0.18));
        let ring = canvas::Path::circle(marker, 10.0);
        frame.fill(&ring, Color::from_rgba8(0xff, 0xff, 0xff, 0.88));
        let inner = canvas::Path::circle(marker, 6.0);
        frame.fill(
            &inner,
            hsv_to_color(
                self.value.hue,
                (self.value.saturation / 100.0).clamp(0.0, 1.0),
                1.0,
            ),
        );

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging
            || cursor
                .position_in(bounds)
                .and_then(|position| {
                    color_grading_value_from_position(position, bounds, self.value)
                })
                .is_some()
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

fn color_grading_value_from_position(
    position: Point,
    bounds: Rectangle,
    current: HueSatLum,
) -> Option<HueSatLum> {
    let center = Point::new(bounds.width * 0.5, bounds.height * 0.5);
    let radius = (bounds.width.min(bounds.height) * 0.5) - 6.0;
    let dx = position.x - center.x;
    let dy = position.y - center.y;
    let distance = (dx * dx + dy * dy).sqrt();
    if distance > radius {
        return None;
    }

    Some(HueSatLum {
        hue: dy.atan2(dx).to_degrees().rem_euclid(360.0),
        saturation: (distance / radius * 100.0).clamp(0.0, 100.0),
        luminance: current.luminance,
    })
}

fn color_grading_marker_point(value: HueSatLum, bounds: Rectangle) -> Point {
    let center = Point::new(bounds.width * 0.5, bounds.height * 0.5);
    let radius = (bounds.width.min(bounds.height) * 0.5) - 6.0;
    let angle = value.hue.to_radians();
    let distance = (value.saturation / 100.0).clamp(0.0, 1.0) * radius;
    Point::new(
        center.x + angle.cos() * distance,
        center.y + angle.sin() * distance,
    )
}

fn hsv_to_color(hue: f32, saturation: f32, value: f32) -> Color {
    let h = hue.rem_euclid(360.0) / 60.0;
    let c = value * saturation;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match h {
        h if h < 1.0 => (c, x, 0.0),
        h if h < 2.0 => (x, c, 0.0),
        h if h < 3.0 => (0.0, c, x),
        h if h < 4.0 => (0.0, x, c),
        h if h < 5.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = value - c;
    Color::from_rgb(r1 + m, g1 + m, b1 + m)
}

fn color_wheel_handle(size: u32) -> image::Handle {
    let size = size.max(2);
    let radius = (size as f32 * 0.5) - 2.0;
    let center = size as f32 * 0.5;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let distance = (dx * dx + dy * dy).sqrt();

            if distance > radius {
                pixels.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }

            let hue = dy.atan2(dx).to_degrees().rem_euclid(360.0);
            let saturation = (distance / radius).clamp(0.0, 1.0);
            let feather = ((radius - distance) / 1.5).clamp(0.0, 1.0);
            let color = hsv_to_color(hue, saturation, 1.0);

            pixels.push((color.r * 255.0) as u8);
            pixels.push((color.g * 255.0) as u8);
            pixels.push((color.b * 255.0) as u8);
            pixels.push((feather * 255.0) as u8);
        }
    }

    image::Handle::from_rgba(size, size, pixels)
}

fn point_from_position(position: Point, bounds: Rectangle) -> CurvePoint {
    CurvePoint {
        x: ((position.x / bounds.width) * 255.0).clamp(0.0, 255.0),
        y: (255.0 - (position.y / bounds.height) * 255.0).clamp(0.0, 255.0),
    }
}

fn distance(a: &CurvePoint, b: &CurvePoint) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

fn nearest_curve_index(points: &[CurvePoint], target_x: f32) -> usize {
    points
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (a.x - target_x)
                .abs()
                .partial_cmp(&(b.x - target_x).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn evaluate_curve(points: &[CurvePoint], x: f32) -> f32 {
    if points.len() < 2 {
        return x;
    }
    let x = x.clamp(0.0, 255.0);
    if x <= points[0].x {
        return points[0].y;
    }
    if x >= points[points.len() - 1].x {
        return points[points.len() - 1].y;
    }

    for index in 0..points.len() - 1 {
        let p1 = points[index];
        let p2 = points[index + 1];
        if x <= p2.x {
            let p0 = if index == 0 { p1 } else { points[index - 1] };
            let p3 = if index + 2 >= points.len() {
                p2
            } else {
                points[index + 2]
            };
            let delta_before = (p1.y - p0.y) / (p1.x - p0.x).abs().max(0.001);
            let delta_current = (p2.y - p1.y) / (p2.x - p1.x).abs().max(0.001);
            let delta_after = (p3.y - p2.y) / (p3.x - p2.x).abs().max(0.001);

            let mut tangent1 = if index == 0 || delta_before * delta_current <= 0.0 {
                if index == 0 { delta_current } else { 0.0 }
            } else {
                (delta_before + delta_current) / 2.0
            };
            let mut tangent2 =
                if index + 1 == points.len() - 1 || delta_current * delta_after <= 0.0 {
                    if index + 1 == points.len() - 1 {
                        delta_current
                    } else {
                        0.0
                    }
                } else {
                    (delta_current + delta_after) / 2.0
                };

            if delta_current != 0.0 {
                let alpha = tangent1 / delta_current;
                let beta = tangent2 / delta_current;
                let tau = alpha * alpha + beta * beta;
                if tau > 9.0 {
                    let scale = 3.0 / tau.sqrt();
                    tangent1 *= scale;
                    tangent2 *= scale;
                }
            }

            let t = (x - p1.x) / (p2.x - p1.x).max(0.001);
            let t2 = t * t;
            let t3 = t2 * t;
            let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
            let h10 = t3 - 2.0 * t2 + t;
            let h01 = -2.0 * t3 + 3.0 * t2;
            let h11 = t3 - t2;
            let dx = p2.x - p1.x;
            return (h00 * p1.y + h10 * tangent1 * dx + h01 * p2.y + h11 * tangent2 * dx)
                .clamp(0.0, 255.0);
        }
    }

    points[points.len() - 1].y
}
