# Tsuiyomi

A terminal UI for managing and reading a **local manga library**, built for libraries
laid out in [Mihon](https://mihon.app/)'s Local source format. The same files work in
both apps.

> **Status:** MVP for local Mihon libraries plus extension-backed remote sources —
> scanning, browsing, metadata, covers, reading, progress, and settings.
> Updates/history and backups are not implemented yet.

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

## Library

The Library tab opens as a cover tile grid, like Mihon. Each tile shows the series cover
(or an initial placeholder), title, and chapter count. Covers are centered without a
border or tile background. `↑↓` moves by grid row, `←→` by one tile, and `Enter` opens
the chapter list. `[`/`]` switch categories and `r` rescans. Tile width is adjustable in
**More → Settings → Appearance** (`thumbnail_width`, 8–28 columns).

## Browse

Browse follows Mihon's source model:

- **Local source** — series directories in the library root.
- **Extension sources** — every source exposed by the extension host, including installed
  Mihon/Tachiyomi extensions.

Controls:

- `↑↓` moves by grid row and `←→` by tile.
- `Enter` opens the selected source, series detail, chapter, or extension action.
- From a detail screen, `Enter` opens the chapter list inside Browse.
- Remote sources open with popular results. `s` searches and `l` loads latest updates.
- Opening a chapter keeps you in the Browse tab; `Esc` walks back through
  reader → detail → results → sources.
- `Esc` or `Backspace` goes back one level.
- `r` refreshes the current list.
- `e` opens Extensions, `a` adds an extension store URL, `t` switches between extensions
  and stores, `u` updates the selected extension, and `d` removes a selected store.

There is no filesystem import browser. To add local manga, place the folder or archive
in the Mihon `local/` directory and rescan.

## Remote sources and extensions

Mihon extension APKs are JVM bytecode. tsuiyomi does not load them itself; it runs a
headless [miwayomi](https://github.com/miwayomi/miwayomi) host on loopback and talks to
its REST API.

The host is started automatically when you first open Browse. It is discovered in this
order:

1. `host_url` in the config (use an already-running server).
2. `host_jar` in the config, or `TSUIYOMI_HOST_JAR` (development override).
3. The bundled runtime extracted to `$XDG_CACHE_HOME/tsuiyomi/host/<version>/`.
4. `TSUIYOMI_JAVA`, `JAVA_HOME`, then `java` on `PATH` for the JVM.

Host data (extensions, converted jars, logs) lives in
`$XDG_DATA_HOME/tsuiyomi/host/`. `host.log` has the JVM output, and the process is
stopped when tsuiyomi exits.

Add the Keiyoushi repository through Browse → Extensions (`e`) → `a`:

```text
https://github.com/keiyoushi/extensions/raw/repo/index.pb
```

Then install extensions; their sources appear in the Browse source list. JSON stores and
protobuf stores such as Keiyoushi's are both supported. `Enter` installs or uninstalls,
`u` reinstalls (updates) the selected extension, `t` switches to the stores list, and `d`
removes a store.

Remote pages are fetched through `read_page` and use the same page/scroll reader and
progress store as local chapters. Remote progress keys are
`remote://<source>/<manga-hash>/<chapter-hash>`.

Sources that need an Android WebView are not supported. Cloudflare-protected sources can
work when an external FlareSolverr is configured for the host.

### Bundling the host

A normal `cargo build` produces a binary without a JVM, which falls back to `java` on
`PATH`. For a genuinely single-file distribution, build the runtime archive first:

```sh
just host-runtime   # pinned miwayomi 0.2.9, patched, + jlink'd JRE 21
just dist           # embeds the archive and builds the release binary
```

`just host-runtime` downloads the pinned host source, applies
`scripts/host-patches/miwayomi-0.2.9.patch`, runs Gradle, and packages the jar with a
`jlink` runtime. It needs a JDK 21 and network access and takes a few minutes. Set
`TSUIYOMI_HOST_JAR_PATH` to reuse an already-built patched jar.

The patch matters: the released host implements the pre-1.6 source API and lacks
`eu.kanade.tachiyomi.AppInfo`, so current Keiyoushi extensions load but fail on details,
chapters, and pages. The patch adds the 1.6 suspend API, `SMangaUpdate`, `AppInfo`, a
page-aware image endpoint for sources such as MangaDex, and disables the host's
self-updater so a bundled runtime cannot be swapped underneath tsuiyomi.

Verify a packaged binary without a terminal:

```sh
TSUIYOMI_HOST_SELFTEST=1 ./target/release/tsuiyomi
```

It extracts the runtime, starts the host, checks `/api/v1/health`, lists sources, and
exits.

The resulting executable is about 135 MB. The archive is embedded at compile time,
extracted once to
`$XDG_CACHE_HOME/tsuiyomi/host/0.2.9/` (about 157 MB on disk), and reused afterwards.
The extracted host runs on a random loopback port and is stopped when tsuiyomi exits.
On NixOS prefer `nix bundle` so the JRE's store paths remain valid.

The archive is not committed (it is over 100 MB). Host source is pinned to commit
`a3a776ebd26f91f558cd59a4bd57fff9119ee987` (v0.2.9).

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
# host_url = "http://127.0.0.1:4567"  # use an external extension host
# host_jar = "/path/to/miwayomi.jar"  # development override
reading_mode = "webtoon"
reader_zoom = 100
scroll_prefetch_behind = 2
scroll_prefetch_ahead = 2
scroll_cache_margin = 4
theme = "catppuccin_mocha"
# accent = "#b4befe"   # optional palette color or hex override
fullscreen = false      # start with the title and commands boxes hidden
thumbnail_width = 14    # cover tile width in columns, 8–28
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

- **Appearance** — theme, accent, fullscreen, thumbnail size.
- **Library** — library path, rescan, sort order, default category.
- **Reader** — reading mode, zoom, scroll prefetch and cache.
- **Downloads** — informational; this build reads a local library directly.
- **Tracking** — informational; no remote tracking services.
- **Browse** — local source path and the extension host (URL or bundled runtime).
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
- [x] Browse the built-in Local source and open its series in Library.
- [x] Browse remote sources and manage Mihon extensions through the bundled JVM host.
- [x] Terminal reader using Kitty/Sixel/iTerm2 graphics with a halfblocks fallback.
- [x] Page-turn and continuous vertical scroll modes (`m` toggles).
- [x] Reader bottom bar: previous/next chapter, page gauge, and mode toggle; tabs hidden.
- [x] Remember the last-read page and resume when a chapter is reopened.
- [ ] Import and export Mihon `.tachibk` backups.
- [ ] Local update tracking, title search, and background scanning.
- [ ] Publish to `crates.io`, release `v1.0.0`.
