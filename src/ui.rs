use std::{
    borrow::Cow,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use freya::prelude::*;
use xcap::Monitor;

use crate::history;

mod colors {
    use freya::prelude::Color;

    pub const CANVAS: Color = Color::from_rgb(247, 248, 250);
    pub const SURFACE: Color = Color::WHITE;
    pub const BORDER: Color = Color::from_rgb(229, 232, 239);
    pub const INK: Color = Color::from_rgb(29, 35, 51);
    pub const MUTED: Color = Color::from_rgb(106, 115, 135);
    pub const ACCENT: Color = Color::from_rgb(99, 85, 221);
    pub const ACCENT_HOVER: Color = Color::from_rgb(80, 66, 194);
    pub const ACCENT_SOFT: Color = Color::from_rgb(239, 237, 253);
    pub const SUCCESS: Color = Color::from_rgb(29, 125, 96);
    pub const ERROR: Color = Color::from_rgb(176, 51, 65);
    pub const ERROR_SOFT: Color = Color::from_rgb(255, 239, 241);
    pub const OVERLAY: Color = Color::from_af32rgb(0.22, 15, 19, 34);
    pub const SELECTION: Color = Color::from_af32rgb(0.12, 139, 125, 255);
}

#[derive(Clone, Copy, PartialEq)]
enum CaptureStatus {
    Idle,
    Capturing,
    Reading,
}

pub struct ScreenshotSession {
    img_bytes: (Instant, Bytes),
    text: String,
    history: Vec<history::ScreenshotData>,
    selected_path: Option<String>,
    display_screenshot: bool,
    status: CaptureStatus,
    error: Option<String>,
    window_id: Option<freya::winit::window::WindowId>,
}

impl ScreenshotSession {
    pub fn new(display_screenshot: bool) -> Self {
        Self {
            img_bytes: (Instant::now(), Bytes::new()),
            text: String::new(),
            history: history::get_history()
                .unwrap_or_default()
                .into_iter()
                .filter(|entry| std::path::Path::new(&entry.screenshot_path).exists())
                .collect(),
            selected_path: None,
            display_screenshot,
            status: CaptureStatus::Idle,
            error: None,
            window_id: None,
        }
    }

    fn show_history(&mut self, entry: &history::ScreenshotData) -> std::io::Result<()> {
        let bytes = if self.display_screenshot {
            Bytes::from(std::fs::read(&entry.screenshot_path)?)
        } else {
            Bytes::new()
        };
        self.img_bytes = (Instant::now(), bytes);
        self.text = entry.response.clone();
        self.selected_path = Some(entry.screenshot_path.clone());
        self.error = None;
        Ok(())
    }

    fn finish_capture(&mut self, result: anyhow::Result<()>) {
        self.status = CaptureStatus::Idle;
        if let Err(error) = result {
            eprintln!("OCR capture failed: {error:?}");
            self.error = Some(format!(
                "Could not extract text. Check your OCR server and try again. {error:#}"
            ));
        }
    }
}

pub struct TextDisplayWindow {
    pub model: Cow<'static, str>,
    pub url: Cow<'static, str>,
    pub monitor: Monitor,
    pub timeout: Duration,
    pub session: State<ScreenshotSession>,
}

impl App for TextDisplayWindow {
    fn render(&self) -> impl IntoElement {
        let mut session = self.session;
        let mut start = use_state(|| CursorPoint::new(0.0, 0.0));
        let mut end = use_state(|| CursorPoint::new(0.0, 0.0));
        let mut dragging = use_state(|| false);
        let root_size = *Platform::get().current_window().root_size.read();
        let status = session.read().status;
        let error = session.read().error.clone();
        let width = (end.read().x - start.read().x).abs() as f32;
        let height = (end.read().y - start.read().y).abs() as f32;
        let x = start.read().x.min(end.read().x) as f32;
        let y = start.read().y.min(end.read().y) as f32;
        let url = self.url.clone();
        let model = self.model.clone();
        let monitor = self.monitor.clone();
        let timeout = self.timeout;

        rect()
            .expanded()
            .cursor(CursorIcon::Crosshair)
            .maybe(status != CaptureStatus::Capturing, |r| {
                r.background(colors::OVERLAY)
            })
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if e.key == Key::Named(NamedKey::Escape) {
                    drop(Platform::get().post_callback(|renderer| renderer.exit()));
                }
            })
            .on_mouse_down(move |e: Event<MouseEventData>| {
                if e.button == Some(MouseButton::Left)
                    && session.peek().status == CaptureStatus::Idle
                {
                    start.set(e.global_location);
                    end.set(e.global_location);
                    dragging.set(true);
                    session.write().error = None;
                }
            })
            .on_mouse_move(move |e: Event<MouseEventData>| {
                if *dragging.peek() {
                    end.set(e.global_location);
                }
            })
            .on_mouse_up(move |e: Event<MouseEventData>| {
                if e.button != Some(MouseButton::Left) || !*dragging.peek() {
                    return;
                }
                dragging.set(false);
                let start = *start.peek();
                let end = e.global_location;
                if (end.x - start.x).abs() < 10.0 || (end.y - start.y).abs() < 10.0 {
                    return;
                }
                session.write().status = CaptureStatus::Capturing;
                let monitor = monitor.clone();
                let url = url.clone();
                let model = model.clone();

                spawn_global(async move {
                    let result: anyhow::Result<()> = async {
                        // Let every overlay paint transparent before taking the screenshot.
                        tokio::time::sleep(Duration::from_millis(120)).await;
                        let image = monitor.capture_image()?;
                        let (x, y, width, height) =
                            capture_bounds(start, end, root_size, image.dimensions()).ok_or_else(
                                || anyhow::anyhow!("Selection is outside the screen"),
                            )?;
                        let cropped =
                            image::imageops::crop_imm(&image, x, y, width, height).to_image();
                        let mut bytes = Vec::new();
                        cropped.write_to(
                            &mut std::io::Cursor::new(&mut bytes),
                            image::ImageFormat::Png,
                        )?;
                        let bytes = Bytes::from(bytes);
                        session.write().status = CaptureStatus::Reading;
                        let response =
                            crate::ocr::perform_ocr(&url, &model, &bytes, timeout).await?;
                        let saved = tokio::task::spawn_blocking({
                            let bytes = bytes.clone();
                            let response = response.clone();
                            move || history::save_screenshot(&bytes, &response)
                        })
                        .await
                        .context("History save task failed")
                        .and_then(|result| result);
                        {
                            let mut session = session.write();
                            match saved {
                                Ok(entry) => {
                                    session.selected_path = Some(entry.screenshot_path.clone());
                                    session.history.insert(0, entry);
                                }
                                Err(e) => {
                                    session.selected_path = None;
                                    session.error = Some(format!(
                                        "Text extracted, but history could not be saved: {e:#}"
                                    ));
                                }
                            }
                            session.img_bytes = (
                                Instant::now(),
                                if session.display_screenshot {
                                    bytes
                                } else {
                                    Bytes::new()
                                },
                            );
                            session.text = response;
                        }

                        // Keep lookup and focus/create together so closing the window cannot race us.
                        Platform::get()
                            .post_callback(move |renderer| {
                                let window_id = session.peek().window_id;
                                if let Some(window) =
                                    window_id.and_then(|id| renderer.windows.get_mut(&id))
                                {
                                    window.window().focus_window();
                                } else {
                                    let window_id = renderer.launch_window(
                                        WindowConfig::new(move || ResultWindow { session })
                                            .with_title("OpenOCR · Workspace")
                                            .with_size(1180.0, 780.0)
                                            .with_min_size(760.0, 600.0)
                                            .with_background(colors::CANVAS),
                                    );
                                    session.write().window_id = Some(window_id);
                                }
                            })
                            .await?;
                        Ok(())
                    }
                    .await;

                    session.write().finish_capture(result);
                });
            })
            .maybe_child((status != CaptureStatus::Capturing).then(|| {
                rect()
                    .width(Size::Fill)
                    .position(Position::new_absolute().top(32.0).left(0.0))
                    .cross_align(Alignment::Center)
                    .child(
                        rect()
                            .horizontal()
                            .cross_align(Alignment::Center)
                            .spacing(14.0)
                            .padding(Gaps::new(14.0, 20.0, 14.0, 20.0))
                            .corner_radius(16.0)
                            .background(colors::INK)
                            .shadow(Shadow::new().y(6.0).blur(24.0).color(colors::OVERLAY))
                            .child(icon(
                                freya::icons::lucide::scan_text(),
                                colors::SURFACE,
                                24.0,
                            ))
                            .child(
                                rect()
                                    .spacing(3.0)
                                    .child(
                                        label()
                                            .text(if status == CaptureStatus::Reading {
                                                "Reading your selection…"
                                            } else {
                                                "Select a screen region"
                                            })
                                            .font_size(15.0)
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .color(colors::SURFACE),
                                    )
                                    .child(
                                        label()
                                            .text(error.unwrap_or_else(|| {
                                                "Drag to select a region  ·  Esc to quit"
                                                    .to_string()
                                            }))
                                            .max_lines(2)
                                            .max_width(Size::px(520.0))
                                            .font_size(12.0)
                                            .color((197, 202, 217)),
                                    ),
                            ),
                    )
            }))
            .maybe_child((*dragging.read()).then(|| {
                rect()
                    .width(Size::px(width))
                    .height(Size::px(height))
                    .position(Position::new_absolute().top(y).left(x))
                    .background(colors::SELECTION)
                    .border(Border::new().width(2.0).fill(colors::ACCENT))
                    .corner_radius(4.0)
                    .overflow(Overflow::Clip)
            }))
    }
}

