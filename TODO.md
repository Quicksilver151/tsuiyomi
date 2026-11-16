# Development plan

Step-by-step roadmap for turning the 0.2 prototype into a usable manga library app.

Principles:

- Manga first, Mihon-compatible on disk.
- `tsuiyomi-core` is UI-agnostic; `tsuiyomi-tui` only renders and handles input.
- Every phase ends with `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test` green.
- Small commits; refactors separate from behavior changes.

---

## Phase 0 — Foundation (done)

- [x] Nix devshell with direnv and `.env.local`.
- [x] `justfile` with run/check/fmt/clippy/test.
- [x] Dependency upgrade (`ratatui 0.30`, `crossterm 0.29`, `confy 2.0`).
- [x] Remove dead modules and warnings.
- [x] Document the Mihon compatibility contract in `AGENTS.md`.

## Phase 1 — Workspace split

- [x] Convert the root `Cargo.toml` into a workspace (`resolver = "2"`).
- [x] Create `crates/tsuiyomi-core` (library scan, metadata, config, errors).
- [x] Create `crates/tsuiyomi-tui` (ratatui widgets, app state, input).
- [x] Create `crates/tsuiyomi` (thin binary that wires core + tui).
- [x] Move `usr::Config` and `utils::files` into core; move `ui` into tui.
- [x] Rename the legacy `Anime`/`anime` identifiers to `Series`/`library`.
- [x] Keep behavior identical; only move code.

**Done when:** `cargo run` shows the same list, `tsuiyomi-core` has no ratatui/crossterm
dependency, and no `anime` identifiers remain.

## Phase 2 — Domain model and scanner

- [x] Define core types: `Library`, `Series`, `Chapter`, `ChapterKind`
      (directory/archive).
- [x] Add `Page` and page discovery for folders and `zip`/`cbz` archives.
- [ ] Add `SeriesId`/`ChapterId` when the reader lands.
- [x] Scan `local/`: one series per direct subdirectory, skip hidden entries, sort
      case-insensitively and naturally.
- [x] Discover chapters: subdirectories plus `.zip`/`.cbz`/`.rar`/`.cbr`/`.epub`.
- [x] Derive chapter titles from directory names or archive stems.
- [x] Parse chapter numbers from names (numbers, `c001`, `v2`, decimals, ranges,
      specials). Put this in its own module with heavy tests.
- [x] Discover pages in a directory (image extensions, natural sort).
- [x] List pages inside `zip`/`cbz` archives (ignore archive-internal directories).
- [x] Collect per-series errors instead of failing the whole scan.
- [x] Add fixture-based tests using `tempfile`.

**Done when:** scanning a fixture library returns the expected series, chapters, numbers,
and page order, and a broken folder does not abort the scan.

## Phase 3 — Metadata

- [x] Parse legacy `details.json` (`title`, `author`, `artist`, `description`, `genre`,
      `status`).
- [ ] Parse series-level `ComicInfo.xml` with `quick-xml`.
- [ ] Parse chapter-level `ComicInfo.xml` from folders and archives (title, number,
      scanlator, date).
- [ ] Implement precedence: `ComicInfo.xml` > `details.json` > directory name.
- [ ] Detect `cover.jpg`; expose a `CoverRef` without loading image data.
- [ ] Treat malformed metadata as a warning attached to the series, not a failure.

**Done when:** metadata fixtures produce the same values Mihon would show, and malformed
XML/JSON leaves the series usable.

## Phase 4 — Library UI

- [x] Replace the anime panel with a series list + detail pane (title, author, tags,
      description, chapter count).
- [x] Add a status bar (library path, counts, errors, key hints).
- [ ] Move scanning to a background thread; post results through a channel as app events.
- [x] Show loading, empty, and error states.
- [x] Add refresh (`r`); the incremental title filter (`/`) is still missing.
- [x] Add `c` config shortcut and `f` fullscreen (hides title and commands boxes).
- [x] Keep selection stable across refresh.
- [x] Add a category tab row inside Library, navigated with `[` and `]`.
- [x] Add mouse support: click tabs, click a series, scroll the list.
- [x] Add TestBackend rendering tests for loaded and empty/error states.

