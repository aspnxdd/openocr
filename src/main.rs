use clap::Parser;
use handlers::*;
use ocr_window::OcrWindow;
use selection::Selection;
use softbuffer::{Context, Surface};
use std::{collections::HashMap, error::Error, rc::Rc};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{EventLoop, EventLoopWindowTarget},
    window::{WindowBuilder, WindowId, WindowLevel},
};
mod handlers;
mod ocr_window;
mod selection;
mod text_display_window;

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
                match event {
                    WindowEvent::CursorMoved { position, .. } => {
                        let OcrWindow {
                            selection, window, ..
                        } = windows.get_mut(&window_id).unwrap();
                        handle_cursor_moved(&mut cursor_pos, selection, window, position);
                    }

                    WindowEvent::MouseInput {
                        button: MouseButton::Left,
                        state: ElementState::Pressed,
                        ..
                    } => {
                        let OcrWindow {
                            selection, window, ..
                        } = windows.get_mut(&window_id).unwrap();
                        handle_mouse_input_pressed(&mut cursor_pos, selection, window);
                    }

                    WindowEvent::MouseInput {
                        button: MouseButton::Left,
                        state: ElementState::Released,
                        ..
                    } => {
                        handle_mouse_input_released(
                            &mut windows,
                            window_id,
                            elwt,
                            url.clone(),
                            model.clone(),
                            display_screenshot,
                        );
                    }

                    WindowEvent::KeyboardInput { event, .. } => {
                        handle_keyboard_input(event, elwt);
                    }

                    WindowEvent::RedrawRequested => {
                        handle_redraw_request(&mut windows, window_id);
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
