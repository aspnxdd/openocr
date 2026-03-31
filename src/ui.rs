use std::{borrow::Cow, path::PathBuf, time::Instant};

use freya::prelude::*;
use xcap::Monitor;

use crate::history;

// ── Color palette ────────────────────────────────────────────────────────────

/// Color palette for the application UI, built on Tailwind CSS v4 colors.
pub mod colors {
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

// ── Layout helpers ───────────────────────────────────────────────────────────

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

// ── Overlay (screen selection) ───────────────────────────────────────────────

pub struct TextDisplayWindow {
    pub model: Cow<'static, str>,
    pub url: Cow<'static, str>,
    pub display_screenshot: bool,
    pub monitor: Monitor,
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
                        WindowConfig::new(move || result_window(img_bytes, text))
                            .with_title("OpenOCR"),
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

                let image = match monitor.capture_image() {
                    Ok(img) => img,
                    Err(e) => {
                        eprintln!("Failed to capture monitor image: {e:?}");
                        return;
                    }
                };
                let cropped = image::imageops::crop_imm(
                    &image,
                    x as u32,
                    y as u32,
                    width as u32,
                    height as u32,
                )
                .to_image();
                let mut bytes: Vec<u8> = Vec::new();
                if let Err(e) = cropped.write_to(
                    &mut std::io::Cursor::new(&mut bytes),
                    image::ImageFormat::Png,
                ) {
                    eprintln!("Failed to encode cropped image as PNG: {e:?}");
                    return;
                }

                let url = url.clone();
                let model = model.clone();

