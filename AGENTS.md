# AGENTS.md

Instructions for AI agents working on tsuiyomi.

## What this is

tsuiyomi is a terminal UI for managing and reading a **local manga library**. It is
**manga-first** and aims to be **compatible with [Mihon](https://mihon.app/)** (the
successor to Tachiyomi): the library on disk uses the same layout Mihon's Local source
reads, so the exact same files work in both apps.

The anime browsing code currently in `src/` is legacy from the v0.x prototype. Keep it
compiling, but do not extend it. It will be replaced by the manga library.

## Status

Early prototype. No stable API, no stable config format, no release. Large refactors are
expected and welcome when they move the project toward the architecture below.

## Environment

- Target system is **NixOS** with flakes enabled. The devshell pins nixpkgs
  `nixos-unstable` through `flake.lock`.
- The devshell is defined in `flake.nix`, entered via `direnv allow` or `nix develop`.
- Machine-local env vars live in `.env.local` (gitignored, copied from
  `.env.local.example`). `TSUIYOMI_LIBRARY` overrides the library root and `TSUIYOMI_LOG`
  sets the log level. Without an override the root defaults to
  `$XDG_DATA_HOME/tsuiyomi/local` (`~/.local/share/tsuiyomi/local`).
- **Never run Nix build commands** (`nix build`, `nix flake check`, `nix develop -c ...`).
  The user manages the devshell and builds. Dry runs and evaluation are fine
  (`nix-instantiate --parse`, `nix eval` of small pure expressions).
- Never commit `.env.local`, `.direnv/`, or absolute personal paths.
- Commits use the existing global git identity. Do not add co-author tags, "generated
  by" footers, or any AI attribution.

## Build and test

Run these inside the devshell, from the repo root:

```sh
just run             # launch the TUI (cargo run)
cargo check          # fast feedback
cargo fmt            # before committing
cargo clippy         # must pass with -D warnings, see justfile
cargo test           # unit tests, especially parsers
```

`justfile` wraps the common commands; `just` lists them. `./run.sh` is the user-facing
entry point: it uses the active devshell or falls back to `nix develop --command just run`.

## Mihon compatibility contract

This is the core requirement. Do not invent a private on-disk format; read and write the
Mihon Local source layout directly.

```
<storage>/local/<series title>/
├── cover.jpg                 # optional custom cover
├── details.json              # legacy metadata; Mihon converts it to ComicInfo.xml
├── ComicInfo.xml             # preferred series metadata
├── chapter_1/                # one directory = one chapter
│   ├── ComicInfo.xml         # optional chapter metadata
│   └── 001.jpg ...
└── chapter_2.cbz             # one archive = one chapter
```

Rules (verified against upstream Mihon, 2026):

- A **series** is one direct subdirectory of `local/`. Hidden entries (`.nomedia`,
  `.noxml`, `.DS_Store`, ...) are ignored.
- A **chapter** is a subdirectory or a supported archive directly inside the series
  directory: `.zip`/`.cbz`, `.rar`/`.cbr`, `.epub`. One directory or archive is one
  chapter; subdirectories inside archives are ignored.
- Chapter title = directory name or archive file stem. Chapter numbers are parsed from
  that name using Mihon's `ChapterRecognition` semantics (numbers, `v2`, `c001`, ranges,
  specials, etc.).
- Pages are image files inside the chapter, sorted naturally.
- Series metadata: top-level `ComicInfo.xml` wins; otherwise `details.json`
  (`title`, `author`, `artist`, `description`, `genre`, `status`) is the legacy format.
  Per-chapter `ComicInfo.xml` is read from directories and archives and can override
  chapter title, number, scanlator, and date.
- Custom cover is `cover.jpg` in the series directory. If absent, Mihon falls back to the
  first page of a chapter; mirroring that is a nice-to-have, not required.
- Series are shown in a folder even without metadata. Metadata failures must not hide a
  series or abort a scan.
- Hidden directories are skipped; natural, case-insensitive ordering is expected.

Backups:

