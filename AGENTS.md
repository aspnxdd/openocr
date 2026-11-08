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

- Parses CLI arguments via `clap` (`--model`, `--url`, `--display-screenshot`, `--timeout`, `--history`)
- `--history` opens the existing workspace with the newest entry before monitor enumeration; no OCR server or capture is needed
- Normal capture mode enumerates all monitors via `xcap::Monitor::all()`
- Creates a transparent, decoration-less, fullscreen Freya window per monitor
- Each window runs a `TextDisplayWindow` app instance
- Launches the Freya event loop with `launch(launch_config)`

### `ui.rs` (GUI)

- **`colors` module**: Light neutral surfaces, indigo accents, and semantic status colors
- **`ScreenshotSession`**: Shared global `State` for captures, history, selected entry, screenshot visibility, processing status, errors, and result-window identity; takes the history-load result so errors remain visible, and keeps saved text accessible when source images are missing
- **`result_window(session)`**: Shared workspace-window configuration for history startup and completed captures
- **`TextDisplayWindow`**: `App` implementation holding `model`, `url`, `monitor`, request `timeout`, and the shared session
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
- **Tests**: `freya-testing` exercises wide/compact/text-only layouts, missing source images, and empty/failed history startup; pure tests cover capture failure recovery, coordinate conversion, and relative timestamps. Preview images are written to `target/workspace-*.png`.

### `ocr.rs` (LLM interaction)

- **`perform_ocr(url, model, png_bytes, timeout) -> Result<String>`**: async function that encodes the image as base64, builds an OpenAI-compatible client via `rig-core`, sends a Chat Completions request with a system preamble, and returns extracted text; the complete call has a Tokio deadline
- Constants: `PREAMBLE`, `TEMPERATURE`
- Local mock-server tests verify the image request, response text, HTTP errors, and stalled-request timeout

### `history.rs` (persistence)

- **`ScreenshotData`** struct: serializable record with `screenshot_path`, `created_at`, `response`
- **`save_screenshot(png_bytes, response)`**: called via `tokio::task::spawn_blocking` without a session guard; locks a separate `~/.openocr/history.lock`, writes and syncs the PNG to `~/.openocr/screenshots/`, then atomically replaces a synced JSON index with the new entry first; ordinary failures clean up owned files. New image/index files start with Unix mode `0600` (existing screenshots are unchanged)
- **`get_history()`**: reads and deserializes history; only a missing file is an empty library, while read/parse failures are reported without resetting the file
- Tests use private path-taking helpers and UUID temporary directories, never process-global `HOME` changes; cover round trips, failed writes, invalid history, and concurrent writers

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
| `freya`     | GUI framework (Skia + winit), pinned exactly to `0.5.0`                           |
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
- **OCR timeout**: `120` seconds (positive integer; configurable with `--timeout`)
- **History-only mode**: `false`

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

- **Freya versioning**: keep `Cargo.lock` checked in and use `--locked` for reproducible builds.
- **Skia build dependencies**: `bindgen 0.73.2` needs the locked `prettyplease 0.3.0` to match its `syn 3` AST types.
- **Flex layout**: rows using `Size::flex()` children need `Content::Flex`; `.expanded()` sets `Size::Fill` on both axes, not flex:1.
- **Hardware validation**: headless tests do not exercise native window startup, real monitor capture, desktop clipboard permissions, or an actual OCR server.
- **Persistence limits**: file syncing and atomic index replacement do not guarantee full power-loss durability; an interrupted save may leave an orphan PNG. Corrupt history must be backed up and repaired rather than silently reset.
- **Monitor setup** still uses `.unwrap()` during enumeration and fullscreen-window configuration.

## Conventions

- **UI copy**: Keep text concise, factual, and functional (labels, statuses, errors, and actionable instructions). Do not add marketing slogans, motivational taglines, filler sentences, or promotional helper cards. Use layout and styling—not marketing copy—to make the app feel modern.
- **Rust edition 2024**
- **No doc comments** (`///`) on public items currently
- **Freya element builder pattern**: UI is built with chained method calls on element builders (`rect().padding().spacing().child(...)`)
- **Freya state hooks**: Reactive state via `use_state()`, similar to React hooks
- **Formatting**: `cargo fmt` + `taplo fmt` (via `just f`)
