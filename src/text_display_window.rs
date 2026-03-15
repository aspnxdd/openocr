use std::borrow::Cow;

use base64::Engine;
use freya::{prelude::*, winit::dpi::PhysicalPosition};
use rig::{
    client::CompletionClient,
    completion::Prompt,
    http_client::ReqwestClient,
    message::{DocumentSourceKind, Image, ImageDetail, ImageMediaType},
    providers::openai,
};

pub struct TextDisplayWindow {
    pub model: Cow<'static, str>,
    pub url: Cow<'static, str>,
    pub display_screenshot: bool,
}

impl App for TextDisplayWindow {
    fn render(&self) -> impl IntoElement {
        let mut img_bytes = use_state(|| Bytes::default());
        let mut text = use_state(|| String::new());

        let mut start = use_state(|| CursorPoint::new(0.0, 0.0));
        let mut end = use_state(|| CursorPoint::new(0.0, 0.0));

        let mut should_capture = use_state(|| false);

        let width = (end.read().x - start.read().x).abs() as f32;
        let height = (end.read().y - start.read().y).abs() as f32;

        let url = self.url.clone();
        let model = self.model.clone().to_string();
        let display_screenshot = self.display_screenshot;

        rect()
            .expanded()
            .opacity(0.4)
            .background((25, 25, 25))
            .on_mouse_down(move |e: Event<MouseEventData>| {
                start.set(e.global_location);
                should_capture.set(true);
            })
            .on_mouse_up(move |e: Event<MouseEventData>| {
                end.set(e.global_location);
                let inst = std::time::Instant::now();
                let xcap_monitors = xcap::Monitor::all().unwrap(); // TODO pass via props to the APP
                println!("{}ms", inst.elapsed().as_millis());
                let target_monitor = xcap_monitors.iter().find(|monitor| {
                    let x = monitor.x();
                    let y = monitor.y();
                    let width = monitor.width() as f32;
                    let height = monitor.height() as f32;
                    let within_x = e.global_location.x >= x as f64
                        && e.global_location.x <= (x as f64 + width as f64);
                    let within_y = e.global_location.y >= y as f64
                        && e.global_location.y <= (y as f64 + height as f64);
                    within_x && within_y
                });

                if let Some(monitor) = target_monitor {
                    dbg!("Capturing monitor: {}", monitor.name());
                    should_capture.set(false);

                    let image = monitor.capture_image().unwrap();
                    let cropped = image::imageops::crop_imm(
                        &image,
                        start.read().x as u32,
                        start.read().y as u32,
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
                    // Prompt the agent and print the response

                    spawn(async move {
                        let response = agent.prompt(image).await.unwrap();

                        dbg!("Response: {:#?}", &response);

                        text.set(response);

                        if display_screenshot {
                            img_bytes.set(Bytes::from(bytes));
                        }
                    });
                }
            })
            .on_mouse_move(move |e: Event<MouseEventData>| {
                end.set(e.global_location);
            })
            .maybe_child(should_capture.read().then(|| {
                rect()
                    .width(Size::px(width))
                    .height(Size::px(height))
                    .background((255, 0, 0, 128))
                    .position(
                        Position::new_absolute()
                            .top(start.read().y.min(end.read().y) as f32)
                            .left(start.read().x.min(end.read().x) as f32),
                    )
            }))
    }

    //             let t = self.text.clone();
    //     let maybe_img = self.img_bytes.clone().map(|bytes| {
    //         rect()
    //             .width(Size::fill())
    //             .center()
    //             .child(ImageViewer::new(("image", bytes)))
    //     });
    //     rect()
    //         .padding(30.0)
    //         .spacing(10.0)
    //         .center()
    //         .maybe_child(maybe_img)
    //         .child(label().text(t.clone()))
    //         .child(
    //             Button::new()
    //                 .on_press(move |_| {
    //                     if let Err(e) = Clipboard::set(t.clone().into()) {
    //                         eprintln!("Failed to copy to clipboard: {:?}", e);
    //                     }
    //                 })
    //                 .outline()
    //                 .child(
    //                     rect()
    //                         .content(Content::Flex)
    //                         .horizontal()
    //                         .spacing(20.0)
    //                         .child(svg(freya::icons::lucide::copy()))
    //                         .child(label().text("Copy").color((0, 0, 0))),
    //                 ),
    //         )
    // }
}
