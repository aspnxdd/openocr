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
use xcap::Monitor;

pub struct TextDisplayWindow {
    pub model: Cow<'static, str>,
    pub url: Cow<'static, str>,
    pub display_screenshot: bool,
    pub monitor: Monitor,
}

fn sub_app(img_bytes: State<(Instant, Bytes)>, text: State<String>) -> impl IntoElement {
    let (id, bytes) = img_bytes.read().clone();

    rect()
        .padding(30.0)
        .spacing(10.0)
        .center()
        .child(
            rect()
                .width(Size::fill())
                .center()
                .child(ImageViewer::new((&id, bytes))),
        )
        .child(label().text(text.read().clone()))
        .child(
            Button::new()
                .on_press(move |_| {
                    if let Err(e) = Clipboard::set(text.read().clone()) {
                        eprintln!("Failed to copy to clipboard: {:?}", e);
                    }
                })
                .outline()
                .child(
                    rect()
                        .content(Content::Flex)
                        .horizontal()
                        .spacing(20.0)
                        .child(svg(freya::icons::lucide::copy()))
                        .child(label().text("Copy").color((0, 0, 0))),
                ),
        )
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
                    .launch_window(WindowConfig::new(move || sub_app(img_bytes, text)))
                    .await;
                is_opened.set(true);
            });
        };

        rect()
            .expanded()
            .opacity(0.4)
            .background((25, 25, 25))
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
                    let response = agent.prompt(image).await.unwrap();

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
                    .background((255, 0, 0, 128))
                    .position(Position::new_absolute().top(y as f32).left(x as f32))
            }))
    }
}
