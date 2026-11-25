# AGENTS.md

Instructions for AI agents working on tsuiyomi.

## What this is

tsuiyomi is a terminal UI for managing and reading a **local manga library**. It is
**manga-first** and aims to be **compatible with [Mihon](https://mihon.app/)** (the
successor to Tachiyomi): the library on disk uses the same layout Mihon's Local source
reads, so the exact same files work in both apps.

The project is manga-first; anime browsing is outside the current scope.

## Status

All completion feature tables live in `docs/roadmap.md` under `## Parity detail`;
category design plans live under `docs/parity/`, and the release plan lives in
`docs/publishing.md`. The roadmap tables are the completion source of truth.

Local libraries, remote sources, extension management, the reader, downloads, tracking,
backups, history, and settings are implemented. The roadmap currently scores **97.7%**;
the remaining rows are live tracker/restore verification, Kitty placement zoom,
DNS-over-HTTPS, page-level download concurrency, custom reader color filters, and verbose
logging. Publishing is complete: v1.0.0 is released with six desktop archives and four
cached Nix closures. Windows x86_64/ARM64 builds and host self-tests pass; manual desktop
open/clipboard/notification checks remain. The API and config format are still unstable,
and large refactors are welcome when they move the project toward the architecture below.

## Progress documentation

Keep progress documentation current in the same commit as the work that changes it. This
is mandatory, not a follow-up:

- Update the affected rows in `docs/roadmap.md` immediately, then run
  `python3 scripts/parity-score.py` so the summary counts and percentages match.
- Update `docs/plan.md` checkboxes, remaining work, and risks when a planned item lands or
  a new gap is discovered.
- Update `docs/publishing.md` when release, CI, Nix, Cachix, Windows-portability, or
  documentation work changes.
- Update `README.md` and this file when user-facing or agent-facing behavior, commands,
  configuration, limitations, or status change.
- Keep comments and doc comments beside behavior changes accurate; do not leave stale
  "planned", "partial", or "missing" claims in place.
- Treat `docs/parity/*` checklists and `docs/history.md` as historical design context,
  not completion metrics. Only the roadmap tables drive the percentage.
- After updating progress docs, report the resulting roadmap score, the categories that
  changed, and any remaining blockers in the response.

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

## Backup inspection

`tsuiyomi backup-info <file>` parses a Mihon backup without touching the library and
prints counts plus per-series source, tracker, and flag fields. The import dialog also
shows counts and unmatched entries afterward. Use both when verifying a real Android
backup.

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
`PATH`. On Windows, `just host-runtime-windows` packages the published jar with a native
JDK 21 through `scripts/build-host-runtime.ps1`; it does not rebuild the patched jar from
source.

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

Mihon's `BackupManga.dateAdded` and `favoriteModifiedAt` fields are parsed and exported;
the library uses them for the recently-added sort, falling back to local filesystem
timestamps when a series predates the field.

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
  API). New filesystem access belongs in core; the TUI still has known direct-filesystem
  debt around covers, downloads, and config sidecars that should be moved behind core
  rather than extended.
- OS integration belongs in `tsuiyomi-core/src/platform.rs`: opening targets, native
  clipboard, desktop notifications, and replacing files atomically. Do not add per-OS
  commands to the TUI.
- Input handling and state updates are separate from rendering. Rendering must not mutate
  application data; view state such as `ListState` scroll offset is allowed.
- Paths come from configuration/env (`TSUIYOMI_LIBRARY`), never hardcoded. Missing or
  unreadable directories are reported, not panicked on.
- Prefer `Result` and typed errors. No `unwrap`/`expect` on user data, filesystem, or
  network input.
- Keep parsing logic (chapter names, `ComicInfo.xml`, `details.json`, backups) in core so
  it can be unit tested without a terminal.

## Current gaps

- Live AniList/MyAnimeList OAuth and token-paste tracker verification.
- A real Android Mihon backup restore (`backup-info` and the import report cover the
  pre-import half).
- Kitty placement-only zoom with a disable toggle.
- DNS-over-HTTPS in the host transport.
- Page-level download concurrency and custom reader color filters.
- Verbose application logging.

## Implementation notes

