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
  `.env.local.example`). `TSUIYOMI_LIBRARY` points at the Mihon `local` directory and
  `TSUIYOMI_LOG` sets the log level.
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

`justfile` wraps the common commands; `just` lists them.

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
GUI, web API, or CLI later. Planned workspace layout:

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
  application state.
- Paths come from configuration/env (`TSUIYOMI_LIBRARY`), never hardcoded. Missing or
  unreadable directories are reported, not panicked on.
- Prefer `Result` and typed errors. No `unwrap`/`expect` on user data, filesystem, or
  network input.
- Keep parsing logic (chapter names, `ComicInfo.xml`, `details.json`, backups) in core so
  it can be unit tested without a terminal.
- The current `src/` tree is a single crate. Split it when starting real feature work;
  do not half-migrate. Until then, keep it compiling.

## Current gaps

- The library browser still uses the legacy "Anime" naming and reads plain directories plus
  `.desc` files. Replace it with the Mihon series/chapter model rather than extending it.
- No tests yet.
- The `src/` tree is still a single crate. Split it before real feature work.

Do not copy these old patterns (all fixed in 0.2, kept here as a warning):

- Hardcoded personal library paths and `expect` on filesystem access.
- `dbg!` output before entering the alternate screen.
- State mutation inside render functions.
- `pub use` crate prelude plus `use crate::*` everywhere.
- Deriving tab highlighting from string splitting.

## Style

- `rustfmt` defaults.
- New code: explicit imports, `snake_case`, small modules, doc comments on public core
  APIs.
- Prefer boring, testable code over clever abstractions.
- Add tests next to parsers. If a behavior depends on Mihon, cite the upstream file or
  doc in a comment.
- Keep refactors separate from behavior changes in commits.