fn capture_bounds(
    start: CursorPoint,
    end: CursorPoint,
    logical_size: Size2D,
    image_size: (u32, u32),
) -> Option<(u32, u32, u32, u32)> {
    if logical_size.width <= 0.0 || logical_size.height <= 0.0 {
        return None;
    }
    let scale_x = image_size.0 as f64 / logical_size.width as f64;
    let scale_y = image_size.1 as f64 / logical_size.height as f64;
    let x = (start.x.min(end.x) * scale_x)
        .floor()
        .clamp(0.0, image_size.0 as f64) as u32;
    let y = (start.y.min(end.y) * scale_y)
        .floor()
        .clamp(0.0, image_size.1 as f64) as u32;
    let right = (start.x.max(end.x) * scale_x)
        .ceil()
        .clamp(0.0, image_size.0 as f64) as u32;
    let bottom = (start.y.max(end.y) * scale_y)
        .ceil()
        .clamp(0.0, image_size.1 as f64) as u32;
    (right > x && bottom > y).then_some((x, y, right - x, bottom - y))
}

#[derive(PartialEq)]
struct ResultWindow {
    session: State<ScreenshotSession>,
}

impl Component for ResultWindow {
    fn render(&self) -> impl IntoElement {
        use_init_theme(|| {
            let mut theme = light_theme();
            theme.colors = ColorsSheet {
                primary: colors::ACCENT,
                tertiary: colors::ACCENT_HOVER,
                secondary: colors::ACCENT_SOFT,
                background: colors::CANVAS,
                surface_tertiary: colors::SURFACE,
                surface_secondary: colors::ACCENT_SOFT,
                border: colors::BORDER,
                border_focus: colors::ACCENT,
                text_primary: colors::INK,
                text_secondary: colors::MUTED,
                ..LIGHT_COLORS
            };
            theme
        });

        let mut session = self.session;
        let displayed_bytes = session.read().img_bytes.clone();
        let displayed_text = session.read().text.clone();
        let capture_id = displayed_bytes.0;
        let mut copied = use_state(|| None::<Instant>);
        let is_copied = *copied.read() == Some(capture_id);
        let has_image = !displayed_bytes.1.is_empty();
        let compact = Platform::get().current_window().root_size.read().width < 1050.0;
        let len = session.read().history.len();
        let error = session.read().error.clone();
        // Prioritize readable text and recovery when a compact workspace needs an error banner.
        let show_source = has_image && (!compact || error.is_none());
        let words = displayed_text.split_whitespace().count();
        let characters = displayed_text.chars().count();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        rect()
            .expanded()
            .horizontal()
            .content(Content::Flex)
            .background(colors::CANVAS)
            .color(colors::INK)
            .font_size(14.0)
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                if e.key == Key::Named(NamedKey::Escape) {
                    drop(Platform::get().post_callback(|renderer| renderer.exit()));
                }
            })
            .child(
                rect()
                    .width(Size::px(if compact { 208.0 } else { 240.0 }))
                    .height(Size::Fill)
                    .content(Content::Flex)
                    .padding(20.0)
                    .spacing(24.0)
                    .background(colors::SURFACE)
                    .border(Border::new().width(1.0).fill(colors::BORDER))
                    .child(
                        rect().horizontal().cross_align(Alignment::Center).spacing(10.0)
                            .child(rect().padding(9.0).corner_radius(12.0).background(colors::ACCENT)
                                .child(icon(freya::icons::lucide::scan_text(), colors::SURFACE, 23.0)))
                            .child(label().text("openocr").font_size(20.0).font_weight(FontWeight::BOLD))
                    )
                    .child(
                        rect().width(Size::Fill).horizontal().content(Content::Flex).cross_align(Alignment::Center)
                            .child(label().text("CAPTURE LIBRARY").font_size(10.0).font_weight(FontWeight::SEMI_BOLD).color(colors::MUTED).width(Size::flex(1.0)))
                            .child(badge(len.to_string(), colors::CANVAS, colors::MUTED))
                    )
                    .child(
                        VirtualScrollView::new(move |i, _| {
                            let entry = session.read().history[i.index].clone();
                            let active = session.read().selected_path.as_deref() == Some(entry.screenshot_path.as_str());
                            let preview = entry.response.lines().find(|line| !line.trim().is_empty()).unwrap_or("Untitled capture").trim().to_string();
                            let path = PathBuf::from(&entry.screenshot_path);
                            let age = capture_age(entry.created_at, now);
                            Button::new()
                                .key(entry.screenshot_path.clone())
                                .width(Size::Fill)
                                .height(Size::px(i.size))
                                .padding(10.0)
                                .corner_radius(12.0)
                                .background(if active { colors::ACCENT_SOFT } else { colors::SURFACE })
                                .border_fill(if active { colors::ACCENT_SOFT } else { colors::SURFACE })
                                .hover_background(colors::CANVAS)
                                .on_press(move |_| {
                                    let result = session.write().show_history(&entry);
                                    if let Err(e) = result {
                                        session.write().error = Some(format!("Could not open this capture: {e}"));
                                    }
                                })
                                .child(
                                    rect().width(Size::Fill).horizontal().content(Content::Flex).spacing(10.0).cross_align(Alignment::Center)
                                        .child(rect().width(Size::px(42.0)).height(Size::px(52.0)).padding(4.0)
                                            .corner_radius(7.0).background(colors::SURFACE).overflow(Overflow::Clip)
                                            .child(ImageViewer::new(ImageSource::Path(path)).width(Size::Fill).height(Size::Fill)
                                                .sampling_mode(SamplingMode::Trilinear)))
                                        .child(rect().width(Size::flex(1.0)).spacing(6.0)
                                            .child(label().text(preview).font_size(12.0).font_weight(FontWeight::MEDIUM)
                                                .max_lines(2).text_overflow(TextOverflow::Ellipsis)
                                                .color(if active { colors::ACCENT } else { colors::INK }))
                                            .child(label().text(age).font_size(10.0).color(colors::MUTED)))
                                ).into()
                        })
                        .length(len)
                        .item_size(98.0)
                        .width(Size::Fill)
                        .height(Size::flex(1.0)),
                    )
                    .maybe_child((len == 0).then(|| label().text("No captures yet.").font_size(12.0).color(colors::MUTED))),
            )
            .child(
                rect()
                    .width(Size::flex(1.0))
                    .height(Size::Fill)
                    .content(Content::Flex)
                    .padding(if compact { 24.0 } else { 32.0 })
                    .spacing(24.0)
                    .child(
                        rect().width(Size::Fill).horizontal().content(Content::Flex).cross_align(Alignment::Center)
                            .child(rect().width(Size::flex(1.0)).horizontal().spacing(8.0).cross_align(Alignment::Center)
                                .child(icon(freya::icons::lucide::layers(), colors::MUTED, 15.0))
                                .child(label().text("Workspace").font_size(12.0).color(colors::MUTED))
                                .child(label().text("/").font_size(12.0).color(colors::MUTED))
                                .child(label().text("Text extraction").font_size(12.0)))
                    )
                    .maybe_child(error.map(|error| rect().width(Size::Fill).padding(12.0).corner_radius(10.0)
                        .background(colors::ERROR_SOFT).color(colors::ERROR).child(label().text(error).font_size(12.0))))
                    .child(
                        rect().width(Size::Fill).height(Size::flex(1.0)).content(Content::Flex).spacing(20.0)
                            .maybe(!compact, |r| r.horizontal())
                            .maybe_child(show_source.then(|| {
                                panel()
                                    .width(if compact { Size::Fill } else { Size::flex(0.9) })
                                    .height(if compact { Size::px(190.0) } else { Size::Fill })
                                    .child(panel_header(freya::icons::lucide::image(), "Source image")
                                        .child(badge("PNG", colors::CANVAS, colors::MUTED)))
                                    .child(
                                        rect().width(Size::Fill).height(Size::flex(1.0)).padding(20.0).center().background(colors::CANVAS)
                                            .child(ImageViewer::new(displayed_bytes).width(Size::Fill).height(Size::Fill)
                                                .image_cover(ImageCover::Center).sampling_mode(SamplingMode::Trilinear).a11y_alt("Selected screen capture"))
                                    )
                                    .child(rect().width(Size::Fill).padding(14.0).horizontal().spacing(6.0).cross_align(Alignment::Center)
                                        .child(icon(freya::icons::lucide::maximize(), colors::MUTED, 12.0))
                                        .child(label().text("Original Image").font_size(11.0).color(colors::MUTED)))
                            }))
                            .child(
                                panel()
                                    .width(if compact || !has_image { Size::Fill } else { Size::flex(1.3) })
                                    .height(if compact { Size::flex(1.0) } else { Size::Fill })
                                    .child(
                                        panel_header(freya::icons::lucide::file_text(), "Extracted text")
                                            .child(
                                                Button::new().filled().enabled(!displayed_text.is_empty())
                                                    .padding(Gaps::new(8.0, 12.0, 8.0, 12.0)).corner_radius(9.0)
                                                    .background(if is_copied { colors::SUCCESS } else { colors::ACCENT })
                                                    .hover_background(if is_copied { colors::SUCCESS } else { colors::ACCENT_HOVER })
                                                    .on_press(move |_| {
                                                        let text = session.peek().text.clone();
                                                        if let Err(e) = Clipboard::set(text) {
                                                            session.write().error = Some(format!("Could not copy text: {e:?}"));
                                                        } else {
                                                            copied.set(Some(capture_id));
                                                            spawn(async move {
                                                                tokio::time::sleep(Duration::from_secs(2)).await;
                                                                if *copied.peek() == Some(capture_id) { copied.set(None); }
                                                            });
                                                        }
                                                    })
                                                    .child(rect().horizontal().cross_align(Alignment::Center).spacing(7.0)
                                                        .child(icon(if is_copied { freya::icons::lucide::check() } else { freya::icons::lucide::copy() }, colors::SURFACE, 14.0))
                                                        .child(label().text(if is_copied { "Copied!" } else { "Copy text" }).font_size(12.0).font_weight(FontWeight::SEMI_BOLD).color(colors::SURFACE)))
                                            )
                                    )
                                    .child(
                                        ScrollView::new().height(Size::flex(1.0)).width(Size::Fill)
                                            .child(rect().width(Size::Fill).padding(if compact { 20.0 } else { 28.0 })
                                                .child(SelectableText::new().key(capture_id).width(Size::Fill).font_size(15.0).line_height(1.65)
                                                    .color(if displayed_text.is_empty() { colors::MUTED } else { colors::INK })
                                                    .span(if displayed_text.is_empty() { "No text was found in this capture. Try selecting a clearer region.".to_string() } else { displayed_text })))
                                    )
                                    .child(rect().width(Size::Fill).padding(Gaps::new(14.0, 20.0, 14.0, 20.0)).horizontal().spacing(8.0)
                                        .border(Border::new().width(1.0).fill(colors::BORDER))
                                        .child(label().text(format!("{words} words")).font_size(11.0).color(colors::MUTED))
                                        .child(label().text("·").font_size(11.0).color(colors::MUTED))
                                        .child(label().text(format!("{characters} characters")).font_size(11.0).color(colors::MUTED)))
                            )
                    )
                    .child(
                        rect().width(Size::Fill).horizontal().content(Content::Flex).cross_align(Alignment::Center)
                            .child(rect().width(Size::flex(1.0)).horizontal().spacing(7.0).cross_align(Alignment::Center)
                                .child(icon(freya::icons::lucide::hard_drive(), colors::MUTED, 13.0))
                                .child(label().text("Saved on your device.").font_size(11.0).color(colors::MUTED)))
                    )
            )
    }
}

