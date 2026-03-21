use std::{borrow::Cow, time::Instant};

use base64::Engine;
use freya::prelude::*;
use rig::{
    client::CompletionClient,
    completion::Prompt,
    http_client::ReqwestClient,
    message::{DocumentSourceKind, Image, ImageDetail, ImageMediaType},
    providers::openai,
};
use serde::{Deserialize, Serialize};
use xcap::Monitor;

/// Color palette for the application UI, built on Tailwind CSS v4 colors.
mod colors {
    use freya::prelude::Color;
    use tw_colors::RGB;

    // ── Backgrounds ──────────────────────────────────────────────
    pub const BG_ROOT: RGB = tw_colors::ZINC_950;
    pub const BG_HEADER: RGB = tw_colors::ZINC_900;
    pub const BG_FOOTER: RGB = tw_colors::ZINC_900;
    pub const BG_PANEL: RGB = tw_colors::ZINC_800;
    pub const BG_OVERLAY: RGB = tw_colors::ZINC_900;

    // ── Borders ──────────────────────────────────────────────────
    pub const BORDER_HEADER: RGB = tw_colors::ZINC_800;
    pub const BORDER_FOOTER: RGB = tw_colors::ZINC_800;
    pub const BORDER_PANEL: RGB = tw_colors::ZINC_700;

    // ── Shadows ──────────────────────────────────────────────────
    pub const SHADOW_PANEL: Color = Color::from_af32rgb(0.3, 0, 0, 0);

    // ── Accent ───────────────────────────────────────────────────
    pub const ACCENT: RGB = tw_colors::VIOLET_500;
    pub const SUCCESS: RGB = tw_colors::EMERALD_400;

    // ── Text ─────────────────────────────────────────────────────
    pub const TEXT_PRIMARY: RGB = tw_colors::ZINC_50;
    pub const TEXT_BODY: RGB = tw_colors::ZINC_200;
    pub const TEXT_SECTION: RGB = tw_colors::ZINC_400;
    pub const TEXT_SUBTITLE: RGB = tw_colors::ZINC_400;
    pub const TEXT_HINT: RGB = tw_colors::ZINC_500;
    pub const TEXT_MUTED: RGB = tw_colors::ZINC_600;
    pub const TEXT_SEPARATOR: RGB = tw_colors::ZINC_600;

    // ── Selection overlay ────────────────────────────────────────
    pub const SELECTION: Color = Color::from_af32rgb(0.5, 255, 0, 0);
}

pub struct TextDisplayWindow {
    pub model: Cow<'static, str>,
    pub url: Cow<'static, str>,
    pub display_screenshot: bool,
    pub monitor: Monitor,
}

pub trait ExpandedXY {
    #[allow(dead_code)]
    /// Expand the `width` using [Size::fill()].
    fn expanded_x(self) -> Self;

    /// Expand the `height` using [Size::fill()].
    fn expanded_y(self) -> Self;
}

impl ExpandedXY for Rect {
    fn expanded_x(mut self) -> Self {
        self.get_layout().layout.width = Size::fill();
        self
    }

    fn expanded_y(mut self) -> Self {
        self.get_layout().layout.height = Size::fill();
        self
    }
}

impl ExpandedXY for Label {
    fn expanded_x(mut self) -> Self {
        self.get_layout().layout.width = Size::fill();
        self
    }

    fn expanded_y(mut self) -> Self {
        self.get_layout().layout.height = Size::fill();
        self
    }
}

