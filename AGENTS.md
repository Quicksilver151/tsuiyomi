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

All parity feature tables live in `docs/roadmap.md` under `## Parity detail`; category
plans live under `docs/parity/`. Update the affected roadmap table whenever behavior
changes or work lands, then run `python3 scripts/parity-score.py` to refresh the
aggregate tables and percentages.

Local-library and extension-host MVP: scanning, browsing, metadata, covers, reading,
progress, settings, remote sources, extension management, and reading history work.
Updates track new local chapters between scans, and Mihon backups import/export. Exported
backups still need verification against a real Mihon restore. The API and config format
are still unstable, and large refactors are welcome when they move the project toward the
architecture below.

## Environment

- Target system is **NixOS** with flakes enabled. The devshell pins nixpkgs
  `nixos-unstable` through `flake.lock`.
- The devshell is defined in `flake.nix`, entered via `direnv allow` or `nix develop`.
  It includes `gcc` because `unrar_sys` compiles bundled UnRAR C++ sources.
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

`just host-runtime` builds the extension-host archive (`miwayomi` patched + `jlink` JRE)
and needs a JDK 21 plus network access. Release builds embed that archive, so run it
before `just dist`; a plain `cargo build` works without it and falls back to `java` on
`PATH`.

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
  directory: `.zip`/`.cbz`, `.rar`/`.cbr`, `.7z`/`.cb7`, `.tar`/`.cbt`, `.epub`. One
  directory or archive is one chapter; subdirectories inside archives are ignored.
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
  back to Unicode halfblocks. Folders, `zip`/`cbz`, `rar`/`cbr`, `7z`/`cb7`, `tar`/`cbt`,
  and image-based `epub` chapters are supported. EPUB follows Mihon: spine XHTML pages are
  scanned for `<img>`/`<image>` targets only; prose is not rendered. `m` toggles page-turn
  vs continuous scroll (`SlicedProtocol`); left/right align page tops, space/page keys
  scroll a screen, and
  `+`/`-` adjust zoom (no upscaling at 100%). Zoom rebuilds the row layout but anchors
  the viewport to the same source pixel, so long strips do not jump to the page start.
  Scroll mode reads page dimensions once,
  lazily decodes only visible/nearby blocks, caches their protocols, and uses Kitty shared
  memory when available. Long webtoon strips are split into small blocks (1024 px / at
  most 128 rows) to bound what one Kitty transmission has to copy and write; the worker
  caches the last two decoded pages so a strip's blocks share one download/decode. Kitty
  zlib compression is enabled as the fallback when shared memory is unavailable. The
  worker drops queued blocks outside the current viewport window, and the UI reveals at
  most one freshly built block per frame so scrolling cannot stall input on a multi-MB
  terminal write. Page decode/resize/encode runs on a background worker; the UI polls and
  redraws without blocking. `ScrollCacheConfig` in the TUI holds prefetch behind/ahead
  and cache-margin values; they are scaled by block size so the lookahead distance in rows
  stays stable. Progress is stored in `$XDG_DATA_HOME/tsuiyomi/progress.json` and written
  after a short idle period instead of on every page change. Chapter bookmarks are stored
  in `$XDG_DATA_HOME/tsuiyomi/bookmarks.json`; `b` toggles them from a story screen or the
  reader. Chapter rows use fixed bookmark/download columns. Right-to-left page mode
  reverses the left/right keys; vertical still uses paged mode. Page images support
  Fit, Fill/crop, and Stretch modes. Retransmit-free Kitty placement zoom is not
  implemented yet.
- `ComicInfo.xml` is parsed in `comic_info.rs` for series and for chapters in
  directories, `zip`/`cbz`, `rar`/`cbr`, `7z`/`cb7`, and `tar`/`cbt` archives. EPUB
  chapter metadata comes from the OPF (`dc:title`, publisher/creator, date), matching
  Mihon. Series precedence is `ComicInfo.xml` > `details.json` > directory name;
  malformed XML becomes a warning and falls back to `details.json`. Valid `cover.*` images
  are detected and rendered in the details pane. If none exists, scanning writes a
  generated `cover.jpg` from the first image of the first chapter, like Mihon (capped at
  512 px on the longest edge). Cover generation is best-effort and must not fail a scan.
- Categories come from `categories.json` in the library root (Mihon keeps them in its
  database). Unlisted series fall into `Default`. More → Categories adds/renames/deletes
  categories, and local story screens edit assignments; writes preserve the file format.
  Replace this with the DB layer later.
