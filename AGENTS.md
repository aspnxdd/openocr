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

- **`colors` module**: Light neutral surfaces, indigo accents, and semantic status colors
- **`ScreenshotSession`**: Shared global `State` for captures, history, selected entry, screenshot visibility, processing status, errors, and result-window identity
- **`TextDisplayWindow`**: `App` implementation holding `model`, `url`, `monitor`, and the shared session
  - Local `use_state()` hooks track drag endpoints and whether a selection is active
  - Guided translucent overlay with an unobstructed indigo selection, reading feedback, and errors
  - Before capture, all overlays paint transparent; `capture_bounds()` converts logical coordinates to image pixels and clips them to the monitor
  - Calls `ocr::perform_ocr()`, saves history, then focuses or creates one result window via a renderer callback
- **`ResultWindow`**: Stateful `Component` using a customized light Freya theme
  - Keyed, virtualized history cards with thumbnails, text previews, timestamps, and selection styling
  - Source and text cards arranged side by side, stacked below 1050 logical pixels
  - Screenshot visibility is respected for fresh captures and history entries
  - Selectable text, word/character counts, and copy-to-clipboard feedback
- **Reusable helpers**: `icon()`, `badge()`, `panel()`, and `panel_header()`
- **ESC key** requests graceful event-loop exit
- **Tests**: `freya-testing` exercises wide/compact/text-only layouts and history loading; pure tests cover coordinate conversion and relative timestamps. Preview images are written to `target/workspace-*.png`.

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
| `freya`     | GUI framework (Skia + winit), pinned exactly to `0.5.0-rc.13`                     |
| `xcap`      | Cross-platform screen capture and monitor enumeration                            |
| `rig-core`  | LLM client framework; used for the OpenAI-compatible API call                    |
| `image`     | Image cropping (crop captured screenshot to selected region)                     |
| `base64`    | Encode screenshot bytes for the LLM image prompt                                 |
| `clap`      | CLI argument parsing (derive mode)                                               |
| `tokio`     | Async runtime for spawning OCR API calls                                         |
| `reqwest`   | HTTP client (used by rig-core under the hood)                                    |
| `winit`     | Window management (re-exported through Freya, also used directly for fullscreen) |
| `freya-testing` | Development-only headless layout, interaction, and rendering tests          |

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

# Test, including headless UI previews
cargo test --locked

# Format code
just f
```

## Known Issues and Quirks

- **Freya is a prerelease**: keep `Cargo.lock` checked in and use `--locked` for reproducible builds.
- **Skia build dependencies**: `bindgen 0.73.2` needs the locked `prettyplease 0.3.0` to match its `syn 3` AST types.
- **Flex layout**: rows using `Size::flex()` children need `Content::Flex`; `.expanded()` sets `Size::Fill` on both axes, not flex:1.
- **Hardware validation**: headless tests do not exercise real monitor capture, desktop clipboard permissions, or an actual OCR server.
- **Monitor setup** still uses `.unwrap()` during enumeration and fullscreen-window configuration.

## Conventions

- **UI copy**: Keep text concise, factual, and functional (labels, statuses, errors, and actionable instructions). Do not add marketing slogans, motivational taglines, filler sentences, or promotional helper cards. Use layout and styling—not marketing copy—to make the app feel modern.
- **Rust edition 2024**
- **No doc comments** (`///`) on public items currently
- **Freya element builder pattern**: UI is built with chained method calls on element builders (`rect().padding().spacing().child(...)`)
- **Freya state hooks**: Reactive state via `use_state()`, similar to React hooks
- **Formatting**: `cargo fmt` + `taplo fmt` (via `just f`)
