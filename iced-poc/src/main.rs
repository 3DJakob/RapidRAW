mod rapidraw_shader;

use iced::widget::{
    Space, button, canvas, column, container, image, mouse_area, row, scrollable, slider, text, tooltip,
};
use iced::{Background, Border, Color, Element, Length, Point, Rectangle, Size, Subscription, Task, Theme, application, mouse, window};
use ::image::{DynamicImage, GenericImageView, RgbImage, imageops::FilterType, open as open_image};
use rawler::{
    decoders::{Orientation, RawDecodeParams},
    imgop::develop::{DemosaicAlgorithm, Intermediate, ProcessingStep, RawDevelop},
    rawsource::RawSource,
};
use rapidraw_shader::{
    BasicAdjustments, CurvePoint, CurvesSettings, RapidRawRenderer, ToneMapper, default_curve_points,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::time::Instant;
use std::path::{Path, PathBuf};
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

fn main() -> iced::Result {
    application("RapidRAW Iced POC Preview", App::update, App::view)
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
    ModifiersChanged(iced::keyboard::Modifiers),
    AnimationFrame(Instant),
    ToggleBasicCard,
    ToggleCurvesCard,
    ExposureChanged(f32),
    BrightnessChanged(f32),
    ContrastChanged(f32),
    HighlightsChanged(f32),
    ShadowsChanged(f32),
    WhitesChanged(f32),
    BlacksChanged(f32),
    ToneMapperChanged(ToneMapper),
    ActiveCurveChannelChanged(CurveChannel),
    CurveChanged(CurveChannel, Vec<CurvePoint>),
    ResetCurveChannel(CurveChannel),
    ResetBasicAdjustments,
    CommitPreviewRender,
    PreviewRendered {
        generation: u64,
        result: Result<RenderedPreview, String>,
    },
}

struct App {
    route: Route,
    samples: Vec<SampleImage>,
    selected_index: usize,
    selected_indices: BTreeSet<usize>,
    shift_pressed: bool,
    basic_card: CardAnimation,
    curves_card: CardAnimation,
    active_curve_channel: CurveChannel,
    current_folder: Option<PathBuf>,
    is_loading: bool,
    status_message: Option<String>,
    basic_adjustments: BasicAdjustments,
    rendered_preview: Option<image::Handle>,
    preview_generation: u64,
    is_rendering_preview: bool,
    pending_preview_quality: Option<PreviewQuality>,
    renderer: Option<RapidRawRenderer>,
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
    interactive_preview_image: Arc<DynamicImage>,
    full_preview_image: Arc<DynamicImage>,
    preview: image::Handle,
    thumbnail: image::Handle,
    is_raw: bool,
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
    changed: bool,
}

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

#[derive(Debug, Clone, Copy)]
struct CardAnimation {
    expanded: bool,
    progress: f32,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let samples = load_samples();
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

        (
            Self {
                route: Route::Home,
                samples,
                selected_index: 0,
                selected_indices: initial_selected_indices,
                shift_pressed: false,
                basic_card: CardAnimation {
                    expanded: true,
                    progress: 1.0,
                },
                curves_card: CardAnimation {
                    expanded: false,
                    progress: 0.0,
                },
                active_curve_channel: CurveChannel::Luma,
                current_folder: None,
                is_loading: false,
                status_message,
                basic_adjustments: BasicAdjustments::default(),
                rendered_preview: None,
                preview_generation: 0,
                is_rendering_preview: false,
                pending_preview_quality: None,
                renderer,
            },
            Task::done(Message::ResetBasicAdjustments),
        )
    }

    fn theme(&self) -> Theme {
        Theme::TokyoNightStorm
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        let modifiers = iced::event::listen_with(|event, _status, _window| match event {
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
                Some(Message::ModifiersChanged(modifiers))
            }
            _ => None,
        });

        if self.is_animating_cards() {
            Subscription::batch(vec![modifiers, window::frames().map(Message::AnimationFrame)])
        } else {
            modifiers
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::EnterEditor => {
                if !self.samples.is_empty() {
                    self.route = Route::Editor;
                }
            }
            Message::BackToHome => {
                self.route = Route::Home;
            }
            Message::OpenFolder => {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.is_loading = true;
                    self.status_message = Some(format!("Loading images from {}...", path.display()));
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
                    self.pending_preview_quality = None;
                    self.basic_adjustments = self
                        .samples
                        .first()
                        .map(|sample| sample.adjustments.clone())
                        .unwrap_or_default();
                        self.route = if self.samples.is_empty() {
                            Route::Home
                        } else {
                            Route::Editor
                        };
                        self.status_message = if self.samples.is_empty() {
                            Some("The selected folder did not contain any supported image files.".to_string())
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
                    }
                    Err(error) => {
                        self.status_message = Some(error);
                    }
                }
            }
            Message::ModifiersChanged(modifiers) => {
                self.shift_pressed = modifiers.shift();
            }
            Message::AnimationFrame(_instant) => {
                step_card_animation(&mut self.basic_card);
                step_card_animation(&mut self.curves_card);
            }
            Message::ToggleBasicCard => {
                self.basic_card.expanded = !self.basic_card.expanded;
            }
            Message::ToggleCurvesCard => {
                self.curves_card.expanded = !self.curves_card.expanded;
            }
            Message::SelectImage(index) => {
                if index < self.samples.len() {
                    if self.shift_pressed {
                        self.selected_indices.insert(index);
                    } else {
                        self.selected_indices.clear();
                        self.selected_indices.insert(index);
                    }
                    self.selected_index = index;
                    self.rendered_preview = None;
                    self.pending_preview_quality = None;
                    self.basic_adjustments = self.samples[index].adjustments.clone();
                    return self.request_preview_render(PreviewQuality::Full);
                }
            }
            Message::ExposureChanged(value) => {
                self.basic_adjustments.exposure = value;
                self.update_selected_adjustments(|adjustments| adjustments.exposure = value);
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::BrightnessChanged(value) => {
                self.basic_adjustments.brightness = value;
                self.update_selected_adjustments(|adjustments| adjustments.brightness = value);
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ContrastChanged(value) => {
                self.basic_adjustments.contrast = value;
                self.update_selected_adjustments(|adjustments| adjustments.contrast = value);
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::HighlightsChanged(value) => {
                self.basic_adjustments.highlights = value;
                self.update_selected_adjustments(|adjustments| adjustments.highlights = value);
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ShadowsChanged(value) => {
                self.basic_adjustments.shadows = value;
                self.update_selected_adjustments(|adjustments| adjustments.shadows = value);
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::WhitesChanged(value) => {
                self.basic_adjustments.whites = value;
                self.update_selected_adjustments(|adjustments| adjustments.whites = value);
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::BlacksChanged(value) => {
                self.basic_adjustments.blacks = value;
                self.update_selected_adjustments(|adjustments| adjustments.blacks = value);
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ToneMapperChanged(value) => {
                self.basic_adjustments.tone_mapper = value;
                self.update_selected_adjustments(|adjustments| adjustments.tone_mapper = value);
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::ActiveCurveChannelChanged(channel) => {
                self.active_curve_channel = channel;
            }
            Message::CurveChanged(channel, points) => {
                let points = sanitize_curve_points(points);
                self.update_selected_adjustments(|adjustments| {
                    curve_points_mut(&mut adjustments.curves, channel).clone_from(&points);
                });
                return self.request_preview_render(PreviewQuality::Interactive);
            }
            Message::ResetCurveChannel(channel) => {
                self.update_selected_adjustments(|adjustments| {
                    *curve_points_mut(&mut adjustments.curves, channel) = default_curve_points();
                });
                return self.request_preview_render(PreviewQuality::Full);
            }
            Message::ResetBasicAdjustments => {
                self.basic_adjustments = BasicAdjustments::default();
                self.update_selected_adjustments(|adjustments| *adjustments = BasicAdjustments::default());
                if !self.samples.is_empty() {
                    return self.request_preview_render(PreviewQuality::Full);
                }
            }
            Message::CommitPreviewRender => {
                if !self.samples.is_empty() {
                    return self.request_preview_render(PreviewQuality::Full);
                }
            }
            Message::PreviewRendered { generation, result } => {
                if generation == self.preview_generation {
                    match result {
                        Ok(rendered) => {
                            self.rendered_preview = Some(rendered.handle);
                            self.status_message = Some(if rendered.changed {
                                "Preview updated.".to_string()
                            } else {
                                "Preview render completed, but the pixels were unchanged.".to_string()
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
        }

        Task::none()
    }

    fn is_animating_cards(&self) -> bool {
        (self.basic_card.progress - if self.basic_card.expanded { 1.0 } else { 0.0 }).abs() > 0.01
            || (self.curves_card.progress - if self.curves_card.expanded { 1.0 } else { 0.0 }).abs() > 0.01
    }

    fn update_selected_adjustments(
        &mut self,
        update: impl Fn(&mut BasicAdjustments),
    ) {
        let selected: Vec<usize> = if self.selected_indices.is_empty() {
            vec![self.selected_index]
        } else {
            self.selected_indices.iter().copied().collect()
        };

        for index in selected {
            if let Some(sample) = self.samples.get_mut(index) {
                update(&mut sample.adjustments);
                if let Err(error) = save_sample_adjustments(sample) {
                    self.status_message = Some(error);
                }
            }
        }

        if let Some(sample) = self.samples.get(self.selected_index) {
            self.basic_adjustments = sample.adjustments.clone();
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

        let top_bar = row![
            top_bar_icon_button("←", Some(Message::BackToHome), "Back"),
            top_bar_icon_button(
                "⌂",
                (!self.is_loading).then_some(Message::OpenFolder),
                "Open folder",
            ),
            Space::with_width(Length::Fixed(12.0)),
            column![
                text(&selected.name).size(28),
                text(selected.path.display().to_string())
                    .size(14)
                    .color(Color::from_rgb8(0xa8, 0xb2, 0xc8)),
                text(format!(
                    "{} selected",
                    self.selected_indices.len().max(1)
                ))
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
            ],
        ]
        .align_y(iced::alignment::Vertical::Center)
        .spacing(10);

        let preview = container(
            image(self.rendered_preview.clone().unwrap_or_else(|| selected.preview.clone()))
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(iced::ContentFit::Contain),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(panel_style);

        let filmstrip_items = self
            .samples
            .iter()
            .enumerate()
            .fold(row![].spacing(16).width(Length::Fill), |row, (index, sample)| {
                row.push(self.filmstrip_item(index, sample))
            });

        let filmstrip = container(
            scrollable(
                container(filmstrip_items)
                    .padding([10, 4])
                    .width(Length::Fill),
            )
            .width(Length::Fill)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            )),
        )
        .width(Length::Fill)
        .height(Length::Fixed(178.0))
        .padding([10, 12])
        .style(panel_style);

        let editor_body = row![
            preview,
            container(self.view_basic_panel())
                .width(Length::Fixed(330.0))
                .height(Length::Fill)
        ]
        .spacing(18)
        .height(Length::Fill);

        let layout = column![top_bar, editor_body, filmstrip]
            .spacing(18)
            .height(Length::Fill);

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([28, 28])
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
                text(sample.path.file_name().and_then(|name| name.to_str()).unwrap_or_default())
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
        let background = if is_active {
            Color::from_rgb8(0x27, 0x35, 0x49)
        } else if is_selected {
            Color::from_rgb8(0x1d, 0x29, 0x38)
        } else {
            Color::from_rgb8(0x16, 0x1b, 0x25)
        };

        let card = container(
            column![
                image(sample.thumbnail.clone())
                    .width(Length::Fixed(148.0))
                    .height(Length::Fixed(92.0))
                    .content_fit(iced::ContentFit::Cover),
                Space::with_height(Length::Fixed(8.0)),
                text(&sample.name).size(15),
            ]
            .spacing(0),
        )
        .padding(10)
        .width(Length::Fixed(170.0))
        .style(move |_| container::Style {
            text_color: Some(Color::WHITE),
            background: Some(Background::Color(background)),
            border: iced::Border::default().rounded(14.0),
            ..container::Style::default()
        });

        mouse_area(card)
            .on_press(Message::SelectImage(index))
            .interaction(iced::mouse::Interaction::Pointer)
            .into()
    }

    fn view_basic_panel(&self) -> Element<'_, Message> {
        let basic_body = column![
            row![
                text("RapidRAW-inspired exposure and tone controls")
                    .size(14)
                    .color(Color::from_rgb8(0xa8, 0xb2, 0xc8)),
                Space::with_width(Length::Fill),
                icon_button("↺", Message::ResetBasicAdjustments, "Reset basic adjustments"),
            ]
            .align_y(iced::alignment::Vertical::Center),
            text(if self.selected_indices.len() > 1 {
                "Changes apply to all selected images."
            } else {
                "Changes apply to the active image."
            })
            .size(13)
            .color(Color::from_rgb8(0x8d, 0x98, 0xae)),
            column![
                text("Tone Mapper").size(14).color(Color::from_rgb8(0xa8, 0xb2, 0xc8)),
                row![
                    tone_mapper_button("Basic", ToneMapper::Basic, self.basic_adjustments.tone_mapper),
                    tone_mapper_button("AgX", ToneMapper::AgX, self.basic_adjustments.tone_mapper),
                ]
                .spacing(8),
            ]
            .spacing(8),
            basic_slider("Exposure", -5.0, 5.0, self.basic_adjustments.exposure, Message::ExposureChanged),
            basic_slider("Brightness", -5.0, 5.0, self.basic_adjustments.brightness, Message::BrightnessChanged),
            basic_slider("Contrast", -100.0, 100.0, self.basic_adjustments.contrast, Message::ContrastChanged),
            basic_slider("Highlights", -100.0, 100.0, self.basic_adjustments.highlights, Message::HighlightsChanged),
            basic_slider("Shadows", -100.0, 100.0, self.basic_adjustments.shadows, Message::ShadowsChanged),
            basic_slider("Whites", -100.0, 100.0, self.basic_adjustments.whites, Message::WhitesChanged),
            basic_slider("Blacks", -100.0, 100.0, self.basic_adjustments.blacks, Message::BlacksChanged),
        ]
        .spacing(14);

        let selected = self.samples.get(self.selected_index);
        let curves_body: Element<'_, Message> = if let Some(sample) = selected {
            let active_curve = curve_points(&self.basic_adjustments.curves, self.active_curve_channel);
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
                    text("Tone curves with histogram overlay")
                        .size(14)
                        .color(Color::from_rgb8(0xa8, 0xb2, 0xc8)),
                    Space::with_width(Length::Fill),
                    icon_button("↺", Message::ResetCurveChannel(self.active_curve_channel), "Reset curve channel"),
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

        let controls = column![
            adjustment_card("Basic", self.basic_card, Message::ToggleBasicCard, basic_body.into(), 430.0),
            adjustment_card("Curves", self.curves_card, Message::ToggleCurvesCard, curves_body, 320.0),
            text("Preview updates live for the selected image.")
                .size(13)
                .color(Color::from_rgb8(0x8d, 0x98, 0xae)),
        ]
        .spacing(16);

        container(scrollable(controls))
            .padding(20)
            .height(Length::Fill)
            .style(panel_style)
            .into()
    }

    fn request_preview_render(&mut self, quality: PreviewQuality) -> Task<Message> {
        if self.is_rendering_preview {
            self.pending_preview_quality = Some(
                self.pending_preview_quality
                    .map_or(quality, |pending| pending.max(quality)),
            );
            return Task::none();
        }

        self.start_preview_render(quality)
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
        let adjustments = sample.adjustments.clone();
        let is_raw = sample.is_raw;
        self.is_rendering_preview = true;
        self.status_message = None;

        Task::perform(
            async move {
                let base_rgba = base_image.to_rgba8().into_raw();
                let result = panic::catch_unwind(AssertUnwindSafe(|| {
                    renderer
                        .render(base_image.as_ref(), &adjustments, is_raw)
                        .map(|image| {
                            let rendered_rgba = image.to_rgba8().into_raw();
                            let changed = rendered_rgba != base_rgba;
                            RenderedPreview {
                                handle: image::Handle::from_rgba(
                                    image.width(),
                                    image.height(),
                                    rendered_rgba,
                                ),
                                changed,
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

fn build_sample_image(path: PathBuf) -> Result<SampleImage, String> {
    let is_raw = is_supported_raw(&path);
    let (interactive_preview_image, full_preview_image, preview, thumbnail) =
        load_preview_handles(&path)?;
    let histogram = build_histogram(full_preview_image.as_ref());
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Untitled")
        .replace(['-', '_'], " ");
    let adjustments = load_sample_adjustments(&path).unwrap_or_default();

    Ok(SampleImage {
        name: title_case(&name),
        path,
        interactive_preview_image,
        full_preview_image,
        preview,
        thumbnail,
        is_raw,
        adjustments,
        histogram,
    })
}

fn load_preview_handles(
    path: &Path,
) -> Result<(Arc<DynamicImage>, Arc<DynamicImage>, image::Handle, image::Handle), String> {
    let image = if is_supported_raw(path) {
        decode_raw_preview(path)?
    } else {
        open_image(path).map_err(|error| error.to_string())?
    };

    let full_preview_image = resize_for_bound(&image, 1800);
    let interactive_preview_image = resize_for_bound(&full_preview_image, 1100);
    let preview = make_rgba_handle(&full_preview_image);
    let thumbnail = make_rgba_handle(&resize_for_bound(&full_preview_image, 320));

    Ok((
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
        exposure: value.get("exposure").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        brightness: value.get("brightness").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        contrast: value.get("contrast").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        highlights: value.get("highlights").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        shadows: value.get("shadows").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        whites: value.get("whites").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        blacks: value.get("blacks").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        tone_mapper,
        curves: curves_from_value(value.get("curves").unwrap_or(&Value::Null)),
    }
}

fn adjustments_to_value(adjustments: &BasicAdjustments) -> Value {
    json!({
        "exposure": adjustments.exposure,
        "brightness": adjustments.brightness,
        "contrast": adjustments.contrast,
        "highlights": adjustments.highlights,
        "shadows": adjustments.shadows,
        "whites": adjustments.whites,
        "blacks": adjustments.blacks,
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
        object.insert("curves".to_string(), curves_to_value(&adjustments.curves));
    }
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

fn panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(Color::WHITE),
        background: Some(Background::Color(Color::from_rgb8(0x14, 0x19, 0x23))),
        border: iced::Border::default().rounded(20.0),
        ..container::Style::default()
    }
}

fn basic_slider<'a>(
    label: &'a str,
    min: f32,
    max: f32,
    value: f32,
    on_change: fn(f32) -> Message,
) -> Element<'a, Message> {
    column![
        row![
            text(label).size(15),
            Space::with_width(Length::Fill),
            text(format!("{value:.2}")).size(14).color(Color::from_rgb8(0xa8, 0xb2, 0xc8)),
        ]
        .align_y(iced::alignment::Vertical::Center),
        slider(min..=max, value, on_change)
            .step(0.01)
            .on_release(Message::CommitPreviewRender),
    ]
    .spacing(6)
    .into()
}

fn tone_mapper_button<'a>(label: &'a str, value: ToneMapper, selected: ToneMapper) -> Element<'a, Message> {
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
            style.text_color = if active { Color::from_rgb8(0x08, 0x12, 0x20) } else { Color::WHITE };
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
    let body_content: Element<'a, Message> = if card.progress > 0.01 {
        container(body)
            .height(Length::Fixed((expanded_height * card.progress).max(1.0)))
            .into()
    } else {
        Space::with_height(Length::Shrink).into()
    };

    let header = button(
        row![
            text(title).size(20),
            Space::with_width(Length::Fill),
            text(if expanded { "▾" } else { "▸" })
                .size(18)
                .color(Color::from_rgb8(0xa8, 0xb2, 0xc8)),
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .padding([4, 0])
    .style(iced::widget::button::text)
    .on_press(toggle_message);

    container(
        column![
            header,
            body_content,
        ]
        .spacing(12),
    )
    .padding(16)
    .style(|_| container::Style {
        text_color: Some(Color::WHITE),
        background: Some(Background::Color(Color::from_rgb8(0x17, 0x1c, 0x27))),
        border: Border::default().rounded(18.0),
        ..container::Style::default()
    })
    .into()
}

fn icon_button<'a>(icon: &'a str, message: Message, _title: &'a str) -> Element<'a, Message> {
    button(text(icon).size(16))
        .padding([7, 10])
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

fn top_bar_icon_button<'a>(
    icon: &'a str,
    message: Option<Message>,
    tip: &'a str,
) -> Element<'a, Message> {
    let button = button(text(icon).size(18))
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
        container(
            text(tip)
                .size(12)
                .color(Color::from_rgb8(0xe2, 0xe8, 0xf0)),
        )
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
            text("✓")
                .size(13)
                .color(Color::from_rgb8(0x86, 0xef, 0xac)),
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
    } else {
        text(status)
            .size(13)
            .color(Color::from_rgb8(0xff, 0xb4, 0xb4))
            .into()
    }
}

fn step_card_animation(card: &mut CardAnimation) {
    let target = if card.expanded { 1.0 } else { 0.0 };
    let delta = target - card.progress;
    if delta.abs() < 0.01 {
        card.progress = target;
    } else {
        card.progress = (card.progress + delta * 0.22).clamp(0.0, 1.0);
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
            if matches!(event, canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))) {
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
                    let min_x = if index == 0 { 0.0 } else { points[index - 1].x + 0.5 };
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
                (canvas::event::Status::Captured, Some(Message::CommitPreviewRender))
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
            Color { a: 0.18, ..self.color },
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

        let hovered = cursor.position_in(bounds).map(|position| point_from_position(position, bounds));
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
            let p3 = if index + 2 >= points.len() { p2 } else { points[index + 2] };
            let delta_before = (p1.y - p0.y) / (p1.x - p0.x).abs().max(0.001);
            let delta_current = (p2.y - p1.y) / (p2.x - p1.x).abs().max(0.001);
            let delta_after = (p3.y - p2.y) / (p3.x - p2.x).abs().max(0.001);

            let mut tangent1 = if index == 0 || delta_before * delta_current <= 0.0 {
                if index == 0 { delta_current } else { 0.0 }
            } else {
                (delta_before + delta_current) / 2.0
            };
            let mut tangent2 = if index + 1 == points.len() - 1 || delta_current * delta_after <= 0.0 {
                if index + 1 == points.len() - 1 { delta_current } else { 0.0 }
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