- The initial scan, refresh, and rescan run on a background scan worker after the TUI
  opens; the Library fills in when it finishes. Browse mutations and migration apply
  remain synchronous.
- Browse follows Mihon's source model. Sources include the built-in Local source and all
  sources exposed by the extension host (a bundled miwayomi JVM). Remote results, manga
  details, chapters, and the reader stay under the Browse tab; Browse never switches to
  Library. Browse has Sources, Extensions, and Migrate tabs (`[`/`]`) inside the panel
  title; Esc at a tab root returns to Library. Sources are grouped by last used, pinned,
  and language. They can be pinned (persisted in `pinned_sources`), and `o` opens a source
  management screen for enabling or disabling languages and individual sources (persisted
  in `enabled_languages` and `disabled_sources`). `g` opens the selected source's home
  page from installed extension metadata. Remote result pages append with `n`
  (or the next remote page action) while `has_next_page` is true. The Extensions tab has an
  inline search bar, detects installed-vs-repository version differences, shows the pending
  update count in the tab title, groups updates/installed/available by language, and offers
  `U` update-all. Installs and updates use host jobs with downloading/installing
  progress, cancellation (`c`), and retry; rows show job state, versions, languages,
  source counts, content warnings, and private/obsolete states. A failed file appears under Not loaded with its
  reason and can be inspected or deleted. Extension language filtering follows
  `enabled_languages`, and `show_nsfw_extensions` hides NSFW entries. Extension search
  accepts comma-separated terms matched against names, packages, languages, source
  names, and installed source IDs/base URLs. `o` on Extensions
  opens extension details with per-source toggles, enable/disable-all actions, source
  preferences, per-extension Incognito, clear-cookies, and uninstall. Signature trust is
  not implemented: the host cannot verify extension signatures, so there is no trust
  action. Migrate lists saved manga by source, multi-selects favorites, matches
  titles against enabled sources with regular/deep queries, previews targets, skips a
  preview (`s`), and migrates (`M`) or copies (`C`) progress/bookmarks/history/categories.
  Replace also moves downloaded chapters into the target source/series directory. Apply
  processes one match per event-loop tick with a progress status; Esc cancels between
  items and persists the migrations that already completed.
  `t` selects, reorders (`+`/`-`), and persists the enabled target sources. `/` on Sources
  runs a background global search over enabled pinned
  sources with per-source progress; empty and failed sources are rows, `h` toggles
  only-with-results, and `A` switches between pinned and all enabled sources. `o` opens
  the selected group's source with the query, `r` retries the search, and `Enter` opens
  the selected result. Results are grouped by source.
  `C` opens a configurable remote source's preferences (switch, text, and list values;
  multi-select is waiting on the host array contract). `z` opens source filters for
  search: checkbox, tri-state, text, select, sort, and nested groups. `v` cycles
  comfortable/compact/list result layouts, and `h` hides saved results. `F` saves a
  remote manga to the host's favorites. Saving checks normalized titles for duplicates and
  asks before removing an existing favorite. Saved manga keep their tsuiyomi categories in
  `remote_library` and are merged into the one Library grid under those categories, or
  the default category when none is recorded, and read through the same remote
  chapter/reader path. The shared story screen applies persisted chapter settings
  (`chapter_settings`, falling back to `chapter_defaults`): tri-state downloaded/unread/
  bookmarked filters, excluded scanlators, source/number/date/title sorting with
  direction, title/number display, and download-new-chapters. Rows show unread/in-progress/read markers, dates,
  scanlators, bookmarks, and download queue state. Chapter actions cover select all/
  invert, mark read/unread/previous, continue reading, single and batch downloads
  (selected, next 1/5/10/25, unread, bookmarked), queue cancellation, download deletion,
  categories, notes, migrate, tag copy, `ComicInfo.xml` export for local series,
  copy/open, cover overlay, tag search, and custom covers for local or saved source manga.
  Panels with item actions support multi-select with `Space` (stable keys, `✓` markers,
  count in the commands bar; Esc clears); delete, install/uninstall/update/pin, source pinning, store removal, save to
  library, chapter download, and bookmark all operate on the selection. In the Library
  grid, `x`/Delete opens a
  remove dialog with "Manga from library" and, for saved source manga, "Downloaded
  chapters" checkboxes, mirroring Mihon's delete dialog. Removing a local series writes
  its path to `hidden_series` and keeps the files; removing a saved manga calls the host,
  drops `remote_library`/caches, and optionally deletes its downloads directory. In
  Extensions, `i`/Enter installs and
  `x`/Delete asks for `y` confirmation before uninstalling; the commands bar lists both.
  Opening a saved manga shows the same
  details screen as Browse (cover, source, author/artist, status, tags, description,
  chapters). Remote covers are fetched through the host while it runs and written to
  `$XDG_DATA_HOME/tsuiyomi/covers` (proxy URLs are process-local, so the bytes, not the
  URL, are cached); `remote_library` itself stores no thumbnail URLs. `R` in the Library
  grid starts/uses the host to refetch missing covers, including calling details for
  favorites whose host entry has no thumbnail URL. Details fetched after opening a
  Browse result are copied back into the result tile. Results without thumbnails trigger
  a background details lookup (`cover_worker`) as they become visible, and the cover is
  applied to the result tile, global groups, saved library entry, and open details.
  Browse covers use the same persistent cache as Library covers, a details response that
  lacks a thumbnail never clears an existing one, and failed cover fetches retry after
  10 seconds instead of staying failed for the session.
  Local tiles regenerate a deleted or invalid generated `cover.jpg` through
  `ensure_series_cover` without a full rescan. Missing covers are retried when the
  Library tab is shown (15-second cooldown) and on `r`. Tiles load
  their cover immediately before drawing. The in-memory cover cache is a 256-entry,
  128 MiB-estimated LRU with covers downscaled to a 512 px longest edge, and drawing
  touches the visible entries. Kitty cover protocols are dropped when a reader closes so
  the next grid draw retransmits covers that the terminal evicted under its image quota.
  Covers are written atomically and validated before being stored, corrupt entries are
  replaced, and old `$XDG_CACHE_HOME` covers migrate on first load. If any saved cover
  is missing at startup, the host starts in the background once to reload favorites and
  refill them. Extensions are managed through the host (repo URLs, install, uninstall).
  tsuiyomi never loads APK bytecode itself; the host supplies the JVM, AndroidCompat, and
  dex2jar runtime. The bundled host is extracted from an optional archive in the binary
  into `$XDG_CACHE_HOME/tsuiyomi/host/<version>/` and supervised on loopback; `host_url`
  overrides it with an external server. Opening Browse for the first time starts the
  bundled host on `host_worker` instead of blocking the UI: the Sources screen renders
  immediately with a status row and fills in when the host answers. A start epoch drops
  results from a host that was stopped meanwhile. Remote progress uses synthetic
  `remote://source/manga/chapter` keys. Remote progress and bookmarks are mirrored to the
  host in batches and merged back when local state is missing. Browse reads, extension
  lists, migration matching, and library rescans use background workers; mutations and
  migration apply remain synchronous.
  WebView-dependent sources are unsupported. Library itself also uses a tile grid, not a
  list. Category keys only affect Library's series grid. There is no filesystem import
  browser; users place folders in local/ and rescan. Browse's Local source lists only
  `series.remote.is_none()` entries; saved source manga and downloads are not mixed in.