                spawn(async move {
                    let result: anyhow::Result<()> = async {
                        let response = crate::ocr::perform_ocr(&url, &model, &bytes).await?;

                        let img = image::load_from_memory(&bytes)
                            .map_err(|e| anyhow::anyhow!("Failed to load cropped image: {e}"))?;

                        if let Err(e) = history::save_screenshot(&img, &response) {
                            eprintln!("Failed to save screenshot: {e:?}");
                        }

                        dbg!("Response: {:#?}", &response);

                        text.set(response);

                        if display_screenshot {
                            img_bytes.set((Instant::now(), Bytes::from(bytes)));
                        }
                        on_open(());
                        Ok(())
                    }
                    .await;

                    if let Err(e) = result {
                        eprintln!("OCR capture failed: {e:?}");
                    }
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

// ── Result window ────────────────────────────────────────────────────────────

fn result_window(img_bytes: State<(Instant, Bytes)>, text: State<String>) -> impl IntoElement {
    use_init_theme(|| DARK_THEME);

    let mut displayed_bytes = use_state(|| img_bytes.read().clone());

    let mut displayed_text = use_state(|| text.read().clone());

    let mut copied = use_state(|| false);

    let filtered_history = history::get_history()
        .unwrap_or_default()
        .into_iter()
        .filter(|e| std::path::Path::new(&e.screenshot_path).exists())
        .collect::<Vec<_>>();

    dbg!("history: {:#?}", &filtered_history);

    let len = filtered_history.len();

    // Root container: dark background, horizontal layout
    rect()
        .expanded()
        .content(Content::Flex)
        .horizontal()
        .spacing(5.0)
        // ── Sidebar: history thumbnails ──────────────────────────────
        .child(
            rect()
                .height(Size::Fill)
                .content(Content::Flex)
                .vertical()
                .background(tw_colors::GRAY_500)
                .width(Size::px(200.0))
                .child(
                    rect()
                        .width(Size::Fill)
                        .padding(Gaps::new(16.0, 24.0, 16.0, 24.0))
                        .content(Content::Flex)
                        .vertical()
                        .spacing(8.0)
                        .child(
                            VirtualScrollView::new(move |i, _| {
                                let entry = &filtered_history[i];
                                let entry = entry.clone();
                                let path_buf = PathBuf::from(&entry.screenshot_path);
                                Button::new()
                                    .on_press(move |_| {
                                        displayed_text.set(entry.response.clone());
                                        displayed_bytes.set((
                                            Instant::now(),
                                            Bytes::from(
                                                std::fs::read(&entry.screenshot_path)
                                                    .unwrap_or_default(),
                                            ),
                                        ));
                                    })
                                    .child(
                                        rect()
                                            .margin(Gaps::new(4.0, 0.0, 4.0, 0.0))
                                            .key(i)
                                            .width(Size::Fill)
                                            .child(
                                                ImageViewer::new(ImageSource::Path(path_buf))
                                                    .sampling_mode(SamplingMode::Trilinear),
                                            ),
                                    )
                                    .into()
                            })
                            .length(len)
                            .item_size(50.)
                            .height(Size::percent(100.)),
                        ),
                ),
        )
        // ── Main content area ────────────────────────────────────────
        .child(
            rect()
                .expanded()
                .background(colors::BG_ROOT)
                .content(Content::Flex)
                .vertical()
                // ── Header bar ──────────────────────────────────────────
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
                // ── Main content area (two panels) ──────────────────────
                .child(
                    rect()
                        .width(Size::Fill)
                        .height(Size::flex(1.0))
                        .padding(20.0)
                        .spacing(16.0)
                        .content(Content::Flex)
                        .horizontal()
                        .cross_align(Alignment::Start)
                        // ── Left panel: Screenshot ───────────────────────
                        .child(
                            rect()
                                .expanded_y()
                                .width(Size::percent(40.0))
                                .content(Content::Flex)
                                .vertical()
                                .spacing(12.0)
                                // Section header
                                .child(section_header(freya::icons::lucide::image(), "Screenshot"))
                                // Image container
                                .child(
                                    rect()
                                        .expanded()
                                        .background(colors::BG_PANEL)
                                        .rounded_lg()
                                        .border(Border::new().width(1.0).fill(colors::BORDER_PANEL))
                                        .shadow(panel_shadow())
                                        .overflow(Overflow::Clip)
                                        .center()
                                        .padding(8.0)
                                        .child(
                                            ImageViewer::new(displayed_bytes.read().clone())
                                                .sampling_mode(SamplingMode::Trilinear),
                                        ),
                                ),
                        )
                        // ── Right panel: Extracted text ─────────────────
                        .child(
                            rect()
                                .expanded_y()
                                .width(Size::percent(60.0))
                                .content(Content::Flex)
                                .vertical()
                                .spacing(12.0)
                                // Section header
                                .child(section_header(
                                    freya::icons::lucide::file_text(),
                                    "Extracted Text",
                                ))
                                // Text content area with scrolling
                                .child(
                                    rect()
                                        .expanded()
                                        .background(colors::BG_PANEL)
                                        .rounded_lg()
                                        .border(Border::new().width(1.0).fill(colors::BORDER_PANEL))
                                        .shadow(panel_shadow())
                                        .content(Content::Flex)
                                        .vertical()
                                        .padding(20.0)
                                        .child(
                                            ScrollView::new().expanded().child(
                                                rect()
                                                    .width(Size::Fill)
                                                    .padding(2.0)
                                                    .font_size(15.0)
                                                    .color(colors::TEXT_BODY)
                                                    .child(
                                                        SelectableText::new(
                                                            displayed_text.read().clone(),
                                                        )
                                                        .into_element(),
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
                                                    if let Err(e) = Clipboard::set(
                                                        displayed_text.read().clone(),
                                                    ) {
                                                        eprintln!(
                                                            "Failed to copy to clipboard: {:?}",
                                                            e
                                                        );
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
                // ── Footer ──────────────────────────────────────────────
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
                ),
        )
}

/// A reusable section header with an icon and label.
fn section_header(icon: Bytes, title: &'static str) -> Rect {
    rect()
        .width(Size::Fill)
        .content(Content::Flex)
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(8.0)
        .child(
            svg(icon)
                .color(colors::ACCENT)
                .width(Size::px(16.0))
                .height(Size::px(16.0)),
        )
        .child(
            label()
                .text(title)
                .font_size(13.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .color(colors::TEXT_SECTION),
        )
}

/// Shared panel shadow style.
fn panel_shadow() -> Shadow {
    Shadow::new()
        .x(0.0)
        .y(4.0)
        .blur(16.0)
        .spread(0.0)
        .color(colors::SHADOW_PANEL)
}
