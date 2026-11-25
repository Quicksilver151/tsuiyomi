# Tsuiyomi

A terminal UI for managing and reading a **local manga library**, built for libraries
laid out in [Mihon](https://mihon.app/)'s Local source format. The same files work in
both apps.

> **Status:** local Mihon libraries and extension-backed remote sources are implemented —
> scanning, browsing, metadata, covers, reading, progress, updates, downloads, backups,
> tracking, and settings. A
> [v1.0.0](https://github.com/Quicksilver151/tsuiyomi/releases/tag/v1.0.0) is available
> for Linux, macOS, and Windows x86_64/ARM64, with a public Cachix cache for Nix. The
> parity tables in [`docs/roadmap.md`](docs/roadmap.md) track the remaining external
> verification.

## Install

Prebuilt archives are published for Linux, macOS, and Windows on x86_64 and ARM64. See
[`INSTALL.md`](INSTALL.md) for the full archive contents, checksum verification, and
unsigned-binary notes.

### Linux

```sh
VERSION=1.0.0
TARGET=x86_64-unknown-linux-gnu  # or aarch64-unknown-linux-gnu
ARCHIVE="tsuiyomi-v${VERSION}-${TARGET}.tar.gz"
BASE="https://github.com/Quicksilver151/tsuiyomi/releases/download/v${VERSION}"

curl -LO "${BASE}/${ARCHIVE}"
curl -LO "${BASE}/SHA256SUMS"
grep "${ARCHIVE}" SHA256SUMS > "${ARCHIVE}.sha256"
sha256sum -c "${ARCHIVE}.sha256"
tar -xzf "${ARCHIVE}"
"./tsuiyomi-v${VERSION}-${TARGET}/tsuiyomi"
```

### macOS

```sh
VERSION=1.0.0
TARGET=aarch64-apple-darwin  # or x86_64-apple-darwin
ARCHIVE="tsuiyomi-v${VERSION}-${TARGET}.tar.gz"
BASE="https://github.com/Quicksilver151/tsuiyomi/releases/download/v${VERSION}"

curl -LO "${BASE}/${ARCHIVE}"
curl -LO "${BASE}/SHA256SUMS"
grep "${ARCHIVE}" SHA256SUMS > "${ARCHIVE}.sha256"
shasum -a 256 -c "${ARCHIVE}.sha256"
tar -xzf "${ARCHIVE}"
"./tsuiyomi-v${VERSION}-${TARGET}/tsuiyomi"
```

macOS Gatekeeper may block the first launch because releases are unsigned. Use
right-click → Open, or:

```sh
xattr -d com.apple.quarantine "./tsuiyomi-v${VERSION}-${TARGET}/tsuiyomi"
```

### Windows

PowerShell:

```powershell
$Version = "1.0.0"
$Target = "x86_64-pc-windows-msvc"  # or aarch64-pc-windows-msvc
$Archive = "tsuiyomi-v$Version-$Target.zip"
$Base = "https://github.com/Quicksilver151/tsuiyomi/releases/download/v$Version"

Invoke-WebRequest "$Base/$Archive" -OutFile $Archive
Invoke-WebRequest "$Base/SHA256SUMS" -OutFile SHA256SUMS

# Compare these two hashes before extracting.
(Get-FileHash $Archive -Algorithm SHA256).Hash
Select-String -Path SHA256SUMS -Pattern $Archive

Expand-Archive $Archive -DestinationPath .
& ".\tsuiyomi-v$Version-$Target\tsuiyomi.exe"
```

Windows SmartScreen may warn about an unknown publisher because the archive is unsigned.

### Nix

With flakes enabled, run the released tag directly:

```sh
nix run github:Quicksilver151/tsuiyomi/v1.0.0
```

Install into a profile:

```sh
nix profile install github:Quicksilver151/tsuiyomi/v1.0.0
```

Pin a release tag rather than `master` so the source and lockfile match the published
Cachix closures. Replace `v1.0.0` when a newer release is available.

#### NixOS and Home Manager

Add the released tag as an input and do **not** make it follow your own nixpkgs:

```nix
inputs.tsuiyomi.url = "github:Quicksilver151/tsuiyomi/v1.0.0";
```

The Cachix closures were built against tsuiyomi's locked nixpkgs revision. Following
your nixpkgs (for example, 25.11 while the release uses unstable) changes the
derivation and forces a local build. The separate input adds another nixpkgs to the
graph, but it lets Nix download the prebuilt package.

Refresh the lock entry after changing the input:

```sh
nix flake update tsuiyomi
```

Add the package and the binary cache to the system configuration:

```nix
environment.systemPackages = [
  inputs.tsuiyomi.packages.${pkgs.stdenv.hostPlatform.system}.default
];

nix.settings = {
  extra-substituters = [ "https://tsuiyomi.cachix.org" ];
  extra-trusted-public-keys = [
    "tsuiyomi.cachix.org-1:enUL7FsGLndXhMOFfoKFu/ra4gtM/RMHnCF5ai5wJGY="
  ];
};
```

NixOS applies `nix.settings` during activation, so the first rebuild after adding the
cache settings may still compile. Check with
`nix show-config | grep -E 'substituters|trusted-public-keys'` and rebuild again if
`tsuiyomi.cachix.org` is missing.

To confirm the cache has the release closure:

```sh
nix path-info --store https://tsuiyomi.cachix.org \
  "$(nix eval --raw 'github:Quicksilver151/tsuiyomi/v1.0.0#packages.x86_64-linux.default.outPath')"
```

Replace `x86_64-linux` with your system. For Home Manager, use `home.packages` instead
of `environment.systemPackages`. Nix is not a Windows distribution channel.

### Terminal requirement

Use a terminal with Kitty graphics protocol support so covers and pages render as images:

| Platform | Recommended terminals |
|---|---|
| Linux | Kitty, Ghostty, WezTerm, Konsole, Warp |
| macOS | Kitty, Ghostty, WezTerm, iTerm2 |
| Windows | WezTerm |

Windows Terminal does not implement the Kitty graphics protocol. Without a supported
terminal or a Sixel/iTerm2 terminal, images fall back to Unicode halfblocks. Terminal
multiplexers such as tmux may need graphics passthrough enabled.

## Goals

- Browse a local manga library from the terminal.
- Read chapters from directories and `zip`/`cbz`, `rar`/`cbr`, `7z`/`cb7`, `tar`/`cbt`,
  and image-based EPUB archives.
- Understand Mihon metadata: `ComicInfo.xml`, legacy `details.json`, `cover.jpg`, and
  chapter numbering parsed from folder/file names.
- Import and export Mihon backups (`.tachibk` / `.proto.gz`), including tracker links
  and per-series flags.
- Track reading progress on AniList, MyAnimeList, MangaUpdates, Shikimori, Bangumi,
  Kitsu, MangaBaka, and Hikka.
- Keep the backend UI-agnostic so a TUI, GUI, web UI, or CLI can share it.

Anime browsing is outside the current scope; the project is manga-first.

## Development

### Requirements

Development uses Nix; prebuilt releases do not.

- NixOS (or any system with Nix) with flakes enabled.
- `direnv` (recommended) or `nix develop`. A plain Rust toolchain also works, but the
  devshell is the supported development path.

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
just run backup-info ~/backup.tachibk   # inspect a Mihon backup without importing
just check
just fmt
just clippy
just test
```

Machine-local paths and log settings live in `.env.local`, which is gitignored. The
devshell sources it automatically.

Documentation: [`docs/plan.md`](docs/plan.md), the
[`docs/roadmap.md`](docs/roadmap.md) parity tables, the
[`docs/publishing.md`](docs/publishing.md) release plan, the
[`docs/history.md`](docs/history.md) historical development log, the
[`docs/releasing.md`](docs/releasing.md) release runbook, the [`CHANGELOG.md`](CHANGELOG.md),
and the [`INSTALL.md`](INSTALL.md) install guide. Category design plans live under
[`docs/parity/`](docs/parity/).

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

The Library tab opens as a cover tile grid, like Mihon. Tiles show the cover (or an
initial placeholder), title, and unread/download/language badges. `/` or `s` filters by
title and Esc clears it. `z` opens filters for unread, in-progress, bookmarked, tracked,
missing, and downloaded series. `v` cycles comfortable, compact, and list layouts; sorts
include natural, title, chapter count, latest chapter, unread, recently read, recently
added, tracker score, last update, chapter fetch date, and random. `B` opens a random
series. The Continue entry after the category names lists in-progress series by recency.
More → Categories lets `s` cycle a category’s sort and `v` cycle its layout, falling back
to the global setting. `[`/`]` switch categories.

Covers are centered without a border or tile background. `↑↓` moves by grid row, `←→` by
one tile, `Enter` opens the chapter list, `C` continues at the next in-progress/unread
chapter, `i` imports a series folder from a path, `r` rescans, `U` refreshes saved-source
stories now, and `R` refetches missing covers. The initial scan and later rescans run on a
background worker, so the TUI opens immediately and the grid fills in. Missing library
covers are retried when the Library tab is shown. `x`/Delete opens the remove dialog:
removing a local series keeps its files and hides it from future scans; saved source manga
can also delete downloaded chapters.

Saved manga from sources open the same details screen as Browse, with cover, source,
author/artist, status, tags, description, chapters, and the cached refresh age plus next
check. Saved stories refresh in the background while the host runs when their cache is
older than the configured interval; if the extension is not loaded, the details pane says
so. Covers are cached in `$XDG_DATA_HOME/tsuiyomi/covers` and survive restarts without the
host; missing or corrupt covers are regenerated, and local covers are rebuilt from the
first page on demand. Browse covers use the same cache, and empty details responses never
overwrite a working thumbnail. Details, description, and chapters are cached per story in
`$XDG_DATA_HOME/tsuiyomi/stories`, so reopening is immediate; `r` refreshes.

Chapter rows start with fixed bookmark and download columns (`★`/`☆`, `⬇`/`↓`); local
chapters use `·` in the download column. Tile width is adjustable in
**More → Settings → Appearance** (`thumbnail_width`, 8–28 columns).

## Updates

The Updates tab lists chapters that appeared since the previous library scan. The first
scan only records a baseline, so installing tsuiyomi does not mark the whole library as
new. `↑↓` selects an update, `Enter` reads it, `x`/`Delete` dismisses one entry, and `X`
clears the list. Entries are kept for 30 days and disappear if their chapter is deleted;
`r` rescans to look for more. Local scans and saved-source refreshes both add entries; a
saved story's first fetch after saving only records a baseline, then later new chapters
appear in the list. Opening a saved-source entry loads its cached story and chapter.

## Browse

Browse follows Mihon's source model with three tabs, switched with `[` and `]`:

- **Sources** — the Local source plus every source exposed by the extension host, grouped
  by last used, pinned, and language. `P` pins the selected source; pinned sources move to
  a `Pinned` group. `o` opens source management, where languages and individual sources
  can be enabled or disabled. `C` opens the selected remote source's preferences when the
  extension exposes them. Local lists only series folders from the configured library
  root, never saved remote manga or downloads. On Local, `s` searches folder titles, `l`
  switches between all and recently modified series, and `v` reverses the title order.
- **Extensions** — updates, not-loaded files, installed extensions, and available
  extensions by language. `/` searches comma-separated terms across names, packages,
  languages, source names, and installed source IDs/base URLs, `i` or `Enter` installs or
  retries available extensions, `x` or `Delete` uninstalls installed extensions or
  deletes failed files after a `y` confirmation, `u` updates the selected extension, `U`
  updates every extension with a pending update, `c` cancels running installs, `P` pins
  all sources of the selected extensions, `a` adds a store URL, and `t` opens the store
  list. Installs and updates report downloading/installing progress and errors. Rows show
  source count, language, installed/available versions, `18+` warnings, and
  sideloaded/orphaned states; host installs are private. Languages
  can be restricted in Sources `o`; NSFW extensions can be hidden from Settings → Browse.
  Rows show their store name, and not-loaded rows show their failure reason and can be
  inspected with `o` or deleted.
  Details show a SHA-256 file fingerprint; `Y` marks it trusted or untrusted and warns if
  a later update changes it. This is trust-on-first-use, a documented deviation from APK
  signing-key verification because the host does not expose signatures.
- **Migrate** — saved remote manga grouped by source. Open a source, select one or more
  favorites with `Space`, and press `M` to search enabled sources and preview matches.
  `t` chooses which enabled sources matching may use, with `+`/`-` reordering them;
  `s` skips the selected preview, `h` hides unmatched entries, `u` hides targets
  without newer chapters, `p` prioritizes the source with the latest chapter, and
  `Enter`/`o` searches for a different target,
  `M` migrates included entries (removing the old favorite), or `C` copies them. Progress,
  bookmarks, history, categories, notes, custom cover, and reading time move with the
  matched chapters, and replace moves or removes downloaded chapters according to the
  `z` migration options. Apply
  shows a progress status and processes one manga per tick; `Esc` cancels between items
  while keeping the migrations that already completed.
- **Global search** — `o` opens the selected source with the query filled in, `r`
  retries the search, `F` saves results, and `h` hides empty/failed sources.
- **Stores** — `a` adds a URL, `n` renames the selected store, `d` removes it, `C` copies
  it, `o` opens it in the system browser, and `r` refreshes the extension lists. Rows show
  the name, repository URL, and extension/installed counts.
- Saving a new remote manga opens a category chooser with the default category
  preselected. `Space` toggles categories, `Enter` saves, and `Esc` cancels.

Controls:

- `↑↓` moves through lists; `←→` moves by tile in grids.
- `Enter` opens the selected source, series detail, chapter, or extension action.
- Remote sources open with popular results. `s` searches, `l` loads latest updates when
  the source supports them, and `n` appends the next results page. Source lists,
  favorites, filters, preferences, extension lists, migration matching, favorite
  add/remove, result pages,
  and remote story details/chapters fetch on background workers with loading status. Moving onto the last visible result also
  fetches the next page automatically. `z` opens the source's
  filters when it exposes them: checkbox, tri-state, text, select, sort, and grouped
  filters, with Reset and Apply rows. `v` cycles comfortable grid, compact grid, and
  list layouts. Saved results carry a ✓ marker and a dimmed title; `h` hides them from
  the result list.
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
  scanlators, sort, direction, display, download-new-chapters, and a per-series viewer
  override), `s` cycles sort, and `v`
  cycles title/number display. `a`/`I` select all or invert, `m`/`u` mark selected chapters read or unread,
  `U` marks previous chapters read, and `n` continues with the next unread chapter.
- `/` on the Sources tab runs a background global search over enabled pinned sources,
  showing progress and grouping hits by source. Empty and failed sources appear as rows;
  `h` hides them. `A` switches between pinned and all enabled sources.

The Library is one grid: local series and manga saved from sources share it, categories,
sorting, and the reader. New saved manga go to the default category. Saved entries are
mirrored into the config so they appear before the extension host starts.

Local and saved-source series open the same story screen: chapter list on the left,
cover and metadata on the right. Browse uses that screen too. Rows show unread,
in-progress, and read state, scanlators, bookmarks, and download state. Chapter release
dates are right-aligned, and in-progress page counts sit immediately after the chapter
title;
chapter filters, sorting, display mode, and viewer overrides persist per series.
`x`/Delete removes selected downloads after confirmation, `c` edits categories for saved
source manga, `M` opens Migrate with the manga selected, `N` edits notes, `T` links a
story to a tracker or edits the linked entry's status, score, dates, privacy, and unlink
action, `Y` copies tags, `E` writes a Mihon-readable `ComicInfo.xml` for a local series,
`C` copies the URL, `S` opens the cover in the system viewer, `o` opens the series page
(or the current chapter page in the reader) and resolves local folders or relative source
URLs through extension metadata, `t` searches a tag, and `w`/`W` show
the cover or set a custom cover for local or saved source manga. An empty path removes a
custom source cover. Remote chapters can be downloaded with `D`; they are read from disk
afterwards, even when the source is offline. Cancelling a queue aborts the active
chapter between page fetches and removes its partial `.tmp` directory.

`i` copies a series folder from a typed path; there is no graphical file picker. You can
also place a folder or archive in the Mihon `local/` directory and rescan.

## Categories

More → Categories lists categories and their series counts. `a` adds a category, `+`/`-`
reorders it, `Enter` renames the selected category, and `x`/Delete removes it. `Default`
stays first. Local and saved-source story screens also edit category assignments; local
assignments are written to `categories.json` in the Mihon library root.

## History

The History tab shows one row per manga: its most recently read chapter. Rows have a
small cover, title, parsed chapter number, read time, source, page progress, and
accumulated reading time, grouped by Today / Yesterday / date. Saved remote manga carry a
star marker, and remote rows omit the duration suffix when no time was recorded. Clicking
a row or its cover selects it. `Enter` reopens the current chapter if it is unfinished,
otherwise the next chapter. `o` opens manga details, `F` saves a remote manga to the
library, `x`/`Delete` opens a delete dialog, `X` clears all history after confirmation,
and `/` searches series titles. `Space` multi-selects rows. Deletion is
permanent and clears the matching chapter progress, so chapters appear unread again;
bookmarks and downloads are untouched.

`[` and `]` switch between History and Stats inside the panel. Stats shows total, today,
7-day, and 30-day reading time, a 14-day bar chart, top series, reading streak, unique
series count, local/saved/downloaded title counts, tracker links and mean score, and
trend counts. Arrow keys or the wheel scroll Stats. Reading time is
accumulated while the reader is open, ignores gaps longer than two minutes, and is
aggregated from live history, so removing a history row also removes its time from Stats.

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
`TSUIYOMI_HOST_JAR_PATH` when running `just host-runtime` to reuse an already-built
patched jar. On Windows, `just host-runtime-windows` packages the published jar with a
native JDK 21 through `scripts/build-host-runtime.ps1`.

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

The resulting executable is about 140 MB. The archive is embedded at compile time,
extracted once to `$XDG_CACHE_HOME/tsuiyomi/host/0.2.9/`, and reused afterwards.
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
Prose is not rendered, matching Mihon's image-only manga chapter model. Malformed metadata
becomes a warning and falls back without hiding the series.

A custom cover is any `cover.*` image in the series directory (`cover.jpg` is the usual
name). It is shown beside the details pane when the terminal is wide enough. If no valid
cover exists, scanning generates `cover.jpg` from the first image of the first chapter,
like Mihon. Generated covers are capped at 512 px on the longest edge; existing valid
covers are never replaced.

## Reader mode

The default is `webtoon` (continuous vertical scroll). Settings are stored in
`~/.config/tsuiyomi/config.toml`, but the in-app settings pages are the source of truth.
A representative excerpt:

```toml
# library_path = "/path/to/mihon/local"  # defaults to $XDG_DATA_HOME/tsuiyomi/local
# host_url = "http://127.0.0.1:4567"  # use an external extension host
# host_jar = "/path/to/miwayomi.jar"  # development override
reading_mode = "webtoon"        # default, left_to_right, right_to_left,
                                # vertical, webtoon, continuous_vertical
reader_fit = "fit"              # fit, crop, scale
reader_zoom = 100
reader_skip_read = false
reader_skip_duplicates = true
reader_chapter_transitions = true
reader_crop_borders = false
reader_grayscale = false
reader_split_wide = false
reader_rotate_wide = false
reader_dual_page = false
scroll_prefetch_behind = 2
scroll_prefetch_ahead = 2
scroll_cache_margin = 4
# downloads_path = "/path/to/downloads"  # defaults to the sibling of local/
download_ahead = 0             # queue N following chapters while reading
delete_after_read = false      # remove a downloaded chapter after its last page
keep_bookmarked_downloads = true
save_chapters_as_cbz = false   # save downloads as .cbz archives
download_concurrency = 1       # simultaneous chapter downloads (1-5)
download_page_concurrency = 5  # simultaneous pages inside one chapter (1-15)
auto_download_new_chapters = false
# auto_download_categories = ["Reading"]
# download_excluded_categories = ["Keep"]
smart_update_unread_only = false
smart_update_skip_unstarted = false
smart_update_skip_completed = false
smart_update_release_days = 0
# update_categories = ["Reading"]
updates_tab_badge = true
notify_updates = true
notify_downloads = true
notify_tracker_failures = true
# user_agent = "tsuiyomi"       # direct page/cover/image fetches
theme = "catppuccin_mocha"
# accent = "#b4befe"            # optional palette color or hex override
fullscreen = false
thumbnail_width = 14           # cover tile width in columns, 8-28
library_display_mode = "comfortable"  # comfortable, compact, list
library_sort = "natural"       # natural, title_asc, title_desc, chapter_count_desc,
                                # latest_chapter_desc, unread_desc, last_read_desc,
                                # date_added_desc, chapter_fetch_desc, tracker_score_desc,
                                # last_update_desc, random
update_interval_hours = 12      # background saved-story checks; 0 disables
# default_category = "Reading"
incognito = false
# anilist_client_id = "..."     # registered AniList application client ID
# anilist_client_secret = "..." # optional; stored in plain text
# mal_client_id = "..."         # registered MyAnimeList client ID
# mal_client_secret = "..."     # optional; stored in plain text
# mangaupdates_username = "..." # session login; password is not stored
```

Per-category sort/layout overrides, migration flags, tracker tokens, and raw Mihon
backup flags are managed by the UI and stored alongside these keys.

Chapter transitions are on by default. Continuous scroll keeps the previous, current, and
next chapters in one strip and draws a boundary card between them with the chapter names,
scanlators, download state, and any missing-chapter gap. Page mode shows a blank boundary
card naming the adjacent chapter; the same page-turn key continues into it. Images that
are still loading show an animated white-block ツ, revealed one block at a time in
stroke order, instead of a frozen frame.

Library sort, layout, default category, missing-chapter display, duplicate-chapter
marking, and update restrictions live under **More → Settings → Library**. Incognito is
under **Security** and stops writing `progress.json` while leaving existing progress
readable.

Scroll mode measures page sizes in the background and shows the first page while it
works; remote dimensions are probed with small ranged requests. Set
`TSUIYOMI_DEBUG_SCROLL=1` to write timings to `/tmp/tsuiyomi-scroll.log`.

Reading mode, zoom, page fit, skip rules, transitions, crop/grayscale, wide-page layout,
dual-page spreads, and scroll-cache settings are also under
**More → Settings → Reader**. A series can override the global viewer from its chapter
settings; “Reset viewer to global” clears the override. Right-to-left mode reverses the
left/right page-turn keys in paged mode; vertical keeps paged mode but turns pages with
up/down. Page-turn modes use the **Page fit** setting (Fit, Fill/crop, or Stretch);
`webtoon` and `continuous_vertical` open in scroll mode. `m` switches between page and
scroll for the current session. Scroll mode fits pages to the terminal width without
upscaling by default; `+`/`-` adjusts zoom from 50% to 300%. In page mode zoom crops into
the page center (or pads it when zooming out); `h`/`l` and `j`/`k` pan the zoomed page.
Page zoom re-encodes the image; placement-only Kitty scaling is not implemented yet. In
scroll mode zooming rebuilds the scroll layout but keeps the top of the viewport on the
same point in the page, including inside very long webtoon strips. Only pages near the
viewport are decoded and cached, and page preparation runs off the UI thread.

Reader page actions: `S` opens the page externally, `Y` copies its URL/path, `E` saves it
under `$XDG_DATA_HOME/tsuiyomi/exports`, and `W` uses it as the story cover. `o` opens the
chapter page/folder, `D` downloads the current chapter, and `b` toggles its bookmark.

## More and settings

The More tab follows Mihon's structure: Settings, Downloads, Categories, Statistics,
Data and storage, About, and Help. Settings pages are Appearance, Library, Reader,
Downloads, Notifications, Tracking, Browse, Data and storage, Security, Advanced, and
About.

- **Appearance** — theme, accent, fullscreen, thumbnail size.
- **Library** — library path, rescan, layout, sort order, default category,
  missing-chapter display, duplicate-chapter marking, saved-story update interval,
  smart-update restrictions (unread-only, skip unstarted/completed/recent releases),
  update category scope, and the Updates tab badge.
- **Reader** — reading mode, page fit, skip-read/duplicate rules, chapter transitions,
  border cropping, grayscale, split/rotate wide pages, dual-page spreads, zoom, scroll
  prefetch, and cache margins.
- **Downloads** — download folder, read-ahead count, delete-after-read and
  keep-bookmarked/keep-category policies, CBZ output, chapter concurrency,
  global/category new-chapter downloads, and category exceptions.
- **Notifications** — desktop toggles for new chapters, download completion/failure, and
  tracker failures.
- **Tracking** — AniList and MyAnimeList browser login with application client IDs,
  MangaUpdates username/password session login, and pasted access tokens for Shikimori,
  Bangumi, Kitsu, MangaBaka, and Hikka.
- **Browse** — extension host URL or bundled runtime and NSFW extension visibility.
- **Statistics** — library/series/chapter counts, read states, reading time, unique
  series, local/saved/downloaded titles, and tracker links with a mean score.
- **Data and storage** — storage counts and library/downloads/host sizes, refreshed
  with `r`; `c` clears cached covers and `d` deletes all downloaded chapters after
  confirmation. Imports and exports Mihon `.tachibk` / `.proto.gz` backups.
- **Security** — Incognito mode, which stops writing reading progress and history.
- **Advanced** — optional FlareSolverr URL, custom User-Agent for direct fetches, and a
  diagnostics view for version, paths, host state, tracker links, queue, and log level.
- **About** — version, graphics protocol, reader state, host state, data directory, and
  host log path. `H` starts or stops the host.

Backup import restores local and saved-source entries, categories, notes, chapter state,
bookmarks, history, tracker links, viewer/chapter flags, update strategy, excluded
scanlators, next-update times, and fetched dates. Saved favorites are added when source
names match the extension host; local entries work without it. After an import, a report
shows imported/local/saved counts, chapter/bookmark/history totals, skipped/failed counts,
and unmatched titles. Export writes the same fields in Mihon's protobuf layout.
`tsuiyomi backup-info <file>` inspects a backup without importing it.

More → Downloads shows the download folder, active queue and waiting chapters, and
downloaded series with chapter counts and sizes (`r` refreshes, `c` clears the queue,
`p` pauses after the active chapter, `j`/`k` select and `+`/`-` reorder pending chapters,
and `x`/Delete removes a series after confirmation). Downloaded chapters follow
`<downloads>/<source>/<series>/<chapter>/<page>.<ext>`, or a `.cbz` archive when CBZ
output is enabled.

Tracking supports AniList, MyAnimeList, MangaUpdates, Shikimori, Bangumi, Kitsu,
MangaBaka, and Hikka. Tracker entries store progress, status, score, dates, privacy where
supported, and an optional remote URL for slug-based services.

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

The phased plan and full parity tables live in [`docs/roadmap.md`](docs/roadmap.md) and
[`docs/plan.md`](docs/plan.md). Implemented so far:

- [x] UI-agnostic core crate plus TUI frontend.
- [x] Mihon `local/` scanning, chapter recognition, and natural page ordering.
- [x] Folders and `cbz`/`zip`, `cbr`/`rar`, `cb7`/`7z`, `cbt`/`tar`, and image EPUB.
- [x] `ComicInfo.xml`, legacy `details.json`, custom covers, and generated covers.
- [x] Built-in Local source, remote sources, extension management, and global search.
- [x] Kitty/Sixel/iTerm2 reader with halfblocks fallback, page/scroll modes, zoom,
      page actions, dual-page spreads, and wide-page layout.
- [x] Library layouts, filters, sorts, Continue shelf, random series, and per-category
      overrides.
- [x] Updates, history, statistics, migrations, downloads (including CBZ and concurrency),
      desktop notifications, and diagnostics.
- [x] AniList, MyAnimeList, MangaUpdates, Shikimori, Bangumi, Kitsu, MangaBaka, and Hikka
      tracker adapters.
- [x] Mihon backup import/export, `backup-info`, and an import report.
- [ ] Page-level download concurrency, custom reader color filters, Kitty placement-only
      zoom, and DNS-over-HTTPS.
- [ ] Create the Cachix cache and publish `v1.0.0` (see the Publishing category in the
      roadmap).
- [ ] Verify live OAuth/token tracking and a real Android Mihon restore.
- [ ] Publish to `crates.io` and release `v1.0.0`.

## License

tsuiyomi is released under the [MIT License](LICENSE). Release binaries also bundle
miwayomi, an OpenJDK runtime, and UnRAR support; see
[THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES) and [`licenses/`](licenses/) for their terms.

## AI disclosure

This project follows the [0rwx](https://0rwx.com/) framework. The project-level label is
**AI:W** (WRITE — AI collaboration): recent code and documentation were produced with an
AI under human direction, and the maintainer reads, tests, and fixes the output.

Different parts of the repository sit at different 0rwx levels, so `AI:0rwx` is used here
as a project descriptor spanning levels 0, R, W, and X — not as a single 0rwx level. The
official labels are `AI:0`, `AI:R`, `AI:W`, and `AI:X`.
