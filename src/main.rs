use clap::Parser;
use freya::prelude::*;
use freya::winit;
use std::error::Error;
use text_display_window::TextDisplayWindow;

mod text_display_window;

use tokio::runtime::Builder;

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

    for (i, monitor) in xcap_monitors.into_iter().enumerate() {
        launch_config = launch_config.with_window(
            WindowConfig::new_app(TextDisplayWindow {
                display_screenshot,
                model: model.clone().into(),
                url: url.clone().into(),
                monitor: monitor.clone(),
            })
            .with_transparency(true)
            .with_background(Color::TRANSPARENT)
            .with_decorations(false)
            .with_window_attributes(move |attributes, el| {
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

    Ok(())
}