fn sub_app(img_bytes: State<(Instant, Bytes)>, text: State<String>) -> impl IntoElement {
    use_init_theme(|| DARK_THEME);
    let (id, bytes) = img_bytes.read().clone();

    let mut copied = use_state(|| false);

    // Root container: dark background, vertical layout
    rect()
        .expanded()
        .background(colors::BG_ROOT)
        .content(Content::Flex)
        .vertical()
        // ── Header bar ──────────────────────────────────────────────
        .child(
            rect()
                .width(Size::Fill)
                .padding(Gaps::new(16.0, 24.0, 16.0, 24.0))
                .content(Content::Flex)
                .horizontal()
                .cross_align(Alignment::Center)
                .main_align(Alignment::Start)
                .spacing(12.0)
                .background(colors::BG_HEADER)
                .border(Border::new().width(1.0).fill(colors::BORDER_HEADER))
                // App icon
                .child(
                    svg(freya::icons::lucide::scan_text())
                        .color(colors::ACCENT)
                        .width(Size::px(22.0))
                        .height(Size::px(22.0)),
                )
                // App title
                .child(
                    label()
                        .text("OpenOCR")
                        .font_size(18.0)
                        .font_weight(FontWeight::BOLD)
                        .color(colors::TEXT_PRIMARY),
                )
                // Separator dot
                .child(
                    label()
                        .text("\u{2022}")
                        .font_size(10.0)
                        .color(colors::TEXT_SEPARATOR),
                )
                // Subtitle
                .child(
                    label()
                        .text("Results")
                        .font_size(14.0)
                        .color(colors::TEXT_SUBTITLE),
                ),
        )
        // ── Main content area (two panels) ──────────────────────────
        .child(
            rect()
                .width(Size::Fill)
                .height(Size::flex(1.0))
                .padding(20.0)
                .spacing(16.0)
                .content(Content::Flex)
                .horizontal()
                .cross_align(Alignment::Start)
                // ── Left panel: Screenshot ───────────────────────────
                .child(
                    rect()
                        .expanded_y()
                        .width(Size::percent(40.0))
                        .content(Content::Flex)
                        .vertical()
                        .spacing(12.0)
                        // Section header
                        .child(
                            rect()
                                .width(Size::Fill)
                                .content(Content::Flex)
                                .horizontal()
                                .cross_align(Alignment::Center)
                                .spacing(8.0)
                                .child(
                                    svg(freya::icons::lucide::image())
                                        .color(colors::ACCENT)
                                        .width(Size::px(16.0))
                                        .height(Size::px(16.0)),
                                )
                                .child(
                                    label()
                                        .text("Screenshot")
                                        .font_size(13.0)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .color(colors::TEXT_SECTION),
                                ),
                        )
                        // Image container
                        .child(
                            rect()
                                .expanded()
                                .background(colors::BG_PANEL)
                                .rounded_lg()
                                .border(Border::new().width(1.0).fill(colors::BORDER_PANEL))
                                .shadow(
                                    Shadow::new()
                                        .x(0.0)
                                        .y(4.0)
                                        .blur(16.0)
                                        .spread(0.0)
                                        .color(colors::SHADOW_PANEL),
                                )
                                .overflow(Overflow::Clip)
                                .center()
                                .padding(8.0)
                                .child(
                                    ImageViewer::new((&id, bytes))
                                        .sampling_mode(SamplingMode::Trilinear),
                                ),
                        ),
                )
                // ── Right panel: Extracted text ─────────────────────
                .child(
                    rect()
                        .expanded_y()
                        .width(Size::percent(60.0))
                        .content(Content::Flex)
                        .vertical()
                        .spacing(12.0)
                        // Section header
                        .child(
                            rect()
                                .width(Size::Fill)
                                .content(Content::Flex)
                                .horizontal()
                                .cross_align(Alignment::Center)
                                .main_align(Alignment::Start)
                                .spacing(8.0)
                                .child(
                                    svg(freya::icons::lucide::file_text())
                                        .color(colors::ACCENT)
                                        .width(Size::px(16.0))
                                        .height(Size::px(16.0)),
                                )
                                .child(
                                    label()
                                        .text("Extracted Text")
                                        .font_size(13.0)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .color(colors::TEXT_SECTION),
                                ),
                        )
                        // Text content area with scrolling
                        .child(
                            rect()
                                .expanded()
                                .background(colors::BG_PANEL)
                                .rounded_lg()
                                .border(Border::new().width(1.0).fill(colors::BORDER_PANEL))
                                .shadow(
                                    Shadow::new()
                                        .x(0.0)
                                        .y(4.0)
                                        .blur(16.0)
                                        .spread(0.0)
                                        .color(colors::SHADOW_PANEL),
                                )
                                .content(Content::Flex)
                                .vertical()
                                .padding(20.0)
                                .child(
                                    ScrollView::new().expanded().child(
                                        paragraph()
                                            .width(Size::Fill)
                                            .padding(Gaps::new(20.0, 20.0, 20.0, 20.0))
                                            .line_height(1.7)
                                            .span(
                                                Span::new(text.read().clone())
                                                    .font_size(15.0)
                                                    .color(colors::TEXT_BODY),
                                            ),
                                    ),
                                ),
                        )
                        // Action bar: copy button
                        .child(
                            rect()
                                .width(Size::Fill)
                                .content(Content::Flex)
                                .horizontal()
                                .cross_align(Alignment::Center)
                                .main_align(Alignment::End)
                                .spacing(10.0)
                                // Hint text
                                .child(
                                    label()
                                        .text("Click to copy extracted text")
                                        .font_size(12.0)
                                        .color(colors::TEXT_HINT),
                                )
                                // Copy button
                                .child(
                                    Button::new()
                                        .on_press(move |_| {
                                            if let Err(e) = Clipboard::set(text.read().clone()) {
                                                eprintln!("Failed to copy to clipboard: {:?}", e);
                                            } else {
                                                copied.set(true);
                                                spawn(async move {
                                                    tokio::time::sleep(
                                                        std::time::Duration::from_secs(2),
                                                    )
                                                    .await;
                                                    copied.set(false);
                                                });
                                            }
                                        })
                                        .filled()
                                        .child(
                                            rect()
                                                .content(Content::Flex)
                                                .horizontal()
                                                .spacing(8.0)
                                                .center()
                                                .child(if *copied.read() {
                                                    svg(freya::icons::lucide::check())
                                                        .color(colors::SUCCESS)
                                                        .width(Size::px(16.0))
                                                        .height(Size::px(16.0))
                                                } else {
                                                    svg(freya::icons::lucide::copy())
                                                        .color(colors::TEXT_PRIMARY)
                                                        .width(Size::px(16.0))
                                                        .height(Size::px(16.0))
                                                })
                                                .child(
                                                    label()
                                                        .text(if *copied.read() {
                                                            "Copied!"
                                                        } else {
                                                            "Copy to Clipboard"
                                                        })
                                                        .font_size(13.0)
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .color(if *copied.read() {
                                                            colors::SUCCESS
                                                        } else {
                                                            colors::TEXT_PRIMARY
                                                        }),
                                                ),
                                        ),
                                ),
                        ),
                ),
        )
        // ── Footer ──────────────────────────────────────────────────
        .child(
            rect()
                .width(Size::Fill)
                .margin(Gaps::new(30.0, 0.0, 0.0, 0.0))
                .padding(Gaps::new(10.0, 24.0, 10.0, 24.0))
                .content(Content::Flex)
                .horizontal()
                .cross_align(Alignment::Center)
                .main_align(Alignment::Center)
                .background(colors::BG_FOOTER)
                .border(Border::new().width(1.0).fill(colors::BORDER_FOOTER))
                .child(
                    label()
                        .text("Powered by local LLM + Freya  \u{2022}  openocr")
                        .font_size(11.0)
                        .color(colors::TEXT_MUTED),
                ),
        )
}

