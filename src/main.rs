use softbuffer::{Context, Surface};
use std::sync::Arc;
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::EventLoop,
    window::{WindowBuilder, WindowLevel},
};

#[derive(Debug)]
struct Selection {
    start: Option<PhysicalPosition<f64>>,
    end: Option<PhysicalPosition<f64>>,
    is_selecting: bool,
}

impl Selection {
    fn new() -> Self {
        Self {
            start: None,
            end: None,
            is_selecting: false,
        }
    }

    fn rect(&self) -> Option<(u32, u32, u32, u32)> {
        match (self.start, self.end) {
            (Some(s), Some(e)) => {
                let x = s.x.min(e.x) as u32;
                let y = s.y.min(e.y) as u32;
                let w = (s.x - e.x).abs() as u32;
                let h = (s.y - e.y).abs() as u32;
                if w == 0 || h == 0 {
                    return None;
                }
                Some((x, y, w, h))
            }
            _ => None,
        }
    }
}

fn draw(buffer: &mut [u32], width: u32, height: u32, selection: &Selection) {
    let overlay_color = 0x88000000u32;
    let border_color = 0xFFFFFF00u32;

    buffer.fill(overlay_color);

    if let Some((sx, sy, sw, sh)) = selection.rect() {
        for y in sy..(sy + sh).min(height) {
            for x in sx..(sx + sw).min(width) {
                buffer[(y * width + x) as usize] = 0x00000000;
            }
        }

        let border = 2u32;
        for y in sy.saturating_sub(border)..=(sy + sh + border).min(height - 1) {
            for x in sx.saturating_sub(border)..=(sx + sw + border).min(width - 1) {
                let on_border = x < sx || x >= sx + sw || y < sy || y >= sy + sh;
                if on_border {
                    buffer[(y * width + x) as usize] = border_color;
                }
            }
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();

    // Get all monitors info before creating windows
    let monitors: Vec<_> = event_loop.available_monitors().collect();

    // Create one overlay window per monitor
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

    // Create softbuffer surface for each window
    let surfaces: Vec<_> = windows
        .iter()
        .map(|window| {
            let context = Context::new(Arc::clone(window)).unwrap();
            let surface = Surface::new(&context, Arc::clone(window)).unwrap();
            (context, surface)
        })
        .collect();

    // Wrap in RefCell so we can mutate inside the closure
    let surfaces = std::cell::RefCell::new(surfaces);
    let selection = std::cell::RefCell::new(Selection::new());
    let cursor_pos = std::cell::RefCell::new(PhysicalPosition::new(0.0_f64, 0.0_f64));

    // Track which window/monitor the cursor is on
    let active_window_id = std::cell::RefCell::new(windows[0].id());

    event_loop
        .run(move |event, elwt| {
            match event {
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::CursorMoved { position, .. },
                } => {
                    *cursor_pos.borrow_mut() = position;
                    *active_window_id.borrow_mut() = window_id;

                    if selection.borrow().is_selecting {
                        selection.borrow_mut().end = Some(position);
                        // Redraw only the active window
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
                        // Find which monitor index this window corresponds to
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

                        // Hide all overlays before capturing
                        for win in &windows {
                            win.set_visible(false);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(200));

                        // Match xcap monitor by position
                        let xcap_monitors = xcap::Monitor::all().unwrap();
                        let target_monitor = xcap_monitors.iter().find(|m| {
                            m.x() == monitor_pos.x && m.y() == monitor_pos.y
                        });

                        if let Some(monitor) = target_monitor {
                            println!("Capturing monitor: {}", monitor.name());
                            let image = monitor.capture_image().unwrap();
                            let cropped =
                                image::imageops::crop_imm(&image, x, y, w, h).to_image();
                            cropped.save("selection.png").unwrap();
                            println!("Saved selection.png");
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

                        // Only draw selection on the active monitor window
                        let active_id = *active_window_id.borrow();
                        if window_id == active_id {
                            draw(&mut buffer, width, height, &selection.borrow());
                        } else {
                            // Other monitors just get the dark overlay
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

                // Event::AboutToWait => {
                //     for win in &windows {
                //         win.request_redraw();
                //     }
                // }

                _ => {}
            }
        })
        .unwrap();
}