- Mihon backups are `.tachibk` / `.proto.gz`: optional gzip (magic `1f 8b`) followed by
  protobuf. Protobuf field 1 is a repeated length-delimited `BackupManga`; other fields
  are metadata.
- There is no `.proto` file upstream anymore. The schema lives in the Kotlin
  `@ProtoNumber` classes under
  `app/src/main/java/eu/kanade/tachiyomi/data/backup/models/`.
- Import should be read-only tolerant: unknown fields are skipped, missing fields use
  defaults, malformed files produce an error rather than a panic.
- Export, when added, must produce files Mihon can restore.

References:

- Local source guide: <https://mihon.app/docs/guides/local-source/>
- Advanced local metadata: <https://mihon.app/docs/guides/local-source/advanced>
- Backups: <https://mihon.app/docs/guides/backups>

## Architecture

The backend is **data-driven and UI-agnostic**. It must be usable from a TUI today and a
GUI, web API, or CLI later. Workspace layout:

```
crates/
├── tsuiyomi-core/   # library scan, metadata, chapter parsing, backups, config, logging
├── tsuiyomi-tui/    # ratatui/crossterm frontend; rendering is pure
└── tsuiyomi/        # thin binary that wires core + tui
```

Rules:

- `tsuiyomi-core` has **no** dependency on `ratatui`, `crossterm`, or any UI crate.
- Core exposes plain data types plus commands/events (or an equivalent request/response
  API). UI code never touches the filesystem directly.
- Input handling and state updates are separate from rendering. Rendering must not mutate
  application data; view state such as `ListState` scroll offset is allowed.
- Paths come from configuration/env (`TSUIYOMI_LIBRARY`), never hardcoded. Missing or
  unreadable directories are reported, not panicked on.
- Prefer `Result` and typed errors. No `unwrap`/`expect` on user data, filesystem, or
  network input.
- Keep parsing logic (chapter names, `ComicInfo.xml`, `details.json`, backups) in core so
  it can be unit tested without a terminal.

## Current gaps

- Reader renders pages with `ratatui-image`: it auto-detects Kitty/Sixel/iTerm2 and falls
  back to Unicode halfblocks. `rar`/`cbr` and `epub` pages are not supported yet
  (`zip`/`cbz` and folders are). `m` toggles page-turn vs continuous scroll
  (`SlicedProtocol`); progress is stored in `$XDG_DATA_HOME/tsuiyomi/progress.json`.
  Reading direction and fit modes are not configurable yet.
- `ComicInfo.xml` and cover handling are not implemented; `details.json` and `.desc` are.
- Categories come from `categories.json` in the library root (Mihon keeps them in its
  database). Unlisted series fall into `Default`. Replace this with the DB layer later.
- Scanning is synchronous on refresh; move it to a background task.
- No title filter or backup support yet.
- The config loader tolerates the existing on-disk schema but does not use or preserve its
  keys (`theme`, `player`, `manga_viewer`, `tty`, ...) when saving is added.
- Core has tests for chapter parsing, config loading, scanning, categories, page
  discovery, and reading progress. The TUI has TestBackend rendering tests for the library,
  chapter, and reader views.

Do not copy these old patterns (all fixed in 0.2, kept here as a warning):

- Hardcoded personal library paths and `expect` on filesystem access.
- `dbg!` output before entering the alternate screen.
- State mutation inside render functions.
- `pub use` crate prelude plus `use crate::*` everywhere.
- Deriving tab highlighting from string splitting.

## Style

- UI tabs mirror Mihon: Library, Updates, History, Browse, More. No numeric prefixes.
  Selected tabs use lavender text only; do not fill their background.
- Theme: Catppuccin Mocha with lavender as the accent. Square borders, a single-row shared
  tab bar at the bottom, a commands box below it, and colored metadata fields. Fullscreen
  hides the title and commands boxes.
- `rustfmt` defaults.
- New code: explicit imports, `snake_case`, small modules, doc comments on public core
  APIs.
- Prefer boring, testable code over clever abstractions.
- Add tests next to parsers. If a behavior depends on Mihon, cite the upstream file or
  doc in a comment.
- Keep refactors separate from behavior changes in commits.