#[derive(Serialize, Deserialize)]
struct ScreenshotData {
    screenshot_path: String,
    created_at: u64,
    response: String,
}

impl App for TextDisplayWindow {
    fn render(&self) -> impl IntoElement {
        let mut img_bytes = use_state(|| (Instant::now(), Bytes::default()));
        let mut text = use_state(String::new);

        let mut start = use_state(|| CursorPoint::new(0.0, 0.0));
        let mut end = use_state(|| CursorPoint::new(0.0, 0.0));

        let mut should_capture = use_state(|| false);

        let mut is_opened = use_state(|| false);

        let width = (end.read().x - start.read().x).abs() as f32;
        let height = (end.read().y - start.read().y).abs() as f32;

        let url = self.url.clone();
        let model = self.model.clone().to_string();
        let display_screenshot = self.display_screenshot;
        let monitor = self.monitor.clone();

        let x = start.read().x.min(end.read().x);
        let y = start.read().y.min(end.read().y);

        let on_open = move |_| {
            if *is_opened.read() {
                return;
            }
            spawn(async move {
                Platform::get()
                    .launch_window(
                        WindowConfig::new(move || sub_app(img_bytes, text)).with_title("OpenOCR"),
                    )
                    .await;
                is_opened.set(true);
            });
        };

        rect()
            .expanded()
            .opacity(0.4)
            .background(colors::BG_OVERLAY)
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if e.key.eq(&Key::Named(NamedKey::Escape)) {
                    std::process::exit(0);
                }
            })
            .on_mouse_down(move |e: Event<MouseEventData>| {
                start.set(e.global_location);
                should_capture.set(true);
            })
            .on_mouse_up(move |e: Event<MouseEventData>| {
                end.set(e.global_location);
                should_capture.set(false);
                if width < 10.0 || height < 10.0 {
                    return;
                }
                dbg!("Capturing monitor: {}", monitor.name());

                let image = monitor.capture_image().unwrap();
                let cropped = image::imageops::crop_imm(
                    &image,
                    x as u32,
                    y as u32,
                    width as u32,
                    height as u32,
                )
                .to_image();
                let mut bytes: Vec<u8> = Vec::new();
                cropped
                    .write_to(
                        &mut std::io::Cursor::new(&mut bytes),
                        image::ImageFormat::Png,
                    )
                    .unwrap();

                let base64_str = base64::engine::general_purpose::STANDARD.encode(&bytes);

                let builder = openai::CompletionsClient::<ReqwestClient>::builder()
                    .api_key("")
                    .base_url(&url);

                let client = builder.build().unwrap();

                let preamble = "Extract the text from the \
                                                following image and do not translate it.";

                let m = model.clone();

                let agent = client.agent(&m).preamble(preamble).temperature(0.5).build();

                let image = Image {
                    data: DocumentSourceKind::base64(&base64_str),
                    media_type: Some(ImageMediaType::PNG),
                    detail: Some(ImageDetail::Auto),
                    additional_params: None,
                    ..Default::default()
                };

                spawn(async move {
                    let entry_id = uuid::Uuid::new_v4().to_string();
                    let response = agent.prompt(image).await.unwrap();

                    let screenshot_path_path_buf = dirs::home_dir()
                        .unwrap()
                        .join(".openocr")
                        .join("screenshots")
                        .join(format!("{}.png", entry_id));

                    let screenshot_path = screenshot_path_path_buf.to_string_lossy().to_string();

                    let entry: ScreenshotData = ScreenshotData {
                        screenshot_path: screenshot_path.clone(),
                        created_at: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        response: response.clone(),
                    };

                    let db_path = dirs::home_dir()
                        .unwrap()
                        .join(".openocr")
                        .join("history.json");

                    let history = if db_path.exists() {
                        let data = std::fs::read_to_string(&db_path).unwrap();
                        serde_json::from_str::<Vec<ScreenshotData>>(&data).unwrap_or_default()
                    } else {
                        Vec::new()
                    };

                    let mut new_history = vec![entry];
                    new_history.extend(history);

                    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
                    std::fs::write(
                        &db_path,
                        serde_json::to_string_pretty(&new_history).unwrap(),
                    )
                    .unwrap();

                    std::fs::create_dir_all(screenshot_path_path_buf.parent().unwrap()).unwrap();
                    cropped.save(&screenshot_path_path_buf).unwrap();

                    dbg!("Response: {:#?}", &response);

                    text.set(response);

                    if display_screenshot {
                        img_bytes.set((Instant::now(), Bytes::from(bytes)));
                    }
                    on_open(());
                });
            })
            .on_mouse_move(move |e: Event<MouseEventData>| {
                end.set(e.global_location);
            })
            .maybe_child(should_capture.read().then(|| {
                rect()
                    .width(Size::px(width))
                    .height(Size::px(height))
                    .background(colors::SELECTION)
                    .position(Position::new_absolute().top(y as f32).left(x as f32))
            }))
    }
}