- Chapter downloads land in `<downloads>/<source>/<series>/<chapter>/<page>.<ext>` using
  Mihon's sanitized directory names (`downloads.rs`). `D` on a remote story screen opens
  a batch menu (selected, next N, unread, bookmarked) and the UI queue dispatches one
  chapter at a time on a worker thread; the reader downloads the current chapter
  directly. Queued and active rows show transient markers, `x`/Delete removes selected
  downloads after confirmation, and downloaded chapters are read from disk through the
  same reader path. Cancelling drops pending chapters and signals the active chapter;
  the core checks cancellation between page fetches, removes the partial `.tmp`
  directory, and never replaces an existing complete download. The root comes from
  `TSUIYOMI_DOWNLOADS`
  (runtime-only and never persisted), `downloads_path`, a sibling `downloads/` when the
  library lives in `local/`, or the app data directory.
- Saved-source details and chapters are cached per story in
  `$XDG_DATA_HOME/tsuiyomi/stories/<hash>.json` (`story_cache.rs`), including the source
  display name and description. The first open after saving fetches from the host, but
  later opens (even after a restart) render immediately without starting the JVM. When
  the host is already running, the first open in a session refreshes the story in the
  background through `story_worker`; `r` reloads synchronously. Chapter download markers
  are cached per story instead of statting every row each frame.
- `i` on the Library grid imports a series folder through a path-entry dialog. The core
  `import_series` helper copies it into one direct subdirectory, never overwrites an
  existing name, skips symbolic links, and removes a partial copy on failure.
