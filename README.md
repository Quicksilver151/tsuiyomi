# Tsuiyomi

A terminal UI for managing and reading a **local manga library**, built for libraries
laid out in [Mihon](https://mihon.app/)'s Local source format. The same files work in
both apps.

> **Status:** early prototype, pre-release. The current code still reflects the original
> anime-browsing prototype; the manga-first rewrite is in progress.

## Goals

- Browse a local manga library from the terminal.
- Read chapters (directories and `cbz`/`zip`/`cbr`/`rar`/`epub` archives).
- Understand Mihon metadata: `ComicInfo.xml`, legacy `details.json`, `cover.jpg`, and
  chapter numbering parsed from folder/file names.
- Import and export Mihon backups (`.tachibk` / `.proto.gz`).
- Keep the backend UI-agnostic so a TUI, GUI, web UI, or CLI can share it.

Anime support from the original prototype is legacy and will be dropped or redesigned.

## Requirements

- NixOS (or any system with Nix) with flakes enabled.
- `direnv` (recommended) or `nix develop`.

## Development

```sh
cp .env.local.example .env.local   # set TSUIYOMI_LIBRARY to your Mihon local/ folder
direnv allow                       # or: nix develop
cargo run
```

Useful commands:

```sh
just run      # or: cargo run
just check
just fmt
just clippy
just test
```

Machine-local paths and log settings live in `.env.local`, which is gitignored. The
devshell sources it automatically.

## Library layout

Tsuiyomi reads the same structure as Mihon's Local source:

```
<storage>/local/<series title>/
├── cover.jpg
├── ComicInfo.xml          # or legacy details.json
├── chapter_1/
│   └── 001.jpg ...
└── chapter_2.cbz
```

See the [Mihon local source guide](https://mihon.app/docs/guides/local-source/) and
[advanced metadata guide](https://mihon.app/docs/guides/local-source/advanced) for the
full rules.

## Roadmap

- [ ] Split the backend into a UI-agnostic core crate and a TUI crate.
- [ ] Scan and cache a Mihon `local/` library.
- [ ] Parse `ComicInfo.xml` and `details.json`.
- [ ] Chapter list per series, with natural sorting and chapter-number parsing.
- [ ] Read chapters in a TUI reader.
- [ ] Import Mihon `.tachibk` backups.
- [ ] Export Mihon-compatible backups.
- [ ] Search and update tracking.
- [ ] Config UI.
- [ ] Remove or redesign the legacy anime code.
- [ ] Publish to `crates.io`, release `v1.0.0`.