**Done when:** the TUI is responsive while scanning and handles empty or broken libraries
without panics.

## Phase 5 — Reader

- [x] Chapter list view per series, sorted by parsed chapter number.
- [x] Open a series with Enter, open a chapter, back out with Esc.
- [x] Open a chapter and build the page list from core.
- [x] Render image pages with `ratatui-image` (Kitty/Sixel/iTerm2, halfblocks fallback).
- [x] Page navigation with `←`/`→`, `PageUp`/`PageDown`, and `Space`.
- [ ] Next/previous chapter from inside the reader.
- [ ] Fit modes and reading direction (LTR/RTL) in config.
- [ ] Persist last-read page per chapter after a short debounce.
- [x] Handle unsupported or corrupt images without aborting.

**Done when:** a chapter can be read end-to-end, progress survives restart, and image
protocol fallback works over SSH.

## Phase 6 — Config and history

- [ ] Define the config schema (library path, reader, theme, log level) and store it under
      the XDG config dir.
- [ ] Preserve the config keys already present on user machines (`theme`,
      `max_http_requests_per_sec`, `player.*`, `manga_viewer`, `tty.graphics`) and map them
      to the reader/player settings. Never drop unknown keys on save.
- [ ] Add CLI flags with `clap`: `--library`, `--config`, `--version`.
- [ ] Precedence: CLI > env (`TSUIYOMI_LIBRARY`) > config file > default.
- [ ] Add an editable config tab with validation.
- [ ] Store read history/progress under the XDG data dir.
- [ ] Start with a JSON store; migrate to SQLite only if it becomes slow.
- [ ] Add config/history migration handling and corrupt-file recovery.

**Done when:** settings and progress survive restarts, and a corrupt config falls back to
defaults with a visible warning.

## Phase 7 — Mihon backups

- [ ] Define protobuf structs with `prost` matching Mihon's `@ProtoNumber` models.
- [ ] Detect optional gzip (magic `1f 8b`) and stream `BackupManga` entries.
- [ ] Import: match series by title, chapters by number/title; import read state and
      history.
- [ ] Report unmatched series/chapters instead of silently dropping them.
- [ ] Export: produce `.tachibk` files Mihon can restore.
- [ ] Add round-trip tests and a real-backup fixture (redacted).

**Done when:** a backup exported by Mihon imports with correct progress, and an exported
backup restores in Mihon.

## Phase 8 — Updates and remote sources

- [ ] Local updates: rescan the library, detect new chapters, mark them unread.
- [ ] Updates tab lists new chapters since the last scan.
- [ ] Decide the remote-source strategy. Mihon extensions are Android/Kotlin APKs and
      cannot run here, so this needs independent source implementations or a companion
      bridge — explicitly out of scope until designed.
- [ ] Only add search/remote browsing after that decision.

**Done when:** new local chapters show up in the Updates tab without rescanning manually.

## Phase 9 — Polish and release

- [ ] Structured logging with `tracing`, honoring `TSUIYOMI_LOG`.
- [ ] Friendly error dialogs and a panic hook that restores the terminal.
- [ ] README with screenshots/asciinema, install instructions, and a support matrix.
- [ ] Add a license.
- [ ] CI: `fmt --check`, `clippy -D warnings`, tests, and `nix flake check`.
- [ ] Add a `packages.default` output to the flake.
- [ ] Publish `tsuiyomi-core` and the binary to `crates.io`.
- [ ] Tag `v1.0.0`.

---

## Suggested dependency additions

Add these only in the phase that needs them:

| Phase | Crate |
| --- | --- |
| 1 | `thiserror` (core errors) |
| 2 | `natord` or `alphanumeric-sort`, `walkdir`, `zip` |
| 3 | `quick-xml`, `serde_json` |
| 5 | `ratatui-image`, `image` |
| 6 | `directories`, `clap`, `toml` |
| 7 | `prost`, `flate2` |
| 8 | source-specific HTTP/HTML crates |
| 9 | `tracing`, `tracing-subscriber` |
