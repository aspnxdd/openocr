# AGENTS.md

## Project Overview

**openocr** is a Rust desktop application that performs screen-region OCR using a local LLM. The user selects a region of their screen via a transparent overlay, the app captures and crops that region, sends it to an OpenAI-compatible API (typically LM Studio running locally), and displays the extracted text in a new window with a copy-to-clipboard button. The GUI is built with [Freya](https://github.com/marc2332/freya), a Rust UI framework on top of Skia and winit.

## Architecture

The codebase is split across 4 source files, each with a single responsibility:

```
src/
  main.rs      # Entry point: CLI parsing, monitor enumeration, window launch
  ui.rs        # All UI code: color palette, layout helpers, overlay selection,
               #   result display window
  ocr.rs       # LLM interaction: client construction, image prompt, response
  history.rs   # Persistence: screenshot data model, save/load history
```

### `main.rs` (entry point)

- Parses CLI arguments via `clap` (`--model`, `--url`, `--display-screenshot`)
- Enumerates all monitors via `xcap::Monitor::all()`
- Creates a transparent, decoration-less, fullscreen Freya window per monitor
- Each window runs a `TextDisplayWindow` app instance
- Launches the Freya event loop with `launch(launch_config)`

### `ui.rs` (GUI)

- **`colors` module**: Tailwind CSS v4-based color palette constants (backgrounds, borders, text, accents)
- **`ExpandedXY` trait**: Layout helper for expanding width/height on `Rect` and `Label` elements
- **`TextDisplayWindow`** struct holds config: `model`, `url`, `display_screenshot`, `monitor`
  - Implements `freya::prelude::App` trait; the `render()` method returns the overlay element tree
  - **State hooks** (React-like): `use_state()` for cursor positions (`start`, `end`), `should_capture`, `is_opened`, `img_bytes`, `text`
  - **Overlay**: Semi-transparent dark rect covering the full screen
  - **Selection**: Red semi-transparent rectangle drawn between mouse-down and mouse-up positions
  - **Capture flow** (on mouse-up, if region > 10x10 px): captures monitor, crops to selection, encodes as PNG, calls `ocr::perform_ocr()`, saves via `history::save_screenshot()`
- **`result_window`** function launched via `Platform::get().launch_window()`:
  - Sidebar with scrollable history thumbnails
  - `ImageViewer` showing the screenshot
  - Selectable text area with extracted text
  - Copy-to-clipboard button with feedback
- **Reusable helpers**: `section_header()` (icon + label, used by both panels) and `panel_shadow()` (shared shadow style)
- **ESC key** exits the application via `std::process::exit(0)`

### `ocr.rs` (LLM interaction)

- **`perform_ocr(url, model, png_bytes) -> Result<String>`**: async function that encodes the image as base64, builds an OpenAI-compatible client via `rig-core`, constructs an agent with a system preamble, sends the image prompt, and returns the extracted text
- Constants: `PREAMBLE`, `TEMPERATURE`

### `history.rs` (persistence)

- **`ScreenshotData`** struct: serializable record with `screenshot_path`, `created_at`, `response`
- **`save_screenshot(image, response)`**: saves the image as PNG to `~/.openocr/screenshots/` and appends an entry to `~/.openocr/history.json`
- **`get_history()`**: reads and deserializes the history JSON file

## Application Flow

```
CLI args -> enumerate monitors -> create fullscreen overlay per monitor
  -> user drags to select region -> capture monitor screenshot -> crop region
  -> encode PNG -> base64 -> send to LLM (OpenAI-compatible API)
  -> receive extracted text -> open result window with text + screenshot
  -> user can copy text to clipboard
```

## Key Dependencies

| Crate       | Purpose                                                                          |
| ----------- | -------------------------------------------------------------------------------- |
| `freya`     | GUI framework (Skia + winit). Pinned to a specific git rev                      |
| `xcap`      | Cross-platform screen capture and monitor enumeration                            |
| `rig-core`  | LLM client framework; used for the OpenAI-compatible API call                    |
| `image`     | Image cropping (crop captured screenshot to selected region)                     |
| `base64`    | Encode screenshot bytes for the LLM image prompt                                 |
| `clap`      | CLI argument parsing (derive mode)                                               |
| `tokio`     | Async runtime for spawning OCR API calls                                         |
| `reqwest`   | HTTP client (used by rig-core under the hood)                                    |
| `winit`     | Window management (re-exported through Freya, also used directly for fullscreen) |
| `ollama-rs` | **Unused** -- legacy dependency, not imported anywhere in the code               |

## Defaults

- **Model**: `allenai/olmocr-2-7b`
- **API URL**: `http://localhost:1234/v1` (LM Studio default)
- **API key**: empty string (LM Studio does not require one)
- **Temperature**: `0.5`
- **Display screenshot**: `true`

## Build and Run

```sh
# Build
cargo build --release

# Run with defaults (requires LM Studio running locally)
cargo run --release

# Run with custom model and URL
cargo run --release -- --model "my-model" --url "http://localhost:8080/v1"

# Format code
just f
```

## Known Issues and Quirks

- **Freya is pinned to a specific git rev** (`f33c70c`), not a released version. This may break if the upstream repo changes.
- **`ollama-rs`** is listed in `Cargo.toml` but is not used in any source file. It is a leftover from earlier development.
- **No tests** exist in the project.
- **`std::process::exit(0)`** is used for ESC key handling -- this is abrupt and skips any cleanup.
- **Error handling** uses `.unwrap()` in several places (monitor capture, image encoding, API client building, API response).
- **Tracing** setup is commented out in `main.rs`.

## Conventions

- **Rust edition 2024**
- **No doc comments** (`///`) on public items currently
- **Freya element builder pattern**: UI is built with chained method calls on element builders (`rect().padding().spacing().child(...)`)
- **Freya state hooks**: Reactive state via `use_state()`, similar to React hooks
- **Formatting**: `cargo fmt` + `taplo fmt` (via `just f`)
