use base64::Engine;
use clap::Parser;
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
use tracing_subscriber::prelude::*;
use winit::window::{Window, WindowAttributes, WindowId};
use winit::{application::ApplicationHandler, monitor::MonitorHandle};
use winit::{
    dpi::PhysicalPosition,
    event::ElementState,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
};
use winit::{
    event::{MouseButton, WindowEvent},
    window::WindowLevel,
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

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The model to use for OCR (default: allenai/olmocr-2-7b)
    #[arg(short, long)]
    model: Option<String>,

    /// The base URL of the OpenAI-compatible API (default: http://localhost:1234/v1)
    #[arg(short, long)]
    url: Option<String>,

    /// Whether to display the captured screenshot in the result window (default: true)
    #[arg(short, long, default_value_t = true, action = clap::ArgAction::Set)]
    display_screenshot: bool,
}

#[derive(Default)]
struct App {
    windows: Option<HashMap<WindowId, OcrWindow>>,
    model: String,
    url: String,
    display_screenshot: bool,
    cursor_pos: PhysicalPosition<f64>,
}

impl App {
    fn new(model: String, url: String, display_screenshot: bool) -> Self {
        Self {
            windows: None,
            model,
            url,
            display_screenshot,
            cursor_pos: PhysicalPosition::new(0.0, 0.0),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let windows: HashMap<WindowId, OcrWindow> =
            HashMap::from_iter(event_loop.available_monitors().map(|monitor| {
                let window = event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("Select Region")
                            .with_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(
                                monitor.clone(),
                            ))))
                            .with_window_level(WindowLevel::AlwaysOnTop)
                            .with_decorations(false)
                            .with_transparent(true),
                    )
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
        self.windows = Some(windows);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let ocr_window = self.windows.as_mut().unwrap().get_mut(&id).unwrap();
        let selection = &mut ocr_window.selection;

        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = position;

                if selection.is_selecting {
                    selection.end = Some(position);
                    ocr_window.window.request_redraw();
                }
            }

            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            } => {
                selection.start = Some(self.cursor_pos);
                selection.end = Some(self.cursor_pos);
                selection.is_selecting = true;

                ocr_window.window.request_redraw();
            }

            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            } => {
                selection.is_selecting = false;

                if let Some((x, y, w, h)) = selection.rect() {
                    let monitor_handle = ocr_window.monitor.clone();
                    let monitor_pos = monitor_handle.position();

                    dbg!("Monitor: pos=({}, {})", monitor_pos.x, monitor_pos.y);
                    dbg!("Selection (local to monitor): x={x}, y={y}, w={w}, h={h}");

                    for win in self.windows.as_ref().unwrap().values() {
                        win.window.set_visible(false);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(200));

                    let xcap_monitors = xcap::Monitor::all().unwrap();
                    let target_monitor = xcap_monitors
                        .iter()
                        .find(|m| m.x() == monitor_pos.x && m.y() == monitor_pos.y);

                    if let Some(monitor) = target_monitor {
                        dbg!("Capturing monitor: {}", monitor.name());
                        let image = monitor.capture_image().unwrap();
                        let cropped = image::imageops::crop_imm(&image, x, y, w, h).to_image();
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
                            .base_url(&self.url);

                        let client = builder.build().unwrap();

                        let preamble = "Extract the text from the \
                                    following image and do not translate it.";

                        let agent = client
                            .agent(&self.model)
                            .preamble(preamble)
                            .temperature(0.5)
                            .build();

                        let image = Image {
                            data: DocumentSourceKind::base64(&base64_str),
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

                        dbg!("Response: {:#?}", &response);

                        let img_bytes = if self.display_screenshot {
                            Some(Bytes::from(bytes))
                        } else {
                            None
                        };

                        std::thread::spawn(|| {
                            launch(LaunchConfig::new().with_window(WindowConfig::new_app(
                                TextDisplayWindow {
                                    text: response.into(),
                                    img_bytes,
                                },
                            )));
                        });
                    } else {
                        dbg!("Could not find matching xcap monitor");
                    }
                }

                event_loop.exit();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.logical_key
                    == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
                {
                    dbg!("Cancelled");
                    event_loop.exit();
                }
            }

            WindowEvent::RedrawRequested => {
                let size = ocr_window.window.inner_size();
                let (width, height) = (size.width, size.height);

                ocr_window
                    .surface
                    .resize(
                        std::num::NonZeroU32::new(width).unwrap(),
                        std::num::NonZeroU32::new(height).unwrap(),
                    )
                    .unwrap();

                let mut buffer = ocr_window.surface.buffer_mut().unwrap();

                draw(&mut buffer, width, height, selection);

                buffer.present().unwrap();
            }

            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // #[cfg(debug_assertions)]
    // tracing_subscriber::registry()
    //     .with(tracing_subscriber::fmt::layer())
    //     .init();

    let args = Args::parse();

    let mut url = "http://localhost:1234/v1".to_string();
    if let Some(u) = args.url {
        url = u;
    }

    let mut model = "allenai/olmocr-2-7b".to_string();
    if let Some(m) = args.model {
        model = m;
    }

    let display_screenshot = args.display_screenshot;

    dbg!("Using model: {}", &model);
    dbg!("Using API URL: {}", &url);
    dbg!(
        "Display screenshot in result window: {}",
        display_screenshot
    );

    let event_loop = EventLoop::new().unwrap();

    let mut app = App::new(model, url, display_screenshot);
    event_loop.run_app(&mut app)?;

    Ok(())
}