- Reader renders pages with `ratatui-image`: it auto-detects Kitty/Sixel/iTerm2 and falls
  back to Unicode halfblocks. Folders, `zip`/`cbz`, `rar`/`cbr`, `7z`/`cb7`, `tar`/`cbt`,
  and image-based `epub` chapters are supported. EPUB follows Mihon: spine XHTML pages are
  scanned for `<img>`/`<image>` targets only; prose is not rendered. `m` toggles page-turn
  vs continuous scroll (`SlicedProtocol`); left/right align page tops, space/page keys
  scroll a screen, and
  `+`/`-` adjust zoom (no upscaling at 100%). Zoom rebuilds the row layout but anchors
  the viewport to the same source pixel, so long strips do not jump to the page start.
  Continuous scroll loads a previous/current/next chapter window and inserts boundary
  cards with chapter names, scanlators, download state, and missing gaps; page mode shows
  a blank boundary page. Unloaded images render an ASCII spinner.
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
  reverses the left/right keys; paged vertical mode uses up/down page turns. Page images
  support Fit, Fill/crop, and Stretch modes. Page-mode zoom crops/pads the decoded page,
  pans with `h`/`l` and `j`/`k`, and re-encodes it. `o` opens the current chapter page or
  local chapter folder; `S`/`Y`/`E`/`W` open, copy, save, or cover the page. Reader settings
  skip read/duplicate chapters, transition at page edges, crop borders, render grayscale,
  show dual-page spreads, and split or rotate wide pages. Placement-only Kitty scaling is a planned opt-in with a re-encode fallback.
- `ComicInfo.xml` is parsed in `comic_info.rs` for series and for chapters in
  directories, `zip`/`cbz`, `rar`/`cbr`, `7z`/`cb7`, and `tar`/`cbt` archives. EPUB
  chapter metadata comes from the OPF (`dc:title`, publisher/creator, date), matching
  Mihon. Series precedence is `ComicInfo.xml` > `details.json` > directory name;
  malformed XML becomes a warning and falls back to `details.json`. Valid `cover.*` images
  are detected and rendered in the details pane. If none exists, scanning writes a
  generated `cover.jpg` from the first image of the first chapter, like Mihon (capped at
  512 px on the longest edge). Cover generation is best-effort and must not fail a scan.
- Categories come from `categories.json` in the library root (Mihon keeps them in its
  database). Unlisted series fall into `Default`. More → Categories adds, reorders,
  renames, and deletes categories, and local story screens edit assignments; writes
  preserve the file format.
  Replace this with the DB layer later.
