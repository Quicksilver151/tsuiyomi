# Tsuiyomi

A terminal UI for managing and reading a **local manga library**, built for libraries
laid out in [Mihon](https://mihon.app/)'s Local source format. The same files work in
both apps.

> **Status:** MVP for local Mihon libraries plus extension-backed remote sources —
> scanning, browsing, metadata, covers, reading, progress, and settings.
> Updates and backups are not implemented yet.

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

Panels with item actions support multi-select: press `Space` on the focused item,
move with the normal keys, and press `Space` again to add or remove items. Selected items
show `✓` and the commands bar shows the count. `Esc` clears the selection before backing
out. Actions such as delete, install/uninstall, pin, update, save to library, download,
and bookmark apply to the whole selection.

The Library tab opens as a cover tile grid, like Mihon. Each tile shows the series cover
(or an initial placeholder), title, and chapter count. Covers are centered without a
border or tile background. `↑↓` moves by grid row, `←→` by one tile, and `Enter` opens
the chapter list. Saved manga from sources open the same details screen as Browse: cover,
source, author/artist, status, tags, description, and chapters. Their covers are cached
in `$XDG_DATA_HOME/tsuiyomi/covers`, so they survive restarts and normal cache cleanup
without the extension host. If a cover file is missing or corrupt, the app starts the
host in the background once to restore it; a local series cover is regenerated on demand
from its first page if the generated file disappears. Browse covers use the same
persistent cache, so once a thumbnail appears in search results or details it is kept
even if the source later omits its URL. Details responses never overwrite a working
thumbnail with an empty one. Details, description, and chapters are cached per story in
`$XDG_DATA_HOME/tsuiyomi/stories`, so reopening a saved story after a restart is
immediate; the first open fetches it, and `r` refreshes it. `[`/`]` switch categories,
`r` rescans, `R` refetches missing covers for saved source manga, and `b` bookmarks the
selected chapter. Missing library covers are also retried automatically when the Library
tab is shown; missing source covers are regenerated or fetched in the background. `x` or `Delete` opens the remove
dialog for the selected series: remove it from the library and, for saved source manga,
optionally delete its downloaded chapters. Removing a local series keeps its files and
hides it from future scans.
Chapter rows start with fixed bookmark and download columns (`★`/`☆`, `⬇`/`↓`); local
chapters use `·` in the download column. Tile width is adjustable in
**More → Settings → Appearance** (`thumbnail_width`, 8–28 columns).

## Browse

Browse follows Mihon's source model with three tabs, switched with `[` and `]`:

- **Sources** — the Local source plus every source exposed by the extension host, grouped
  by last used, pinned, and language. `P` pins the selected source; pinned sources move to
  a `Pinned` group. `o` opens source management, where languages and individual sources
  can be enabled or disabled. `C` opens the selected remote source's preferences when the
  extension exposes them. Local lists only series folders from the configured library
  root, never saved remote manga or downloads. On Local, `s` searches folder titles, `l`
  switches between all and recently modified series, and `v` reverses the title order.
- **Extensions** — installed and available extensions. `/` searches by name or package,
  `i` or `Enter` installs available extensions, `x` or `Delete` uninstalls installed
  ones after a `y` confirmation, `u` updates the selected extension, `U` updates every
  extension with a pending update, `P` pins all sources of the selected extensions, `a`
  adds a store URL, and `t` opens the store list. The Extensions tab shows the pending
  update count, and rows show the installed and available versions. All actions apply to
  a multi-selection where relevant.
- **Migrate** — saved remote manga grouped by source. Open a source, select one or more
  favorites with `Space`, and press `M` to search enabled sources and preview matches.
  `M` migrates matched entries (removing the old favorite) or `C` copies them. Progress,
  bookmarks, history, categories, and reading time move with the matched chapters.

Controls:

- `↑↓` moves through lists; `←→` moves by tile in grids.
- `Enter` opens the selected source, series detail, chapter, or extension action.
- Remote sources open with popular results. `s` searches, `l` loads latest updates when
  the source supports them, and `n` appends the next results page. `z` opens the source's
  filters when it exposes them: checkbox, tri-state, text, select, sort, and grouped
  filters, with Reset and Apply rows. `v` cycles comfortable grid, compact grid, and
  list layouts. Saved results carry a ✓ marker; `h` hides them from the result list.
- Opening a chapter keeps you in the Browse tab; `Esc` walks back through
  reader → detail → results → sources.
- `Esc` or `Backspace` goes back one level.
- `r` refreshes the current list.
- `e` jumps to the Extensions tab; `d` removes a store from the store list.
- `F` adds or removes the open manga from the library. Adding a title that matches a
  saved manga opens a duplicate dialog (`a` add anyway, `Enter` open the existing entry);
  removing a saved result asks for confirmation. New saves use the default category.
- `D` opens the downloads menu for the open remote story: selected, next 1/5/10/25,
  unread, or bookmarked chapters, plus queue cancellation.
- `b` bookmarks or unbookmarks the selected chapter (also available in the reader).
- On a story screen, `f` opens chapter settings (downloaded/unread/bookmarked filters,
  scanlators, sort, direction, display), `s` cycles sort, and `v` cycles title/number
  display. `a`/`I` select all or invert, `m`/`u` mark selected chapters read or unread,
  `U` marks previous chapters read, and `n` continues with the next unread chapter.
- `/` on the Sources tab runs a background global search over enabled pinned sources,
  showing progress and grouping hits by source. Empty and failed sources appear as rows;
  `h` hides them. `A` switches between pinned and all enabled sources.

The Library is one grid: local series and manga saved from sources share it, categories,
sorting, and the reader. New saved manga go to the default category. Saved entries are
mirrored into the config so they appear before the extension host starts.

Local and saved-source series open the same story screen: chapter list on the left,
cover and metadata on the right. Browse uses that screen too. Rows show unread,
in-progress, and read state, chapter dates, scanlators, bookmarks, and download state;
chapter filters, sorting, and display mode persist per series. `x`/Delete removes
selected downloads after confirmation, `c` edits categories for saved source manga,
`M` opens Migrate with the manga selected, `N` edits notes, `C` copies the URL, `o`
opens the local folder or an absolute source URL, and `w`/`W` show the cover or set a
custom local cover. Remote chapters can be downloaded with `D`; they are read from disk
afterwards, even when the source is offline.

There is no filesystem import browser. To add local manga, place the folder or archive
in the Mihon `local/` directory and rescan.

## History

The History tab shows one row per manga: its most recently read chapter. Rows have a
small cover, title, chapter, read time, source, page progress, and accumulated reading
time, grouped by Today / Yesterday / date. `Enter` reopens the current chapter if it is
unfinished, otherwise the next chapter. `o` opens manga details, `F` saves a remote manga
to the library, `x`/`Delete` opens a delete dialog, `X` clears all history after
confirmation, and `/` searches series titles. `Space` multi-selects rows. Deletion is
permanent and clears the matching chapter progress, so chapters appear unread again;
bookmarks and downloads are untouched.

`[` and `]` switch between History and Stats inside the panel. Stats shows total, today,
7-day, and 30-day reading time, a 14-day bar chart, top series, reading streak, and
trend counts. Reading time is accumulated while the reader is open and aggregated from live
history, so removing a history row also removes its time from Stats.

History is stored as an unbounded append-only
`$XDG_DATA_HOME/tsuiyomi/history.jsonl`; the old `history.json` is imported on first
start. Incognito and per-extension Incognito stop new history and time recording.

## Remote sources and extensions

Mihon extension APKs are JVM bytecode. tsuiyomi does not load them itself; it runs a
headless [miwayomi](https://github.com/miwayomi/miwayomi) host on loopback and talks to
its REST API.

The host is started automatically when you first open Browse. It runs only on loopback,
on an ephemeral port, and is stopped when tsuiyomi exits. Its state, data directory, and
log path are listed in **More → About**, where `H` stops or restarts it. It is discovered
in this order:

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
# downloads_path = "/path/to/downloads"  # defaults to the sibling of local/
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

Scroll mode measures page sizes in the background and shows the first page while it
works; remote dimensions are probed with small ranged requests. Set
`TSUIYOMI_DEBUG_SCROLL=1` to write timings to `/tmp/tsuiyomi-scroll.log`.

Reading mode, zoom, and scroll cache settings can also be changed in
**More → Settings → Reader** using the thin sliders. Valid `reading_mode` values are `default`,
`left_to_right`, `right_to_left`, `vertical`, `webtoon`, and `continuous_vertical`.
Page-turn modes open single pages; `webtoon` and `continuous_vertical` open in scroll
mode. `m` switches between page and scroll for the current session. Scroll mode fits
pages to the terminal width without upscaling by default; `+`/`-` adjusts zoom from 50%
to 300%. Zooming rebuilds the scroll layout but keeps the top of the viewport on the same
point in the page, including inside very long webtoon strips. Only pages near the viewport
are decoded and cached, and page preparation runs off the UI thread. `D` downloads the current chapter and `b` toggles its bookmark.

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
- **Downloads** — shows the download folder; `D` downloads the selected remote
  chapter, and downloaded chapters are read from disk.
- **Tracking** — informational; no remote tracking services.
- **Browse** — local source path and the extension host (URL or bundled runtime).
- **Data and storage** — storage locations and backup status.
- **Security** — Incognito mode, which stops writing reading progress.
- **Advanced** — logging and diagnostic notes.
- **About** — version, graphics protocol, and reader state.

Tracking is not implemented; its page says so instead of pretending to work.
Downloaded chapters follow Mihon's layout:
`<downloads>/<source>/<series>/<chapter>/<page>.<ext>`.

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