fn icon(bytes: Bytes, color: Color, size: f32) -> SvgViewer {
    SvgViewer::new(bytes)
        .color(color)
        .width(Size::px(size))
        .height(Size::px(size))
}

fn badge(text: impl Into<Cow<'static, str>>, background: Color, color: Color) -> Rect {
    rect()
        .padding(Gaps::new(5.0, 8.0, 5.0, 8.0))
        .corner_radius(6.0)
        .background(background)
        .child(
            label()
                .text(text)
                .font_size(10.0)
                .font_weight(FontWeight::MEDIUM)
                .color(color),
        )
}

fn panel() -> Rect {
    rect()
        .content(Content::Flex)
        .background(colors::SURFACE)
        .corner_radius(16.0)
        .border(Border::new().width(1.0).fill(colors::BORDER))
        .overflow(Overflow::Clip)
        .shadow(
            Shadow::new()
                .y(3.0)
                .blur(12.0)
                .color(Color::from_af32rgb(0.025, 29, 35, 51)),
        )
}

fn panel_header(bytes: Bytes, title: &'static str) -> Rect {
    rect()
        .width(Size::Fill)
        .height(Size::px(68.0))
        .padding(Gaps::new(14.0, 20.0, 14.0, 20.0))
        .horizontal()
        .content(Content::Flex)
        .cross_align(Alignment::Center)
        .spacing(9.0)
        .child(icon(bytes, colors::MUTED, 16.0))
        .child(
            label()
                .text(title)
                .width(Size::flex(1.0))
                .font_size(13.0)
                .font_weight(FontWeight::SEMI_BOLD),
        )
}