- The initial scan, refresh, and rescan run on a background scan worker after the TUI
  opens; the Library fills in when it finishes. Browse reads and favorite add/remove run
  on workers; migration apply yields between items. Library deletion and backup import
  remain synchronous user-initiated bulk actions.
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
  source counts, content warnings, store names, and sideloaded/orphaned states; host
  installs are private because the extension directory belongs to tsuiyomi. A failed file
  appears under Not loaded with its reason and can be inspected or deleted. Extension
  language filtering follows `enabled_languages`, and `show_nsfw_extensions` hides NSFW
  entries. Extension search
  accepts comma-separated terms matched against names, packages, languages, source
  names, and installed source IDs/base URLs. `o` on Extensions
  opens extension details with per-source toggles, enable/disable-all actions, source
  preferences, per-extension Incognito, clear-cookies, and uninstall. Details show a
  SHA-256 file fingerprint and `Y` trusts/untrusts it (TOFU, warning on change). This is
  a documented deviation from APK signing-key verification: the host cannot verify
  signatures. Migrate lists saved manga by source, multi-selects favorites, matches titles
  against enabled sources with regular/deep queries, previews targets, skips a preview
  (`s`), hides unmatched rows (`h`), hides targets without newer chapters (`u`),
  prioritizes the source with the latest chapter (`p`),
  and migrates (`M`) or copies (`C`) progress/bookmarks/history/categories.
  Replace also moves—or, with the remove option, deletes—downloaded chapters in the old
  source/series directory. `z` opens migration options for chapters, categories, notes,
  custom cover, and old-download handling. Apply
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
  direction, title/number display, download-new-chapters, and an optional per-series
  viewer override with reset-to-global. Rows show
  unread/in-progress/read markers, scanlators, bookmarks, and download queue state;
  chapter dates are right-aligned and in-progress page counts follow the chapter title.
  Chapter actions cover select all/invert, mark read/unread/previous, continue reading,
  single and batch downloads
  (selected, next 1/5/10/25, unread, bookmarked), queue cancellation, download deletion,
  categories, notes, migrate, tag copy, AniList/MyAnimeList tracking and entry metadata (`T`),
  `ComicInfo.xml` export for local series, copy/open, cover share through the system
  viewer, relative source-page URL resolution through installed extension metadata, cover
  overlay, tag search, and
  custom covers for local or saved source manga.
  The details pane shows the cached story refresh age, the next check, and a warning when
  a remote source extension is not loaded. Saved stories whose cache is older than
  `update_interval_hours` refresh through `story_worker` while the host is running; checks
  are throttled to once a minute and ten stories per pass. `U` on the Library grid refreshes
  every saved story immediately. Panels with item actions support multi-select with
  `Space` (stable keys, `✓` markers, count in the commands bar; Esc clears); delete,
  install/uninstall/update/pin, source pinning, store removal, save to library, chapter
  download, and bookmark all operate on the selection. In the Library
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
  WebView-dependent sources are unsupported. Library supports comfortable, compact, and
  list layouts. Category keys only affect Library's series grid. `i` imports a series
  through a typed path dialog; there is no graphical file picker. Browse's Local source lists only
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
  Entries vanish when their chapter leaves the library. Saved-source refreshes also add
  entries for chapters not in the previous cached story; the first fetch only seeds a
  baseline. Remote entries open through the cached story and disappear if the favorite is
  removed.
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
- `C` on the Library grid opens the selected series at its next in-progress or unread
  chapter, matching Mihon's continue-reading action.
- Library has title filtering; unread, in-progress, bookmarked, tracked, missing, and
  downloaded filters (`z`); comfortable/compact/list layouts (`v`); a Continue shelf after
  the category names; random series (`B`); and unread/download/language badges on tiles.
- Library title filtering and backup import/export are implemented. Import restores local
  entries without the host; remote entries need a matching extension source. Exported
  backups still need a real Mihon restore check.
- Config persists user settings in `crates/tsuiyomi-core/src/config.rs` and preserves
  unknown keys from existing files. Major groups: library path/filters/layout/sort,
  reader modes and page transforms, downloads/policies/CBZ/concurrency, smart updates,
  notifications, host and FlareSolverr URLs, Browse/source preferences, migration flags,
  tracker auth and links, backup extras, trusted extension fingerprints, and hidden
  series. Treat the struct as the schema source of truth instead of duplicating the
  field list here.
- More follows Mihon's structure: Settings, Downloads, Categories, Statistics, Data and
  storage, About, Help. Settings is a submenu matching Mihon's categories: Appearance,
  Library, Reader, Downloads, Tracking, Browse, Data and storage, Security, Advanced,
  About. Entry pages are listed in `SettingsPage`; each page is a gray-divider grouped
  row list. Implemented options cover appearance, library path/layout/filters/sort/update
  restrictions, reader modes/page transforms, downloads and policies, notifications,
  tracking, Browse, backup/data cleanup, Security → Incognito, Advanced diagnostics and
  User-Agent, and About. Tracking supports AniList,
  MyAnimeList, MangaUpdates, Shikimori, Bangumi, Kitsu, MangaBaka, and Hikka through
  `tracker_worker`; AniList/MAL use browser OAuth, the others use session or pasted-token
  credentials. More → Downloads lists
  downloaded series with chapter counts and sizes (`r` refreshes, `c` cancels the queue,
  `x`/Delete removes one after confirmation) and shows the live queue. More → Statistics shows
  library/chapter counts, read states, reading-time totals, unique series, local/saved/
  downloaded title counts, and tracker links with a mean score. More
  → Data and storage shows library/downloads/host sizes on demand (`r`) and can clear
  cached covers (`c`) or all downloads (`d`, confirmed). More has a single focus: Enter
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
