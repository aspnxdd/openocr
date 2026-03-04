use base64::Engine;
use rig::{
    client::CompletionClient,
    completion::Prompt,
    http_client::ReqwestClient,
    message::{DocumentSourceKind, Image, ImageDetail, ImageMediaType},
    providers::openai,
};
use selection::{Selection, draw};
use softbuffer::{Context, Surface};
use std::error::Error;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{EventLoop, EventLoopWindowTarget},
    window::{WindowBuilder, WindowLevel},
};
mod selection;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let event_loop = EventLoop::new().unwrap();

    let monitors: Vec<_> = event_loop.available_monitors().collect();

    let windows: Vec<winit::window::Window> = monitors
        .iter()
        .map(|monitor| {
            WindowBuilder::new()
                .with_title("Select Region")
                .with_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(
                    monitor.clone(),
                ))))
                .with_window_level(WindowLevel::AlwaysOnTop)
                .with_decorations(false)
                .with_transparent(true)
                .build(&event_loop)
                .unwrap()
        })
        .collect();

    let mut surfaces: Vec<_> = windows
        .iter()
        .map(|window| {
            let context = Context::new(window).unwrap();
            let surface = Surface::new(&context, window).unwrap();
            (context, surface)
        })
        .collect();

    let mut selection = Selection::new();
    let mut cursor_pos = PhysicalPosition::new(0.0_f64, 0.0_f64);

    event_loop
        .run(|event, elwt: &EventLoopWindowTarget<()>| {
            match event {
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::CursorMoved { position, .. },
                } => {
                    cursor_pos = position;

                    if selection.is_selecting {
                        selection.end = Some(position);
                        if let Some(w) = windows.iter().find(|w| w.id() == window_id) {
                            w.request_redraw();
                        }
                    }
                }

                Event::WindowEvent {
                    window_id,
                    event:
                        WindowEvent::MouseInput {
                            button: MouseButton::Left,
                            state: ElementState::Pressed,
                            ..
                        },
                } => {
                    selection.start = Some(cursor_pos);
                    selection.end = Some(cursor_pos);
                    selection.is_selecting = true;

                    if let Some(w) = windows.iter().find(|w| w.id() == window_id) {
                        w.request_redraw();
                    }
                }

                Event::WindowEvent {
                    window_id,
                    event:
                        WindowEvent::MouseInput {
                            button: MouseButton::Left,
                            state: ElementState::Released,
                            ..
                        },
                } => {
                    selection.is_selecting = false;

                    if let Some((x, y, w, h)) = selection.rect() {
                        let monitor_index = windows
                            .iter()
                            .position(|win| win.id() == window_id)
                            .unwrap_or(0);

                        let monitor_handle = &monitors[monitor_index];
                        let monitor_pos = monitor_handle.position();

                        println!(
                            "Monitor {monitor_index}: pos=({}, {})",
                            monitor_pos.x, monitor_pos.y
                        );
                        println!("Selection (local to monitor): x={x}, y={y}, w={w}, h={h}");

                        for win in &windows {
                            win.set_visible(false);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(200));

                        let xcap_monitors = xcap::Monitor::all().unwrap();
                        let target_monitor = xcap_monitors
                            .iter()
                            .find(|m| m.x() == monitor_pos.x && m.y() == monitor_pos.y);

                        if let Some(monitor) = target_monitor {
                            println!("Capturing monitor: {}", monitor.name());
                            let image = monitor.capture_image().unwrap();
                            let cropped = image::imageops::crop_imm(&image, x, y, w, h).to_image();
                            let mut bytes: Vec<u8> = Vec::new();
                            cropped
                                .write_to(
                                    &mut std::io::Cursor::new(&mut bytes),
                                    image::ImageFormat::Png,
                                )
                                .unwrap();

                            let base64_str =
                                base64::engine::general_purpose::STANDARD.encode(&bytes);

                            let builder = openai::CompletionsClient::<ReqwestClient>::builder()
                                .api_key("")
                                .base_url("http://localhost:1234/v1");

                            let client = builder.build().unwrap();

                            let preamble = "Extract the text from the \
                                        following image and do not translate it.";

                            let agent = client
                                .agent("allenai/olmocr-2-7b")
                                .preamble(preamble)
                                .temperature(0.5)
                                .build();

                            let s = format!("{base64_str}");

                            let image = Image {
                                data: DocumentSourceKind::base64(&s),
                                media_type: Some(ImageMediaType::PNG),
                                detail: Some(ImageDetail::Auto),
                                additional_params: None,
                                ..Default::default()
                            };
                            // Prompt the agent and print the response

                            let response = tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current()
                                    .block_on(async { agent.prompt(image).await.unwrap() })
                            });

                            println!("Response: {:#?}", response);
                        } else {
                            println!("Could not find matching xcap monitor");
                        }
                    }

                    elwt.exit();
                }

                Event::WindowEvent {
                    event: WindowEvent::KeyboardInput { event, .. },
                    ..
                } => {
                    if event.logical_key
                        == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
                    {
                        println!("Cancelled");
                        elwt.exit();
                    }
                }

                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::RedrawRequested,
                } => {
                    if let Some(idx) = windows.iter().position(|w| w.id() == window_id) {
                        let (_, surface) = &mut surfaces[idx];
                        let size = windows[idx].inner_size();
                        let (width, height) = (size.width, size.height);

                        surface
                            .resize(
                                std::num::NonZeroU32::new(width).unwrap(),
                                std::num::NonZeroU32::new(height).unwrap(),
                            )
                            .unwrap();

                        let mut buffer = surface.buffer_mut().unwrap();

                        draw(&mut buffer, width, height, &selection);

                        buffer.present().unwrap();
                    }
                }

                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    elwt.exit();
                }

                _ => {}
            }
        })
        .unwrap();

    Ok(())
}
