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
cp .env.local.example .env.local   # adjust TSUIYOMI_LIBRARY if needed
direnv allow                       # or: nix develop
./run.sh                           # enters the devshell automatically if needed
```

`run.sh` uses the active devshell when it detects one, and otherwise runs
`nix develop --command just run`. Arguments are forwarded to `just run`.

Useful commands (inside the devshell):

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

By default the library root is `$XDG_DATA_HOME/tsuiyomi/local`
(`~/.local/share/tsuiyomi/local`). Override it with `TSUIYOMI_LIBRARY` or
`library_path` in the config. To share a library with Mihon, point it at Mihon's
`local/` folder.

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

### Categories

Mihon keeps categories in its database. Tsuiyomi reads them from a `categories.json`
file in the library root until the database layer exists (see
`categories.example.json`):

```json
{
  "categories": [
    { "name": "Reading", "series": ["One Piece", "Frieren"] },
    { "name": "Plan to Read", "series": ["Berserk"] }
  ]
}
```

Series that are not listed in any category appear in the system `Default` category.
Switch categories with `[` and `]`. `downloads/` and `extensions/` are never treated as
library entries.

## Reader mode

The default is `webtoon` (continuous vertical scroll). Set it in
`~/.config/tsuiyomi/config.toml`:

```toml
reading_mode = "webtoon"
```

Valid values follow Mihon's reading modes: `default`, `left_to_right`,
`right_to_left`, `vertical`, `webtoon`, `continuous_vertical`. Page-turn modes open
single pages; `webtoon` and `continuous_vertical` open in scroll mode. `m` switches
between page and scroll for the current session. Scroll mode fits pages to the terminal
width without upscaling by default; `+`/`-` adjusts zoom from 50% to 300%.

## Roadmap

See [TODO.md](TODO.md) for the phased, step-by-step plan. Short version:

- [x] Split the backend into a UI-agnostic core crate and a TUI crate.
- [x] Scan a Mihon `local/` library and list chapters.
- [x] Discover chapter pages in folders and `cbz`/`zip` archives.
- [ ] Parse `ComicInfo.xml` (legacy `details.json` and `.desc` already work).
- [x] Terminal reader using Kitty/Sixel/iTerm2 graphics with a halfblocks fallback.
- [x] Page-turn and continuous vertical scroll modes (`m` toggles).
- [x] Reader bottom bar: previous/next chapter, page gauge, and mode toggle; tabs hidden.
- [x] Remember the last-read page and resume when a chapter is reopened.
- [ ] Import and export Mihon `.tachibk` backups.
- [ ] Local update tracking, search, and config UI.
- [ ] Publish to `crates.io`, release `v1.0.0`.