- New local chapters are detected by comparing each scan with the snapshot in
  `$XDG_DATA_HOME/tsuiyomi/updates.json` (`updates.rs`). The first scan only seeds the
  snapshot. The Updates tab lists one entry per new chapter, newest first, for 30 days;
  `x`/Delete dismisses one, `X` clears the list, `Enter` reads it, and `r` rescans.
  Entries vanish when their chapter leaves the library. Saved source manga are not
  refreshed by a background update job.
- Reading history lives in an unbounded append-only
  `$XDG_DATA_HOME/tsuiyomi/history.jsonl` (`history.rs`), one record per chapter and
  compacted with tombstones. The UI shows the latest chapter per series, grouped by
  local date, with a small cover and accumulated read time. `Enter` resumes the current
  chapter if unfinished or the next chapter otherwise; `o` opens details, `F` saves a
  remote manga, `x`/Delete opens a single/all-series delete dialog, `X` clears all
  history, `/` searches titles, and `Space` multi-selects. Deletion is permanent and
  clears matching chapter progress so chapters become unread again. Rows show parsed
  chapter numbers, a star for saved remote manga, and omit the duration suffix when no
  time was recorded; clicking a row or cover selects it. History and Stats are mini-tabs
  inside the panel (`[`/`]`); Stats scrolls with the arrow keys, includes unique-series
  counts, and is derived from live history and per-chapter daily buckets, so deleting
  history removes its time from totals, charts, and top series. Reading-time gaps longer
  than two minutes are discarded. The duplicate-save dialog can open Migrate for the
  existing entry with `M`. `record_progress` flushes history with progress; global and
  per-extension Incognito suppress history and time.
- Library title filtering and backup import/export are implemented. Import restores local
  entries without the host; remote entries need a matching extension source. Exported
  backups still need a real Mihon restore check.
- Config persists `library_path`, `reading_mode`, `reader_zoom`,
  `scroll_prefetch_behind`, `scroll_prefetch_ahead`, `scroll_cache_margin`, `theme`,
  `accent`, `fullscreen`, `thumbnail_width`, `library_sort`, `default_category`,
  `incognito`, `host_url`, `host_jar`, `pinned_sources`, `last_used_source`,
  `enabled_languages`, `disabled_sources`, `incognito_extensions`, `local_source_order`,
  `global_search_scope`, `global_search_only_results`, `source_display_mode`,
  `browse_hide_saved`, `show_nsfw_extensions`, `flaresolverr_url`, `migration_sources`,
  `remote_library`, `downloads_path`, and `hidden_series`, and preserves unknown keys from
  existing files.
- More follows Mihon's structure: Settings, Downloads, Categories, Statistics, Data and
  storage, About, Help. Settings is a submenu matching Mihon's categories: Appearance,
  Library, Reader, Downloads, Tracking, Browse, Data and storage, Security, Advanced,
  About. Entry pages are listed in `SettingsPage`; each page is a gray-divider grouped
  row list. Implemented real options: appearance/theme/accent/thumbnail size, library
  path/rescan/sort/default category, reader mode/zoom/fit/prefetch/cache, downloads
  folder, backup import, and Security → Incognito (stops progress writes). Tracking is an
  informational placeholder. More → Data and storage shows library/downloads/host sizes
  on demand (`r`) and can clear cached covers (`c`) or all downloads (`d`, confirmed). More has a single focus: Enter
  enters the selected section/page or activates a setting, Esc backs out one level, and
  left/right adjust the selected setting. Clicking sections, menu rows, and sliders works.
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
  Selected tabs use lavender text only; do not fill their background. Tabs are hidden in
  the reader; the bottom bar shows prev/next chapter, a page gauge, and the mode toggle.
- Default theme: Catppuccin Mocha with lavender as the accent. Other built-in themes live
  in `palette.rs`; accents are palette colors or custom hex. Square borders, a single-row
  shared tab bar at the bottom, a commands box below it that grows and wraps when actions
  do not fit on one line, and colored metadata fields.
  Fullscreen hides the title and commands boxes.
- `rustfmt` defaults.
- New code: explicit imports, `snake_case`, small modules, doc comments on public core
  APIs.
- Prefer boring, testable code over clever abstractions.
- Add tests next to parsers. If a behavior depends on Mihon, cite the upstream file or
  doc in a comment.
- Keep refactors separate from behavior changes in commits.
