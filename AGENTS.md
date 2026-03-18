# AGENTS.md

## Project Overview

**openocr** is a Rust desktop application that performs screen-region OCR using a local LLM. The user selects a region of their screen via a transparent overlay, the app captures and crops that region, sends it to an OpenAI-compatible API (typically LM Studio running locally), and displays the extracted text in a new window with a copy-to-clipboard button. The GUI is built with [Freya](https://github.com/marc2332/freya), a Rust UI framework on top of Skia and winit.

## Architecture

The codebase is compact (~260 lines across 2 source files) and follows a single-responsibility split:

```
src/
  main.rs                    # Entry point: CLI parsing, monitor enumeration, window launch
  text_display_window.rs     # Core logic: overlay rendering, region selection, screenshot
                             #   capture, LLM OCR request, result display window
```

### `main.rs` (entry point)

- Parses CLI arguments via `clap` (`--model`, `--url`, `--display-screenshot`)
- Enumerates all monitors via `xcap::Monitor::all()`
- Creates a transparent, decoration-less, fullscreen Freya window per monitor
- Each window runs a `TextDisplayWindow` app instance
- Launches the Freya event loop with `launch(launch_config)`

### `text_display_window.rs` (GUI + OCR)

- **`TextDisplayWindow`** struct holds config: `model`, `url`, `display_screenshot`, `monitor`
- Implements `freya::prelude::App` trait; the `render()` method returns the element tree
- **State hooks** (React-like): `use_state()` for cursor positions (`start`, `end`), `should_capture`, `is_opened`, `img_bytes`, `text`
- **Overlay**: Semi-transparent dark rect covering the full screen (`opacity: 0.4`, `background: rgb(25,25,25)`)
- **Selection**: Red semi-transparent rectangle drawn between mouse-down and mouse-up positions
- **Capture flow** (on mouse-up, if region > 10x10 px):
  1. `xcap` captures the full monitor image
  2. `image::imageops::crop_imm` crops to the selected region
  3. Cropped image encoded as PNG then base64
  4. `rig-core`'s `openai::CompletionsClient` sends the image to the LLM API
  5. System preamble: `"Extract the text from the following image and do not translate it."`
  6. Response text stored in state
- **Result window**: `sub_app` function launched via `Platform::get().launch_window()`. Contains:
  - `ImageViewer` showing the screenshot (if `display_screenshot` is true)
  - `label` with the extracted text
  - `Button` with a copy icon to copy text to clipboard
- **ESC key** exits the application via `std::process::exit(0)`

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
| `freya`     | GUI framework (Skia + winit). Pinned to a specific PR: `refs/pull/1655/head`     |
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

- **Freya is pinned to a PR branch** (`refs/pull/1655/head`), not a released version. This may break if the PR is force-pushed or merged.
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
