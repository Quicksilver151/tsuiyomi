# Tsuiyomi

A terminal UI for managing and reading a **local manga library**, built for libraries
laid out in [Mihon](https://mihon.app/)'s Local source format. The same files work in
both apps.

> **Status:** early prototype, pre-release. The current code still reflects the original
> anime-browsing prototype; the manga-first rewrite is in progress.

## Goals

- Browse a local manga library from the terminal.
- Read chapters from directories and `zip`/`cbz`, `rar`/`cbr`, `7z`/`cb7`, `tar`/`cbt`,
  and image-based EPUB archives.
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

## Metadata

Series metadata follows Mihon: `ComicInfo.xml` wins, then legacy `details.json`, then the
directory name. The parser reads series title, writer, artist credits, summary, genre,
tags, categories, and publishing status. Chapter-level `ComicInfo.xml` is read from
chapter directories and `zip`/`cbz`, `rar`/`cbr`, `7z`/`cb7`, and `tar`/`cbt` archives and
can override title, number, scanlator, and date. EPUB chapters use OPF metadata
(`dc:title`, publisher/creator, date) and their spine XHTML pages are scanned for images.
Prose EPUBs are not rendered yet. Malformed metadata becomes a warning and falls back
without hiding the series.

A custom cover is any `cover.*` image in the series directory (`cover.jpg` is the usual
name). It is shown beside the details pane when the terminal is wide enough. If no valid
cover exists, scanning generates `cover.jpg` from the first image of the first chapter,
like Mihon. Generated covers are capped at 512 px on the longest edge; existing valid
covers are never replaced.

## Reader mode

The default is `webtoon` (continuous vertical scroll). Set it in
`~/.config/tsuiyomi/config.toml`:

```toml
reading_mode = "webtoon"
reader_zoom = 100
scroll_prefetch_behind = 2
scroll_prefetch_ahead = 2
scroll_cache_margin = 4
theme = "catppuccin_mocha"
# accent = "#b4befe"   # optional palette color or hex override
fullscreen = false      # start with the title and commands boxes hidden
library_sort = "natural" # natural, title_asc, title_desc
# default_category = "Reading"
incognito = false
```

Library sort, default category, and Incognito mode live under
**More → Settings → Library** and **Security**. Incognito stops writing
`progress.json` while leaving existing progress readable.

Reading mode, zoom, and scroll cache settings can also be changed in
**More → Settings → Reader** using the thin sliders. Valid `reading_mode` values are `default`,
`left_to_right`, `right_to_left`, `vertical`, `webtoon`, and `continuous_vertical`.
Page-turn modes open single pages; `webtoon` and `continuous_vertical` open in scroll
mode. `m` switches between page and scroll for the current session. Scroll mode fits
pages to the terminal width without upscaling by default; `+`/`-` adjusts zoom from 50%
to 300%. Only pages near the viewport are decoded and cached, and page preparation runs
off the UI thread.

- `↑↓` selects sections/settings; `Enter` enters the selected More section, opens a
  settings page, or activates a setting; `Esc` backs out one level; `←→` adjusts the
  selected setting. Clicking a section or slider also works.

## More and settings

The More tab follows Mihon's structure: Settings, Downloads, Categories, Statistics,
Data and storage, About, and Help. Settings is a submenu mirroring Mihon's settings
categories:

- **Appearance** — theme, accent, fullscreen.
- **Library** — library path, rescan, sort order, default category.
- **Reader** — reading mode, zoom, scroll prefetch and cache.
- **Downloads** — informational; this build reads a local library directly.
- **Tracking** — informational; no remote tracking services.
- **Browse** — local source path; Mihon extensions are Android-only.
- **Data and storage** — storage locations and backup status.
- **Security** — Incognito mode, which stops writing reading progress.
- **Advanced** — logging and diagnostic notes.
- **About** — version, graphics protocol, and reader state.

Downloads, tracking, and remote browsing are not implemented. Their pages say so
instead of pretending to work.

## Themes

**More → Settings → Appearance** switches between Catppuccin (Mocha, Macchiato, Frappe,
Latte), Nord, Solarized (Dark, Light), Gruvbox (Dark, Light), Material Dark, Dracula,
One Dark, and Tokyo Night. `theme` stores the same names in snake_case.

The accent can be a palette color (`default`, `blue`, `teal`, `green`, `yellow`, `peach`,
`mauve`) or a custom hex value (`#rgb` or `#rrggbb`). Press `Enter` on Theme or Accent to
open the picker, use `↑↓` to select, and `Enter` to apply. Selections preview live while
you move; `Esc` restores the saved theme. Each theme row shows the same set of palette
swatches on the right. In the accent picker, typing a hex digit jumps to the custom field;
`Backspace` deletes. Themes and accents persist to `config.toml`.

**Fullscreen** (`config.toml` key `fullscreen`, or the setting under Appearance) starts
the app with the title and commands boxes hidden. `f` toggles it for the current session
without changing the saved default.

## Roadmap

See [TODO.md](TODO.md) for the phased, step-by-step plan. Short version:

- [x] Split the backend into a UI-agnostic core crate and a TUI crate.
- [x] Scan a Mihon `local/` library and list chapters.
- [x] Discover chapter pages in folders and `cbz`/`zip`, `cbr`/`rar`, `cb7`/`7z`,
      `cbt`/`tar`, and comic EPUB archives.
- [x] Parse series and chapter `ComicInfo.xml`, with legacy `details.json`/`.desc`
      fallback.
- [x] Detect `cover.*` images and show them in the details pane.
- [x] Terminal reader using Kitty/Sixel/iTerm2 graphics with a halfblocks fallback.
- [x] Page-turn and continuous vertical scroll modes (`m` toggles).
- [x] Reader bottom bar: previous/next chapter, page gauge, and mode toggle; tabs hidden.
- [x] Remember the last-read page and resume when a chapter is reopened.
- [ ] Import and export Mihon `.tachibk` backups.
- [ ] Local update tracking, title search, and background scanning.
- [ ] Publish to `crates.io`, release `v1.0.0`.