fn capture_age(created_at: u64, now: u64) -> String {
    match now.saturating_sub(created_at) {
        0..60 => "Just now".to_string(),
        seconds @ 60..3600 => format!("{} min ago", seconds / 60),
        seconds @ 3600..86400 => format!("{} hr ago", seconds / 3600),
        seconds => format!("{} d ago", seconds / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_bounds_scale_reverse_and_clip_selection() {
        let bounds = |start, end| capture_bounds(start, end, Size2D::new(100.0, 100.0), (200, 200));
        assert_eq!(
            bounds((80.0, 70.0).into(), (10.0, 20.0).into()),
            Some((20, 40, 140, 100))
        );
        assert_eq!(
            bounds((-10.0, 80.0).into(), (120.0, 120.0).into()),
            Some((0, 160, 200, 40))
        );
        assert_eq!(bounds((110.0, 10.0).into(), (120.0, 20.0).into()), None);
        assert_eq!(bounds((20.0, 20.0).into(), (20.0, 20.0).into()), None);
    }

    fn workspace() -> impl IntoElement {
        ResultWindow {
            session: use_consume::<State<ScreenshotSession>>(),
        }
    }

    fn fixture_session(display_screenshot: bool) -> ScreenshotSession {
        let source = concat!(env!("CARGO_MANIFEST_DIR"), "/./demo.png");
        ScreenshotSession {
            img_bytes: (
                Instant::now(),
                if display_screenshot {
                    Bytes::from_static(include_bytes!("../demo.png"))
                } else {
                    Bytes::new()
                },
            ),
            text: "let mut should_capture: State<bool> = use_state(|| false);".to_string(),
            history: vec![
                history::ScreenshotData {
                    screenshot_path: source.to_string(),
                    created_at: 1_800_000_000,
                    response: "let mut should_capture: State<bool> = use_state(|| false);"
                        .to_string(),
                },
                history::ScreenshotData {
                    screenshot_path: concat!(env!("CARGO_MANIFEST_DIR"), "/demo.png").to_string(),
                    created_at: 1_799_999_000,
                    response: "A second capture, ready to copy.".to_string(),
                },
            ],
            selected_path: Some(source.to_string()),
            display_screenshot,
            status: CaptureStatus::Idle,
            error: None,
            window_id: None,
        }
    }

    fn find_label(runner: &freya_testing::TestingRunner, text: &str) -> freya_testing::TestingNode {
        runner
            .find(|node, element| {
                (element as &dyn std::any::Any)
                    .downcast_ref::<freya::elements::label::LabelElement>()
                    .filter(|label| label.text == text)
                    .map(|_| node)
            })
            .unwrap_or_else(|| panic!("Missing label: {text}"))
    }

    #[test]
    fn workspace_layout_and_history_work_at_both_sizes() {
        for (width, height, display_screenshot, name) in [
            (1180.0, 780.0, true, "wide"),
            (760.0, 600.0, true, "compact"),
            (760.0, 600.0, false, "text-only"),
        ] {
            let (mut runner, mut session) = freya_testing::TestingRunner::new(
                workspace,
                (width, height).into(),
                |r| r.provide_root_context(|| State::create(fixture_session(display_screenshot))),
                1.0,
            );
            runner.poll_n(Duration::from_millis(30), 10);
            for text in ["Workspace", "Extracted text", "Copy text"] {
                let node = find_label(&runner, text);
                let area = node.layout().area;
                assert!(
                    area.min_x() >= 0.0 && area.max_x() <= width,
                    "{name}: {text} overflows horizontally: {area:?}"
                );
                assert!(
                    area.min_y() >= 0.0 && area.max_y() <= height,
                    "{name}: {text} overflows vertically: {area:?}"
                );
            }
            runner.render_to_file(format!("target/workspace-{name}.png"));
            let second = find_label(&runner, "A second capture, ready to copy.")
                .layout()
                .area
                .center();
            runner.click_cursor((second.x as f64, second.y as f64));
            assert_eq!(session.peek().text, "A second capture, ready to copy.");
            assert_eq!(session.peek().img_bytes.1.is_empty(), !display_screenshot);

            session.write().history[0].screenshot_path = "/missing/openocr-test.png".to_string();
            runner.sync_and_update();
            let first = find_label(
                &runner,
                "let mut should_capture: State<bool> = use_state(|| false);",
            )
            .layout()
            .area
            .center();
            runner.click_cursor((first.x as f64, first.y as f64));
            if display_screenshot {
                assert!(
                    session
                        .peek()
                        .error
                        .as_ref()
                        .is_some_and(|e| e.starts_with("Could not open this capture:"))
                );
                assert_eq!(session.peek().text, "A second capture, ready to copy.");
                let paragraph = runner
                    .find(|node, element| {
                        (element as &dyn std::any::Any)
                            .downcast_ref::<freya::elements::paragraph::ParagraphElement>()
                            .filter(|paragraph| {
                                paragraph
                                    .spans
                                    .iter()
                                    .any(|span| span.text == "A second capture, ready to copy.")
                            })
                            .map(|_| node)
                    })
                    .expect("Previous text should remain readable after a history error");
                let area = paragraph.layout().area;
                assert!(
                    area.min_y() >= 0.0
                        && area.max_y() <= find_label(&runner, "6 words").layout().area.min_y(),
                    "{name}: error clipped the preserved text: {area:?}"
                );
                runner.render_to_file(format!("target/workspace-{name}-error.png"));
            } else {
                assert!(session.peek().error.is_none());
                assert!(session.peek().img_bytes.1.is_empty());
                assert_eq!(
                    session.peek().text,
                    "let mut should_capture: State<bool> = use_state(|| false);"
                );
            }
            session.write().text.clear();
            runner.sync_and_update();
            assert!(find_label(&runner, "0 words").layout().area.width() > 0.0);
        }
    }

    #[test]
    fn long_history_keeps_rows_aligned_while_scrolling() {
        let (mut runner, session) = freya_testing::TestingRunner::new(
            workspace,
            (760.0, 600.0).into(),
            |r| {
                r.provide_root_context(|| {
                    let mut session = fixture_session(false);
                    session.history = (0..30)
                        .map(|index| history::ScreenshotData {
                            screenshot_path: format!(
                                "{}/{}demo.png",
                                env!("CARGO_MANIFEST_DIR"),
                                "./".repeat(index)
                            ),
                            created_at: 1_800_000_000,
                            response: format!("Capture {index}"),
                        })
                        .collect();
                    State::create(session)
                })
            },
            1.0,
        );
        for scroll in [0.0, -140.0, -10_000.0] {
            runner.scroll((100.0, 250.0), (0.0, scroll));
            let rows = runner.find_many(|node, element| {
                (element as &dyn std::any::Any)
                    .downcast_ref::<freya::elements::label::LabelElement>()
                    .filter(|label| label.text.starts_with("Capture "))
                    .map(|_| node.layout().area)
            });
            assert!(rows.len() > 1);
            for pair in rows.windows(2) {
                assert!((pair[1].min_y() - pair[0].min_y() - 98.0).abs() < 0.1);
            }
        }
        let last = find_label(&runner, "Capture 29").layout().area.center();
        runner.click_cursor((last.x as f64, last.y as f64));
        assert_eq!(session.peek().text, "Capture 29");
    }

    #[test]
    fn failed_capture_returns_to_idle_and_preserves_previous_text() {
        for status in [CaptureStatus::Capturing, CaptureStatus::Reading] {
            let mut session = fixture_session(false);
            session.status = status;
            let previous = session.text.clone();
            session.finish_capture(Err(anyhow::anyhow!("OCR request timed out")));
            assert!(session.status == CaptureStatus::Idle);
            assert_eq!(session.text, previous);
            assert!(
                session
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("OCR request timed out")
            );
            session.status = CaptureStatus::Reading;
            session.error = None;
            session.finish_capture(Ok(()));
            assert!(session.status == CaptureStatus::Idle);
            assert!(session.error.is_none());
        }
    }

    #[test]
    fn capture_age_handles_boundaries_and_future_clocks() {
        assert_eq!(capture_age(101, 100), "Just now");
        assert_eq!(capture_age(0, 59), "Just now");
        assert_eq!(capture_age(0, 60), "1 min ago");
        assert_eq!(capture_age(0, 3600), "1 hr ago");
        assert_eq!(capture_age(0, 86400), "1 d ago");
    }
}
