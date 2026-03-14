use base64::Engine;
use freya::prelude::*;
use rig::{
    client::CompletionClient,
    completion::Prompt,
    http_client::ReqwestClient,
    message::{DocumentSourceKind, Image, ImageDetail, ImageMediaType},
    providers::openai,
};
use selection::{Selection, draw};
use softbuffer::{Context, Surface};
use std::{collections::HashMap, error::Error, rc::Rc};
use text_display_window::TextDisplayWindow;
use tracing::debug;
use tracing_subscriber::prelude::*;
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{EventLoop, EventLoopWindowTarget},
    monitor::MonitorHandle,
    window::{Window, WindowBuilder, WindowId, WindowLevel},
};
mod selection;
mod text_display_window;

struct OcrWindow {
    selection: Selection,
    monitor: MonitorHandle,
    #[allow(dead_code)]
    context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    window: Rc<Window>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(debug_assertions)]
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let event_loop = EventLoop::new().unwrap();

    let mut windows: HashMap<WindowId, OcrWindow> =
        HashMap::from_iter(event_loop.available_monitors().map(|monitor| {
            let window = WindowBuilder::new()
                .with_title("Select Region")
                .with_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(
                    monitor.clone(),
                ))))
                .with_window_level(WindowLevel::AlwaysOnTop)
                .with_decorations(false)
                .with_transparent(true)
                .build(&event_loop)
                .unwrap();
            let window = Rc::new(window);
            let id = window.id();
            let context = Context::new(window.clone()).unwrap();
            let surface = Surface::new(&context, window.clone()).unwrap();
            (
                id,
                OcrWindow {
                    selection: Selection::new(),
                    monitor: monitor.clone(),
                    context,
                    surface,
                    window,
                },
            )
        }));

    let mut cursor_pos = PhysicalPosition::new(0.0_f64, 0.0_f64);

    event_loop
        .run(|event, elwt: &EventLoopWindowTarget<()>| {
            if let Event::WindowEvent { window_id, event } = event {
                let OcrWindow {
                    selection,
                    surface,
                    window,
                    ..
                } = windows.get_mut(&window_id).unwrap();
                match event {
                    WindowEvent::CursorMoved { position, .. } => {
                        cursor_pos = position;

                        if selection.is_selecting {
                            selection.end = Some(position);
                            window.request_redraw();
                        }
                    }

                    WindowEvent::MouseInput {
                        button: MouseButton::Left,
                        state: ElementState::Pressed,
                        ..
                    } => {
                        selection.start = Some(cursor_pos);
                        selection.end = Some(cursor_pos);
                        selection.is_selecting = true;

                        window.request_redraw();
                    }

                    WindowEvent::MouseInput {
                        button: MouseButton::Left,
                        state: ElementState::Released,
                        ..
                    } => {
                        selection.is_selecting = false;

                        if let Some((x, y, w, h)) = selection.rect() {
                            let monitor_handle = &windows[&window_id].monitor;
                            let monitor_pos = monitor_handle.position();

                            debug!("Monitor: pos=({}, {})", monitor_pos.x, monitor_pos.y);
                            debug!("Selection (local to monitor): x={x}, y={y}, w={w}, h={h}");

                            for win in windows.values() {
                                win.window.set_visible(false);
                            }
                            std::thread::sleep(std::time::Duration::from_millis(200));

                            let xcap_monitors = xcap::Monitor::all().unwrap();
                            let target_monitor = xcap_monitors
                                .iter()
                                .find(|m| m.x() == monitor_pos.x && m.y() == monitor_pos.y);

                            if let Some(monitor) = target_monitor {
                                debug!("Capturing monitor: {}", monitor.name());
                                let image = monitor.capture_image().unwrap();
                                let cropped =
                                    image::imageops::crop_imm(&image, x, y, w, h).to_image();
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

                                let s = base64_str.to_string();

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

                                debug!("Response: {:#?}", response);

                                launch(LaunchConfig::new().with_window(WindowConfig::new_app(
                                    TextDisplayWindow {
                                        text: response.into(),
                                    },
                                )))
                            } else {
                                debug!("Could not find matching xcap monitor");
                            }
                        }

                        elwt.exit();
                    }

                    WindowEvent::KeyboardInput { event, .. } => {
                        if event.logical_key
                            == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
                        {
                            debug!("Cancelled");
                            elwt.exit();
                        }
                    }

                    WindowEvent::RedrawRequested => {
                        let size = window.inner_size();
                        let (width, height) = (size.width, size.height);

                        surface
                            .resize(
                                std::num::NonZeroU32::new(width).unwrap(),
                                std::num::NonZeroU32::new(height).unwrap(),
                            )
                            .unwrap();

                        let mut buffer = surface.buffer_mut().unwrap();

                        draw(&mut buffer, width, height, selection);

                        buffer.present().unwrap();
                    }

                    WindowEvent::CloseRequested => {
                        elwt.exit();
                    }

                    _ => {}
                }
            }
        })
        .unwrap();

    Ok(())
}
