use base64::Engine;
use clap::Parser;
use freya::prelude::*;
use freya::winit;
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
// use winit::{
//     dpi::PhysicalPosition,
//     event::{ElementState, Event, MouseButton, WindowEvent},
//     event_loop::{EventLoop, EventLoopWindowTarget},
//     monitor::MonitorHandle,
//     window::{Window, WindowBuilder, WindowId, WindowLevel},
// };
mod selection;
mod text_display_window;

use tokio::runtime::Builder;

// struct OcrWindow {
//     selection: Selection,
//     monitor: MonitorHandle,
//     #[allow(dead_code)]
//     context: Context<Rc<Window>>,
//     surface: Surface<Rc<Window>, Rc<Window>>,
//     window: Rc<Window>,
// }

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let rt = Builder::new_multi_thread().enable_all().build().unwrap();
    let _rt = rt.enter();
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

    let xcap_monitors = xcap::Monitor::all().unwrap();
    let mut launch_config = LaunchConfig::new();

    // #[derive(Default)]
    // struct BuilderA {
    //     stuff: Vec<i32>,
    // }

    // impl BuilderA {
    //     pub fn with_stuff(mut self, stuff: i32) -> Self {
    //         self.stuff.push(stuff);
    //         self
    //     }

    //     pub fn with_stuff_2(&mut self, stuff: i32) -> &mut Self {
    //         self.stuff.push(stuff);
    //         self
    //     }
    // }

    // let builder = BuilderA::default().with_stuff(123).with_stuff(456);

    // let mut builder2 = BuilderA::default();
    // builder2.with_stuff_2(123);

    // fn launch2(value: BuilderA){

    // }

    // launch2(BuilderA::default());

    // launch2(BuilderA::default().with_stuff(123));

    // launch2(BuilderA::default().with_stuff_2(123));
 
    // print!("{:?}", builder.stuff);

    for (i, monitor) in xcap_monitors.into_iter().enumerate() {
        launch_config = launch_config.with_window(
            WindowConfig::new_app(TextDisplayWindow {
                display_screenshot,
                model: model.clone().into(),
                url: url.clone().into(),
            })
            .with_transparency(true)
            .with_background(Color::TRANSPARENT)
            .with_decorations(false)
            .with_window_attributes(move |mut attributes, el| {
                let monitor = el.available_monitors().nth(i).unwrap();
                attributes
                    .with_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(
                        monitor.clone(),
                    ))))
                    .with_inner_size(monitor.size())
            }),
        )
    }

    launch(launch_config);

    // let event_loop = EventLoop::new().unwrap();

    // let mut windows: HashMap<WindowId, OcrWindow> =
    //     HashMap::from_iter(event_loop.available_monitors().map(|monitor| {
    //         let window = WindowBuilder::new()
    //             .with_title("Select Region")
    //             .with_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(
    //                 monitor.clone(),
    //             ))))
    //             .with_window_level(WindowLevel::AlwaysOnTop)
    //             .with_decorations(false)
    //             .with_transparent(true)
    //             .build(&event_loop)
    //             .unwrap();
    //         let window = Rc::new(window);
    //         let id = window.id();
    //         let context = Context::new(window.clone()).unwrap();
    //         let surface = Surface::new(&context, window.clone()).unwrap();
    //         (
    //             id,
    //             OcrWindow {
    //                 selection: Selection::new(),
    //                 monitor: monitor.clone(),
    //                 context,
    //                 surface,
    //                 window,
    //             },
    //         )
    //     }));

    // let mut cursor_pos = PhysicalPosition::new(0.0_f64, 0.0_f64);

    // event_loop
    //     .run(|event, elwt: &EventLoopWindowTarget<()>| {
    //         if let Event::WindowEvent { window_id, event } = event {
    //             let OcrWindow {
    //                 selection,
    //                 surface,
    //                 window,
    //                 ..
    //             } = windows.get_mut(&window_id).unwrap();
    //             match event {
    //                 WindowEvent::CursorMoved { position, .. } => {
    //                     cursor_pos = position;

    //                     if selection.is_selecting {
    //                         selection.end = Some(position);
    //                         window.request_redraw();
    //                     }
    //                 }

    //                 WindowEvent::MouseInput {
    //                     button: MouseButton::Left,
    //                     state: ElementState::Pressed,
    //                     ..
    //                 } => {
    //                     selection.start = Some(cursor_pos);
    //                     selection.end = Some(cursor_pos);
    //                     selection.is_selecting = true;

    //                     window.request_redraw();
    //                 }

    //                 WindowEvent::MouseInput {
    //                     button: MouseButton::Left,
    //                     state: ElementState::Released,
    //                     ..
    //                 } => {
    //                     selection.is_selecting = false;

    //                     if let Some((x, y, w, h)) = selection.rect() {
    //                         let monitor_handle = &windows[&window_id].monitor;
    //                         let monitor_pos = monitor_handle.position();

    //                         dbg!("Monitor: pos=({}, {})", monitor_pos.x, monitor_pos.y);
    //                         dbg!("Selection (local to monitor): x={x}, y={y}, w={w}, h={h}");

    //                         for win in windows.values() {
    //                             win.window.set_visible(false);
    //                         }
    //                         std::thread::sleep(std::time::Duration::from_millis(200));

    //                         let xcap_monitors = xcap::Monitor::all().unwrap();
    //                         let target_monitor = xcap_monitors
    //                             .iter()
    //                             .find(|m| m.x() == monitor_pos.x && m.y() == monitor_pos.y);

    //                         if let Some(monitor) = target_monitor {
    //                             dbg!("Capturing monitor: {}", monitor.name());
    //                             let image = monitor.capture_image().unwrap();
    //                             let cropped =
    //                                 image::imageops::crop_imm(&image, x, y, w, h).to_image();
    //                             let mut bytes: Vec<u8> = Vec::new();
    //                             cropped
    //                                 .write_to(
    //                                     &mut std::io::Cursor::new(&mut bytes),
    //                                     image::ImageFormat::Png,
    //                                 )
    //                                 .unwrap();

    //                             let base64_str =
    //                                 base64::engine::general_purpose::STANDARD.encode(&bytes);

    //                             let builder = openai::CompletionsClient::<ReqwestClient>::builder()
    //                                 .api_key("")
    //                                 .base_url(&url);

    //                             let client = builder.build().unwrap();

    //                             let preamble = "Extract the text from the \
    //                                 following image and do not translate it.";

    //                             let agent = client
    //                                 .agent(&model)
    //                                 .preamble(preamble)
    //                                 .temperature(0.5)
    //                                 .build();

    //                             let image = Image {
    //                                 data: DocumentSourceKind::base64(&base64_str),
    //                                 media_type: Some(ImageMediaType::PNG),
    //                                 detail: Some(ImageDetail::Auto),
    //                                 additional_params: None,
    //                                 ..Default::default()
    //                             };
    //                             // Prompt the agent and print the response

    //                             let response = tokio::task::block_in_place(|| {
    //                                 tokio::runtime::Handle::current()
    //                                     .block_on(async { agent.prompt(image).await.unwrap() })
    //                             });

    //                             dbg!("Response: {:#?}", &response);

    //                             let img_bytes = if display_screenshot {
    //                                 Some(Bytes::from(bytes))
    //                             } else {
    //                                 None
    //                             };

    //                             launch(LaunchConfig::new().with_window(WindowConfig::new_app(
    //                                 TextDisplayWindow {
    //                                     text: response.into(),
    //                                     img_bytes,
    //                                 },
    //                             )))
    //                         } else {
    //                             dbg!("Could not find matching xcap monitor");
    //                         }
    //                     }

    //                     elwt.exit();
    //                 }

    //                 WindowEvent::KeyboardInput { event, .. } => {
    //                     if event.logical_key
    //                         == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
    //                     {
    //                         dbg!("Cancelled");
    //                         elwt.exit();
    //                     }
    //                 }

    //                 WindowEvent::RedrawRequested => {
    //                     let size = window.inner_size();
    //                     let (width, height) = (size.width, size.height);

    //                     surface
    //                         .resize(
    //                             std::num::NonZeroU32::new(width).unwrap(),
    //                             std::num::NonZeroU32::new(height).unwrap(),
    //                         )
    //                         .unwrap();

    //                     let mut buffer = surface.buffer_mut().unwrap();

    //                     draw(&mut buffer, width, height, selection);

    //                     buffer.present().unwrap();
    //                 }

    //                 WindowEvent::CloseRequested => {
    //                     elwt.exit();
    //                 }

    //                 _ => {}
    //             }
    //         }
    //     })
    //     .unwrap();

    Ok(())
}
