use base64::Engine;
use ollama_rs::{
    Ollama,
    generation::{completion::request::GenerationRequest, images::Image},
};
use softbuffer::{Context, Surface};
use std::{error::Error, sync::Arc};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{EventLoop, EventLoopWindowTarget},
    window::{WindowBuilder, WindowLevel},
};
mod selection;
use selection::{Selection, draw};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new().unwrap();

    let monitors: Vec<_> = event_loop.available_monitors().collect();

    let windows: Vec<Arc<winit::window::Window>> = monitors
        .iter()
        .map(|monitor| {
            Arc::new(
                WindowBuilder::new()
                    .with_title("Select Region")
                    .with_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(
                        monitor.clone(),
                    ))))
                    .with_window_level(WindowLevel::AlwaysOnTop)
                    .with_decorations(false)
                    .with_transparent(true)
                    .build(&event_loop)
                    .unwrap(),
            )
        })
        .collect();

    let surfaces: Vec<_> = windows
        .iter()
        .map(|window| {
            let context = Context::new(Arc::clone(window)).unwrap();
            let surface = Surface::new(&context, Arc::clone(window)).unwrap();
            (context, surface)
        })
        .collect();

    let surfaces = std::cell::RefCell::new(surfaces);
    let selection = std::cell::RefCell::new(Selection::new());
    let cursor_pos = std::cell::RefCell::new(PhysicalPosition::new(0.0_f64, 0.0_f64));
    let active_window_id = std::cell::RefCell::new(windows[0].id());

    event_loop
        .run(move |event, elwt: &EventLoopWindowTarget<()>| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    match event {
                        Event::WindowEvent {
                            window_id,
                            event: WindowEvent::CursorMoved { position, .. },
                        } => {
                            *cursor_pos.borrow_mut() = position;
                            *active_window_id.borrow_mut() = window_id;

                            if selection.borrow().is_selecting {
                                selection.borrow_mut().end = Some(position);
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
                            let pos = *cursor_pos.borrow();
                            let mut sel = selection.borrow_mut();
                            sel.start = Some(pos);
                            sel.end = Some(pos);
                            sel.is_selecting = true;
                            *active_window_id.borrow_mut() = window_id;

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
                            selection.borrow_mut().is_selecting = false;

                            if let Some((x, y, w, h)) = selection.borrow().rect() {
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
                                    println!("data:image/png;base64,{}", base64_str);
                                    let ollama = Ollama::default();
                                    let request = GenerationRequest::new(
                                        "llama3.2-vision".to_string(),
                                        "Extract text from the image and send it back. Don't translate nor include any other word which is not strictly written in the text of the image"
                                    ).add_image(Image::from_base64(base64_str));
                                    let response = ollama.generate(request).await.unwrap().response;
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
                                let mut surfaces = surfaces.borrow_mut();
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

                                let active_id = *active_window_id.borrow();
                                if window_id == active_id {
                                    draw(&mut buffer, width, height, &selection.borrow());
                                } else {
                                    buffer.fill(0x88000000);
                                }

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
                });
            });
        })
        .unwrap();

    Ok(())
}
