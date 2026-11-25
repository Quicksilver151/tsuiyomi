# Roadmap

All parity feature tables live below, with category plans under [`parity/`](parity/).
Scores are generated from the tables by `scripts/parity-score.py`.

<!-- parity-summary:start -->

## Parity completion

| Category | ✅ | 🟡 | ❌ | ➖ | Score |
|---|---:|---:|---:|---:|---:|
| [On-disk compatibility](#on-disk-compatibility) | 24 | 1 | 0 | 1 | 98.0% |
| [Library](#library) | 27 | 0 | 0 | 0 | 100.0% |
| [Browse, sources, and global search](#browse-sources-and-global-search) | 30 | 1 | 0 | 1 | 98.4% |
| [Extensions and host lifecycle](#extensions-and-host-lifecycle) | 23 | 0 | 0 | 2 | 100.0% |
| [Shared story / chapter screen](#shared-story--chapter-screen) | 46 | 0 | 0 | 1 | 100.0% |
| [Reader](#reader) | 23 | 2 | 0 | 0 | 96.0% |
| [History and Stats](#history-and-stats) | 21 | 0 | 0 | 0 | 100.0% |
| [Migrate](#migrate) | 23 | 0 | 0 | 0 | 100.0% |
| [Tracking](#tracking) | 10 | 3 | 0 | 0 | 88.5% |
| [Downloads](#downloads) | 7 | 0 | 0 | 1 | 100.0% |
| [Settings, security, and data](#settings-security-and-data) | 15 | 2 | 1 | 0 | 88.9% |
| [Documented TUI deviations](#documented-tui-deviations) | 4 | 0 | 0 | 4 | 100.0% |
| [Publishing](#publishing) | 14 | 1 | 0 | 1 | 96.7% |
| **All categories** | **267** | **10** | **1** | **11** | **97.8%** |

Score = `(✅ + 0.5×🟡) / (✅ + 🟡 + ❌)`. `➖` intentional TUI deviations are
excluded. Recompute with `python3 scripts/parity-score.py` after editing a
table below.

<!-- parity-summary:end -->

## Parity detail

### On-disk compatibility

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Layout | `local/<series>/` library root | Yes | Yes | ✅ | `TSUIYOMI_LIBRARY` or app-data default. |
| Layout | One direct subdirectory per series | Yes | Yes | ✅ | Scanner ignores files at the root. |
| Layout | Hidden entries ignored | Yes | Yes | ✅ | `.nomedia`, `.noxml`, `.DS_Store`, hidden dirs. |
| Chapters | Directory chapter | Yes | Yes | ✅ | One directory = one chapter. |
| Chapters | `.zip` / `.cbz` | Yes | Yes | ✅ | Natural page order. |
| Chapters | `.rar` / `.cbr` | Yes | Yes | ✅ | Bundled UnRAR support. |
| Chapters | `.7z` / `.cb7` | Yes | Yes | ✅ | |
| Chapters | `.tar` / `.cbt` | Yes | Yes | ✅ | |
| Chapters | `.epub` | Yes | Yes | ✅ | Image targets from spine XHTML, matching Mihon; prose is not part of a manga chapter. |
| Chapters | Number recognition | Yes | Yes | ✅ | Numbers, `v2`, `c001`, ranges, specials. |
| Chapters | Natural page ordering | Yes | Yes | ✅ | Case-insensitive natural sort. |
| Metadata | `ComicInfo.xml` series metadata | Yes | Yes | ✅ | Preferred over legacy metadata. |
| Metadata | `details.json` fallback | Yes | Yes | ✅ | Legacy `title`, `author`, `artist`, `description`, `genre`, `status`. |
| Metadata | Directory-name fallback | Yes | Yes | ✅ | Metadata failure never hides a series. |
| Metadata | Chapter `ComicInfo.xml` | Yes | Yes | ✅ | Directory and archive chapters. |
| Metadata | EPUB OPF metadata | Yes | Yes | ✅ | `dc:title`, creator/publisher, date. |
| Covers | `cover.jpg` custom cover | Yes | Yes | ✅ | Other valid `cover.*` images also work. |
| Covers | Generated first-page cover | Yes | Yes | ✅ | Best effort, capped at 512 px longest edge. |
| Categories | Mihon database categories | Yes | ➖ | ➖ | Mihon keeps categories in its Android database; tsuiyomi uses a portable `categories.json` sidecar, with unlisted series in `Default`. |
| Metadata writes | Mihon-readable metadata export | Yes | ✅ | ✅ | `E` writes `ComicInfo.xml` for a local series; existing files are kept. Legacy `details.json` is not rewritten. |
| Backups | Import `.tachibk` / `.proto.gz` | Yes | ✅ | ✅ | Local and remote favorites, categories, notes, chapter state, bookmarks, and history. Local entries work without the extension host. |
| Backups | Export Mihon-restorable backups | Yes | ✅ | ✅ | Local and remote library, categories, chapter state, history, notes, and date-added fields. Protobuf fields match current upstream models and round-trip in tests. |
| Backups | Tracking entries | Yes | ✅ | ✅ | Field 18 is parsed and exported for every tracker service with a known Mihon sync id. |
| Backups | Viewer/chapter flags and update strategy | Yes | ✅ | ✅ | Viewer mode, chapter filters/sort, update strategy, and next update are preserved; the first two also apply to chapter settings. |
| Backups | Excluded scanlators and next update | Yes | ✅ | ✅ | Fields 108 and 113 are parsed, applied, and exported. |
| Backups | Real Mihon restore verification | Yes | 🟡 | 🟡 | Protobuf round-trip and `backup-info` pass; an Android Mihon restore still needs a real backup/device. |

### Library

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Grid | Cover tile grid | Yes | Yes | ✅ | Adjustable tile width. |
| Grid | Generated/placeholder covers | Yes | Yes | ✅ | Initials placeholder when no image. |
| Grid | Saved source manga merged in | Yes | Yes | ✅ | Mirrored in `remote_library`. |
| Grid | Comfortable grid / compact grid / list layouts | Yes | ✅ | ✅ | `v` cycles comfortable, compact, and list layouts; persisted. |
| Grid | Unread / download / language badges | Yes | ✅ | ✅ | Unread count, downloaded-chapter count, and source language codes on covers. |
| Categories | Category tabs/filtering | Yes | ✅ | ✅ | `[`/`]`; default category. |
| Categories | Edit categories from story | Yes | ✅ | ✅ | Local and saved source manga; local assignments write `categories.json`. |
| Sorting | Natural / title A–Z / Z–A | Yes | ✅ | ✅ | Persisted `library_sort`. |
| Sorting | Other Mihon sort fields | Yes | ✅ | ✅ | Chapter count, latest chapter, unread, recently read, recently added, tracker score, last update, chapter fetch date, and random. |
| Sorting | Per-category sort and display settings | Yes | ✅ | ✅ | More → Categories cycles each category’s sort (`s`) and layout (`v`); unset inherits the global setting. |
| Search | Library title filter | Yes | ✅ | ✅ | `/` or `s` filters the Library grid; Esc clears. |
| Filters | Library filter dialog | Yes | ✅ | ✅ | `z` toggles unread, in-progress, bookmarked, tracked, missing, and downloaded filters. |
| Updates | Updates tab | Yes | ✅ | ✅ | New local chapters per scan and newly discovered saved-source chapters, kept 30 days; Enter reads, `x` dismisses, `X` clears. |
| Updates | Update settings and smart-update rules | Yes | ✅ | ✅ | Unread-only, skip-unstarted, skip-completed, and recent-release limits. |
| Updates | Category-only update and tab-badge setting | Yes | ✅ | ✅ | Category scope for automatic updates and a tab-badge toggle. |
| Removal | Remove local series, keep files | Yes | ✅ | ✅ | Path stored in `hidden_series`. |
| Removal | Remove saved source manga + downloads | Yes | ✅ | ✅ | Delete dialog with optional downloads. |
| Covers | Persistent cover cache | Yes | ✅ | ✅ | LRU, 512 px, atomic writes, Kitty reset. |
| Covers | Automatic cover recovery | Yes | ✅ | ✅ | Missing saved covers trigger host refill. |
| Selection | Multi-select and batch delete | Yes | ✅ | ✅ | Stable keys and command-bar count. |
| Reading | Continue-reading shelf/button | Yes | ✅ | ✅ | `C` continues the selected series; a Continue shelf after the categories lists in-progress series by recency. |
| Discovery | Random manga action | Yes | ✅ | ✅ | `B` opens a random visible series. |
| Filters | Downloaded-only library view | Yes | ✅ | ✅ | Download filter in `z`, plus per-tile download counts. |
| Display | Hide missing-chapter indicators | Yes | ✅ | ✅ | Settings → Library toggle hides the count. |
| Reading | Mark duplicate chapters read | Yes | ✅ | ✅ | Marking chapters read propagates to same-number chapters, and refreshed chapters inherit read state from read duplicates. |
| Scan | Background scan | Yes | ✅ | ✅ | Refresh, rescan, and the initial scan run on a worker after the UI opens. |
| Scan | Import browser / file picker | Yes | ✅ | ✅ | `i` opens a path dialog and copies a folder into `local/`; name collisions get a suffix. |

### Browse, sources, and global search

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Navigation | Sources / Extensions / Migrate tabs | Yes | ✅ | ✅ | `[`/`]`, Esc returns to Library at roots. |
| Sources | Local + host sources | Yes | ✅ | ✅ | |
| Sources | Last-used / pinned / language groups | Yes | ✅ | ✅ | Persisted. |
| Sources | Pin source | Yes | ✅ | ✅ | `P`. |
| Sources | Enable/disable languages and sources | Yes | ✅ | ✅ | Management screen with `o`. |
| Sources | Source preferences | Yes | 🟡 | 🟡 | Switch, text, and list preferences work; multi-select still waits on the host array contract. |
| Sources | Open source home page | Yes | ✅ | ✅ | `g`; resolved from installed extension source metadata. |
| Sources | Source filters | Yes | ✅ | ✅ | Checkbox, tri-state, text, select, sort, groups. |
| Sources | Search / latest / filters | Yes | ✅ | ✅ | |
| Results | Comfortable / compact / list layouts | Yes | ✅ | ✅ | `v`, persisted. |
| Results | In-library markers | Yes | ✅ | ✅ | Saved results marked. |
| Results | Hide saved results | Yes | ✅ | ✅ | `h`, persisted. |
| Results | Dim/mark saved results | Yes | ✅ | ✅ | `✓` plus dimmed title/status in terminals. |
| Results | Pagination | Yes | ✅ | ✅ | `n` plus automatic load at the last result. |
| Results | Empty/error retry hints | Yes | ✅ | ✅ | Retry and source-home hints in empty result states. |
| Results | Save / remove with confirmation | Yes | ✅ | ✅ | Multi-select supported. |
| Results | Duplicate detection | Yes | ✅ | ✅ | Add anyway, open existing, Migrate. |
| Results | Category choice when saving | Yes | ✅ | ✅ | Default preselected. |
| Global search | Pinned/all scope | Yes | ✅ | ✅ | `A`, persisted. |
| Global search | Progressive per-source results | Yes | ✅ | ✅ | Background worker. |
| Global search | Per-source loading rows | Yes | ✅ | ✅ | Pending groups show `searching…`. |
| Global search | Empty/error source rows | Yes | ✅ | ✅ | `h` only-with-results. |
| Global search | Retry / cancel previous search | Yes | ✅ | ✅ | `r`; generation drops stale results. |
| Global search | Open group’s source with query | Yes | ✅ | ✅ | `o`. |
| Global search | Open selected result | Yes | ✅ | ✅ | `Enter`. |
| Global search | Group ordering | Yes | ✅ | ✅ | Pinned first, normal next, empty/error last, then name. |
| Stores | Add / remove repository | Yes | ✅ | ✅ | |
| Stores | Copy / open / refresh store URL | Yes | ✅ | ✅ | `C`, `o`, `r`. |
| Stores | Store name / website / counts | Yes | ✅ | ✅ | Rows show an editable display name, the repository URL, and extension/installed counts. |
| Networking | Background Browse calls | Yes | ✅ | ✅ | Browse reads, favorite mutations, and saved-story refreshes run on workers; migration apply processes one item per tick. Library deletion and backup import are separate synchronous bulk actions. |
| Search | Extension source URL/id search terms | Yes | ✅ | ✅ | Matches names, packages, languages, source IDs, and source base URLs for installed and repository entries. |
| Search | Debounced extension search | Yes | ➖ | ➖ | Local filtering is immediate, so a debounce would only add latency. |

### Extensions and host lifecycle

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Sources | Repo + installed merge | Yes | ✅ | ✅ | Installed version preserved separately. |
| Updates | Update detection | Yes | ✅ | ✅ | Version comparison. |
| Updates | Update badge / update all | Yes | ✅ | ✅ | `U`. |
| Grouping | Updates / not loaded / installed / available | Yes | ✅ | ✅ | Available grouped by language. |
| Install | Async progress | Yes | ✅ | ✅ | `pending/downloading/installing/done/error/cancelled`. |
| Install | Cancel active install | Yes | ✅ | ✅ | Host job cancellation; `c` in Extensions. |
| Install | Retry failed install | Yes | ✅ | ✅ | `i` clears the error and retries. |
| Install | Old-host blocking fallback | Yes | ✅ | ✅ | Falls back when job API is missing. |
| Install | Desktop JVM jar preference | Yes | ✅ | ✅ | Avoids dex2jar corruption. |
| Not loaded | Section and reason | Yes | ✅ | ✅ | Package, file, reason, message. |
| Not loaded | Inspect / delete failed file | Yes | ✅ | ✅ | `o`, `x`/Delete with confirmation. |
| Trust | Signature verification / trust action | Yes | ➖ | ➖ | `Y` trusts a SHA-256 file fingerprint (TOFU) and flags changes; APK signing-key verification is unavailable in the host, so no signature-trusted state is fabricated. |
| Uninstall | Confirmation and batch uninstall | Yes | ✅ | ✅ | |
| Details | Package, version, store, sources | Yes | ✅ | ✅ | |
| Details | Enable/disable all sources | Yes | ✅ | ✅ | |
| Details | Per-source toggle | Yes | ✅ | ✅ | |
| Details | Per-source preferences | Yes | ✅ | ✅ | |
| Details | Clear cookies | Yes | ✅ | ✅ | Fixed against the 1.6 source API. |
| Details | Per-extension Incognito | Yes | ✅ | ✅ | |
| Warnings | NSFW/content warning indicator | Yes | ✅ | ✅ | `18+` in rows and details. |
| Warnings | Hide NSFW extensions | Yes | ✅ | ✅ | Settings → Browse. |
| Warnings | Private/shared/orphaned indicators | Yes | ✅ | ✅ | Host installs are private, store/sideloaded origin and orphaned flags are shown; shared system installs do not exist on desktop. |
| Languages | Enabled-language filtering | Yes | ✅ | ✅ | Installed entries remain manageable. |
| Search | Comma-separated extension terms | Yes | ✅ | ✅ | Matches names, packages, languages, source names. |
| WebView | WebView-dependent sources | Yes | ➖ | ➖ | Unsupported by design; documented TUI deviation. |

### Shared story / chapter screen

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Shared screen | Library local / saved remote / Browse / History | Yes | ✅ | ✅ | One story component set. |
| Header | Cover, title, author, artist, source | Yes | ✅ | ✅ | |
| Header | Status, tags, description | Yes | ✅ | ✅ | |
| Header | Chapter count and latest chapter | Yes | ✅ | ✅ | |
| Header | Notes | Yes | ✅ | ✅ | `N`, persisted per story. |
| Header | Missing-chapter count | Yes | ✅ | ✅ | |
| Header | Source/stub warning | Yes | ✅ | ✅ | Warns when a remote source is not loaded; saved entries always keep their source, so Mihon migration stubs do not occur. |
| Header | Next update / fetch interval | Yes | ✅ | ✅ | Saved stories refresh in the background when older than the configurable interval; details show the last refresh and next check. Checks run only while the host is up. |
| Header | Add/remove library | Yes | ✅ | ✅ | `F`. Remove confirmation exists; downloads checkbox is Library-level. |
| Header | Edit categories | Yes | ✅ | ✅ | Saved source manga; `c`. |
| Header | Migrate from story | Yes | ✅ | ✅ | `M`, preloads Migrate. |
| Header | Tracking action | Yes | ✅ | ✅ | `T` searches AniList and MyAnimeList, links the story, shows progress, and unlinks after confirmation. |
| Header | Copy/share URL | Yes | ✅ | ✅ | Native clipboard on Windows/macOS; OSC 52 elsewhere and as fallback. |
| Header | Open source page | Yes | ✅ | ✅ | Opens absolute URLs, local folders, and relative source paths resolved through the installed extension base URL; falls back to copied text when metadata is unavailable. |
| Header | Refresh details and chapters | Yes | ✅ | ✅ | Remote refresh; local rescan. |
| Header | Continue reading | Yes | ✅ | ✅ | In-progress first, then first unread. |
| Chapter list | Title / number / date | Yes | ✅ | ✅ | Display mode switches title/number emphasis. |
| Chapter list | Read/unread/in-progress state | Yes | ✅ | ✅ | Markers plus dimmed read titles. |
| Chapter list | Progress | Yes | ✅ | ✅ | In-progress rows. |
| Chapter list | Bookmark marker | Yes | ✅ | ✅ | |
| Chapter list | Download state/progress | Yes | ✅ | ✅ | Queued/downloading/error/cancelled/done. |
| Chapter list | Scanlator | Yes | ✅ | ✅ | |
| Filters | Downloaded tri-state | Yes | ✅ | ✅ | |
| Filters | Unread tri-state | Yes | ✅ | ✅ | |
| Filters | Bookmarked tri-state | Yes | ✅ | ✅ | |
| Filters | Excluded scanlators | Yes | ✅ | ✅ | |
| Sorting | Source/number/date/title | Yes | ✅ | ✅ | Direction toggle. |
| Display | Title vs chapter number | Yes | ✅ | ✅ | |
| Persistence | Per-series settings | Yes | ✅ | ✅ | `chapter_settings`. |
| Persistence | Global defaults | Yes | ✅ | ✅ | `chapter_defaults`. |
| Selection | Multi-select / count | Yes | ✅ | ✅ | |
| Selection | Select all / invert | Yes | ✅ | ✅ | `a`, `I`. |
| Selection | Mark read/unread | Yes | ✅ | ✅ | `m`, `u`. |
| Selection | Mark previous read | Yes | ✅ | ✅ | `U`. |
| Downloads | Download selected | Yes | ✅ | ✅ | |
| Downloads | Next 1/5/10/25 | Yes | ✅ | ✅ | Download menu. |
| Downloads | Unread / bookmarked batches | Yes | ✅ | ✅ | |
| Downloads | Cancel queue / active chapter | Yes | ✅ | ✅ | Core cancellation hook. |
| Downloads | Delete selected downloads | Yes | ✅ | ✅ | Confirmed. |
| Downloads | Auto-download new chapters | Yes | ✅ | ✅ | Per-series setting. |
| Downloads | Series download screen | Yes | ✅ | ✅ | More → Downloads shows the active queue, waiting chapters, downloaded series, sizes, `c` queue cancellation, and per-series deletion. |
| Actions | Copy/open, cover overlay | Yes | ✅ | ✅ | `C`, `o`, `w`. |
| Actions | Custom cover | Yes | ✅ | ✅ | Local and saved source manga; `W`. |
| Actions | Remote cover share/delete | Yes | ✅ | ✅ | `S` opens the cached cover in the system viewer; an empty custom-cover path removes a cover. |
| Actions | Tag search | Yes | ✅ | ✅ | `t` picker. |
| Actions | Tag copy | Yes | ✅ | ✅ | `Y` copies the story tags through the clipboard. |
| Gestures | Swipe to read/bookmark/download | Yes | ➖ | ➖ | TUI uses keys and menu actions. |

### Reader

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Formats | Folders, zip/cbz, rar/cbr, 7z/cb7, tar/cbt | Yes | ✅ | ✅ | |
| Formats | Image EPUB | Yes | ✅ | ✅ | Prose ignored. |
| Images | Kitty / Sixel / iTerm2 | Yes | ✅ | ✅ | Auto-detected. |
| Images | Unicode halfblock fallback | Yes | ✅ | ✅ | |
| Actions | Page actions: copy/save page, set cover, open externally | Yes | ✅ | ✅ | Reader `S` opens the page, `Y` copies it, `E` saves it, and `W` sets it as the cover. |
| Modes | Page mode | Yes | ✅ | ✅ | |
| Modes | Webtoon/continuous scroll | Yes | ✅ | ✅ | Sliced blocks, lazy decode, caching. |
| Modes | Continuous chapter scrolling | Yes | ✅ | ✅ | Previous/current/next window with boundary cards; page mode gets a blank adjacent-chapter page. |
| Modes | Left-to-right / right-to-left / vertical | Yes | ✅ | ✅ | RTL reverses left/right in page mode; paged vertical uses up/down page turns. |
| Modes | Per-series viewer mode and reset to global | Yes | ✅ | ✅ | Chapter settings store a viewer override; “Reset viewer to global” clears it. |
| Modes | Page image fit | Yes | ✅ | ✅ | Fit, fill/crop, and stretch options. |
| Zoom | Page and scroll zoom | Yes | ✅ | ✅ | `+`/`-`; page zoom crops/pads and pans with `h/l`/`j/k`, scroll zoom rebuilds rows, no upscaling at 100%. |
| Display | Crop borders | Yes | ✅ | ✅ | Uniform borders are trimmed; Reader setting. |
| Display | Color filters / grayscale | Yes | 🟡 | 🟡 | Grayscale is implemented; custom color filters remain. |
| Display | Dual-page, split wide pages, rotate wide pages | Yes | ✅ | ✅ | Paged mode supports dual spreads, half-page navigation for split wide pages, and 90° rotation. |
| Zoom | Kitty placement-only zoom with disable toggle | Yes | 🟡 | 🟡 | Page zoom works by re-encoding; placement-only scaling and its disable setting are planned. |
| Progress | Resume last page | Yes | ✅ | ✅ | Debounced save. |
| Progress | Read-state derivation | Yes | ✅ | ✅ | Page/pages. |
| Progress | Host read-state mirror | Yes | ✅ | ✅ | Remote progress/bookmarks upload in batches and merge back when local state is missing. |
| Navigation | Previous/next chapter | Yes | ✅ | ✅ | Follows filtered list. |
| Navigation | Skip read/filtered/duplicate chapters and chapter transitions | Yes | ✅ | ✅ | Skip-read and duplicate toggles, filtered navigation, and edge transitions. |
| Navigation | Open chapter page in browser | Yes | ✅ | ✅ | Reader `o` opens the remote chapter page; local chapters open their folder. |
| Bookmarks | Toggle bookmark | Yes | ✅ | ✅ | `b`. |
| Downloads | Download current chapter | Yes | ✅ | ✅ | `D`, active cancellation now works. |
| Privacy | Incognito suppresses history/time | Yes | ✅ | ✅ | Global and per-extension. |

### History and Stats

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Rows | Latest chapter per series | Yes | ✅ | ✅ | Unbounded JSONL. |
| Rows | Cover, title, chapter, source | Yes | ✅ | ✅ | Slim source line when no duration. |
| Rows | Parsed chapter number | Yes | ✅ | ✅ | `Ch. N`. |
| Rows | Saved-remote marker | Yes | ✅ | ✅ | Star. |
| Rows | Date groups | Yes | ✅ | ✅ | Today / Yesterday / date. |
| Rows | Search | Yes | ✅ | ✅ | |
| Rows | Mouse selection | Yes | ✅ | ✅ | Row and cover hit-testing. |
| Resume | Current chapter or next chapter | Yes | ✅ | ✅ | |
| Delete | Single / all-series delete | Yes | ✅ | ✅ | Clears matching progress. |
| Delete | Multi-select and clear all | Yes | ✅ | ✅ | |
| Privacy | Delete does not remove bookmarks/downloads | Yes | ✅ | ✅ | |
| Stats | Totals and today/7d/30d | Yes | ✅ | ✅ | Derived from live history. |
| Stats | 14-day chart | Yes | ✅ | ✅ | |
| Stats | Top series, streak, active days | Yes | ✅ | ✅ | |
| Stats | Unique series count | Yes | ✅ | ✅ | |
| Stats | Tracker overview and mean score | Yes | ✅ | ✅ | History → Stats and More → Statistics show linked/scored entries and the mean score. |
| Stats | Local/downloaded/total title counts | Yes | ✅ | ✅ | More → Statistics reports local, saved, and downloaded title counts. |
| Stats | Keyboard/wheel scrolling | Yes | ✅ | ✅ | |
| Time | Per-chapter and per-day duration | Yes | ✅ | ✅ | |
| Time | Idle cutoff | Yes | ✅ | ✅ | Two-minute inactivity cutoff. |
| Migration | Migrate from duplicate dialog | Yes | ✅ | ✅ | `M`. |

### Migrate

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Sources | Saved manga grouped by source | Yes | ✅ | ✅ | |
| Selection | Favorites + multi-select | Yes | ✅ | ✅ | |
| Matching | Regular title matching | Yes | ✅ | ✅ | Normalized similarity. |
| Matching | Deep matching | Yes | ✅ | ✅ | Punctuation/bracket stripping and word queries. |
| Matching | Threshold and empty-chapter rejection | Yes | ✅ | ✅ | |
| Preview | Target title/source/score | Yes | ✅ | ✅ | |
| Apply | Migrate (replace) | Yes | ✅ | ✅ | Removes old favorite/state. |
| Apply | Copy | Yes | ✅ | ✅ | Keeps original. |
| Transfer | Progress | Yes | ✅ | ✅ | Chapter number/title mapping. |
| Transfer | Bookmarks | Yes | ✅ | ✅ | |
| Transfer | History | Yes | ✅ | ✅ | |
| Transfer | Categories | Yes | ✅ | ✅ | |
| Transfer | Reading duration | Yes | ✅ | ✅ | Replace mode. |
| Transfer | Notes | Yes | ✅ | ✅ | Copied to the target story key; removable via migration options. |
| Transfer | Custom cover | Yes | ✅ | ✅ | Cached cover is copied to the target when available. |
| Apply | Per-migration flags/options | Yes | ✅ | ✅ | `z` toggles chapters, categories, notes, custom cover, and remove-old-downloads. |
| Entry | Story-level Migrate | Yes | ✅ | ✅ | `M` on story. |
| Entry | Duplicate-dialog Migrate | Yes | ✅ | ✅ | `M` in duplicate dialog. |
| Config | Migration source configuration/order | Yes | ✅ | ✅ | Target sources are selectable, reorderable, and persisted. |
| Matching | Prioritize by chapters / hide options | Yes | ✅ | ✅ | `p` searches all targets and prefers the newest chapter, `h` hides unmatched, and `u` hides targets without newer chapters. |
| Apply | Manual search override / skip | Yes | ✅ | ✅ | `↵`/`o` searches target sources and replaces the match; `s` skips an item. |
| Apply | Batch progress / cancel | Yes | ✅ | ✅ | Matching runs in the background; apply processes one manga per tick with a progress status, and Esc cancels between items while keeping completed migrations. |
| Downloads | Move/delete old downloads | Yes | ✅ | ✅ | Replace moves the old series’ downloaded chapters to the target source/series; copy leaves them in place. |

### Tracking

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Auth | AniList browser login, refresh, logout | Yes | 🟡 | 🟡 | Browser OAuth with a loopback callback and logout; AniList does not issue refresh tokens here, so expiry requires re-login. Needs a registered client ID for end-to-end verification. |
| Auth | MyAnimeList browser login, refresh, logout | Yes | 🟡 | 🟡 | Browser OAuth/PKCE with refresh and logout; not yet exercised against a registered app. |
| Services | AniList / MyAnimeList | Yes | ✅ | ✅ | Search, link/unlink, and progress sync work. |
| Services | MangaUpdates | Yes | ✅ | ✅ | Session-token login, search, link, progress/status/score sync, and backup round-trip. |
| Services | Shikimori | Yes | ✅ | ✅ | Access-token login (user-supplied), search, link, progress/status/score sync, and backup round-trip. |
| Services | Bangumi | Yes | ✅ | ✅ | Access-token login, search, link, progress/status/score/privacy sync, and backup round-trip. |
| Services | Kitsu | Yes | ✅ | ✅ | Access-token login, search, link, progress/status/score sync, and backup round-trip. |
| Services | MangaBaka | Yes | ✅ | ✅ | Access-token login, search, link, progress/status/score/privacy/date sync, and backup round-trip. |
| Services | Hikka | Yes | ✅ | ✅ | Access-token login, search with stored slug URL, link, progress/status/score/date sync, and backup round-trip. |
| Metadata | Status, score, start/end dates, private flag | Yes | ✅ | ✅ | `T` on a linked story edits status, score, dates, AniList privacy, and unlink; updates sync when logged in. |
| Overview | Tracker stats and mean score | Yes | ✅ | ✅ | More → Statistics shows linked entries, scored entries, and the mean local score. |
| Actions | Link/unlink/search and background progress sync | Yes | ✅ | ✅ | Worker-backed. |
| Verification | Live tracker service checks | Yes | 🟡 | 🟡 | Parsers, status mapping, and backup round-trips are tested; no live accounts have exercised the OAuth/token flows end to end. |

### Downloads

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Management | Folder, queue, series list, cancel, delete | Yes | ✅ | ✅ | More → Downloads. |
| Policies | Concurrent pages/sources settings | Yes | ✅ | ✅ | 1–5 chapters and 1–15 pages per chapter download at once. |
| Policies | Download ahead / auto-download while reading | Yes | ✅ | ✅ | Queues the next N chapters when a remote chapter opens. |
| Policies | Delete after read/bookmarked/excluded categories | Yes | ✅ | ✅ | Delete-after-read, keep-bookmarked, and keep-category exceptions. |
| Policies | Global/category new-chapter downloads | Yes | ✅ | ✅ | Per-series toggle plus a global policy scoped to chosen categories. |
| Format | Save chapters as CBZ | Yes | ✅ | ✅ | Optional CBZ archives; folders remain the default. |
| Queue | Pause, resume, reorder | Yes | ✅ | ✅ | `p` pauses after the active chapter; `j`/`k` select and `+`/`-` reorder pending chapters in More → Downloads. |
| Networking | Download only on Wi-Fi | Yes | ➖ | ➖ | Desktop has no metered-network concept. |

### Settings, security, and data

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Appearance | Theme / accent / fullscreen | Yes | ✅ | ✅ | Catppuccin default. |
| Appearance | Thumbnail size | Yes | ✅ | ✅ | |
| Library | Path / rescan / sort / default category | Yes | ✅ | ✅ | |
| Reader | Mode / zoom / prefetch / cache | Yes | ✅ | ✅ | |
| Browse | Host URL / NSFW extensions | Yes | ✅ | ✅ | |
| Advanced | FlareSolverr URL | Yes | ✅ | ✅ | Validated, persisted, host restart. |
| Advanced | Custom user agent | Yes | 🟡 | 🟡 | Applied to direct page/cover/image fetches; extension-source requests still use the host’s client. |
| Advanced | DNS-over-HTTPS | Yes | ❌ | ❌ | Planned. |
| Advanced | Diagnostics / verbose logging | Yes | 🟡 | 🟡 | Advanced → Diagnostics shows version, paths, host state, tracker links, queue, and log level; verbose app logging remains. |
| Notifications | Update notifications | Yes | ✅ | ✅ | Desktop notifications for local scans and saved-source refreshes. |
| Notifications | Download and tracker-failure notifications | Yes | ✅ | ✅ | Download completion/failure and tracker failures notify through the desktop. |
| Security | Global Incognito | Yes | ✅ | ✅ | |
| Security | Per-extension Incognito | Yes | ✅ | ✅ | |
| Downloads | Download settings/management page | Yes | ✅ | ✅ | Folder setting plus a downloaded-series list with sizes and per-series deletion. |
| Data | Backup import/export | Yes | ✅ | ✅ | Import and export from More → Data and storage. |
| Data | Storage statistics/cleanup | Yes | ✅ | ✅ | Sizes/counts, cover-cache cleanup, clear-all, and per-series deletion. |
| Categories | Category management screen | Yes | ✅ | ✅ | More → Categories adds, reorders with `+`/`-`, renames, and deletes; `Default` stays first. |
| Statistics | Dedicated statistics screen | Yes | ✅ | ✅ | More → Statistics shows library/chapter counts, read states, reading time, and unique series read; History → Stats remains live. |

### Documented TUI deviations

| Android feature | TUI equivalent | Status | Notes |
|---|---|---|---|
| WebView source page | Copy URL (`C`) / open URL (`o`) | ✅ | Relative source URLs resolve through installed extension metadata when available. |
| Android share sheet | Clipboard copy / open file | ✅ | `C` on story copies through the native clipboard or OSC 52; `S` opens the cached cover externally. |
| File picker | Path-entry dialog | ✅ | Custom covers and series import. |
| Swipe actions | Keys and download menu | ➖ | Intentional terminal deviation. |
| Android notifications | Desktop notifications + TUI status/queue markers | ✅ | Best-effort `notify-send`, Notification Center, or a PowerShell toast; no background daemon runs while the TUI is closed. |
| WebView-dependent source | None | ➖ | The host has no browser engine, so these sources stay unsupported. |
| Mihon database categories | Portable `categories.json` sidecar | ➖ | A filesystem library cannot share the Android app database; the sidecar keeps categories portable. |
| APK signature verification | TOFU file fingerprints | ➖ | The host cannot verify signing keys; the TUI records a SHA-256 fingerprint instead. |

### Publishing

| Area | Feature | Required | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Planning | Release plan and target matrix | Yes | Yes | ✅ | [`publishing.md`](publishing.md) defines targets, artifacts, workflows, Nix, and Cachix. |
| Licensing | MIT `LICENSE` and Cargo metadata | Yes | Yes | ✅ | `LICENSE`, workspace `license`/repository/homepage/readme/keywords/categories, and per-crate inheritance are committed. |
| Licensing | Third-party notices | Yes | Yes | ✅ | `THIRD_PARTY_NOTICES`, Apache-2.0/GPLv2+CE/UnRAR texts, and a generated Rust dependency inventory are committed. |
| Portability | Windows portability gate | Yes | Partial | 🟡 | Windows x64/ARM64 tests and the embedded-host self-test pass in the release workflow. Shared openers, clipboard, and notifications are implemented but still need a manual Windows check. |
| Host | Bundled host runtime build | Yes | Yes | ✅ | The Bash script builds the patched jar and a `jlink` JRE; `build-host-runtime.ps1` packages the published jar with a native Windows JRE. `host-v0.2.9` is published; ARM64 release packaging is untested. |
| CI | Formatting, lint, and test workflows | Yes | Yes | ✅ | `.github/workflows/ci.yml` passes fmt, clippy, MSRV, notices, and tests on Linux, macOS, and Windows x86_64/ARM64. |
| CI | Host jar and per-target runtime artifacts | Yes | Yes | ✅ | `host-v0.2.9` published the patched jar and checksum; the v0.2.0 release built all six native host runtimes. |
| Release | Six-target binary matrix | Yes | Yes | ✅ | The v0.2.0 release produced all six Linux/macOS/Windows x86_64/ARM64 archives. |
| Release | SHA-256 checksums | Yes | Yes | ✅ | v0.2.0 published a verified `SHA256SUMS` for all six archives. |
| Release | GitHub release automation | Yes | Yes | ✅ | v0.2.0 was published automatically as a pre-release from a tag. |
| Nix | Flake package and app outputs | Yes | Yes | ✅ | v0.2.0 built `packages.default`/`apps.default`/`checks.default` on x86_64-linux, aarch64-linux, x86_64-darwin, and aarch64-darwin. x86_64-darwin pins `nixpkgs-26.05-darwin`. |
| Nix | Cachix binary cache | Yes | Yes | ✅ | Public `tsuiyomi` cache contains all four v1.0.0 closures; the flake carries the substituter and public key. |
| Release | `v0.2.0` test release | Yes | Yes | ✅ | Published as a GitHub pre-release with six archives and `SHA256SUMS`. |
| Release | `v1.0.0` public release | Yes | Yes | ✅ | Published as latest with six archives and verified `SHA256SUMS`. |
| Docs | Release and install documentation | Yes | Yes | ✅ | `INSTALL.md` covers archive layout, checksum verification, unsigned-binary warnings, data paths, source builds, and release packaging. |
| Distribution | Code signing | No | No | ➖ | Deliberately omitted; unsigned archives with SHA-256 checksums only. |

## Verification snapshot

| Check | Result |
|---|---|
| Core tests | 151 pass, 1 ignored |
| TUI tests | 229 pass |
| Binary integration test | 1 pass (`backup-info`) |
| Windows target check | `cargo check --target x86_64-pc-windows-gnu` passes; runtime CI is still pending |
| MSRV | Rust 1.90.0 `cargo check --workspace --locked` passes |
| Clippy | Passes with `-D warnings` |
| Release build | Passes |
| Bundled host | Patched, rebuilt, embedded archive |
| Live checks | Extension jobs, not-loaded delete, story filtering, downloads, cover overlay, history, categories, read-state mirroring |
| Docs audit | 2026-11-24: stale phase checklists archived; parity tables reconciled with plan; Publishing category added |
| CI | 2026-11-25: green on Linux, macOS, and Windows x86_64/ARM64; Windows path assertions fixed; one macOS arm64 runner-capacity failure passed on rerun |
| Host jar | 2026-11-25: `host-v0.2.9` published with `miwayomi-0.2.9-patched.jar` and SHA-256 `c9779f95…47c` |
| v0.2.0 | 2026-11-25: pre-release published six archives, `SHA256SUMS`, and four Nix package builds |
| Cachix | 2026-11-25: all four v1.0.0 closures verified in `tsuiyomi.cachix.org` |
| v1.0.0 | 2026-11-25: latest release published with six archives and `SHA256SUMS` |
| Nix package | Flake evaluation succeeds for the four systems; derivation build and Cachix push are pending |
| Portability | 2026-11-24: shared opener, `java.exe`/`PATHEXT`, `taskkill`, reserved filenames, native clipboard, PowerShell notifications, Windows runtime package |
| Licensing | 2026-11-25: MIT license, workspace metadata, changelog, third-party notices, license texts, dependency inventory |

## Remaining work

The parity tables above are the status source of truth. Remaining gaps:

1. Live verification for AniList/MyAnimeList OAuth and the token-paste tracker services.
2. A real Android Mihon backup restore; `backup-info` and the import report cover the
   pre-import half.
3. Kitty placement-only zoom with its disable toggle and re-encode fallback.
4. DNS-over-HTTPS in the host transport.
5. Page-level download concurrency (chapter/source concurrency is implemented).
6. Custom reader color filters (grayscale is implemented).
7. Verbose application logging and a diagnostics log view.
8. Windows CI execution and release automation; tracked in [publishing.md](publishing.md).

## Historical record

The original development phases and dependency notes are archived in
[history.md](history.md). They are retained for design context only; unchecked boxes in
that archive are not current gaps.
