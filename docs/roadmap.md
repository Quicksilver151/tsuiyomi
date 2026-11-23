# Roadmap

All parity feature tables live below, with category plans under [`parity/`](parity/).
Scores are generated from the tables by `scripts/parity-score.py`.

<!-- parity-summary:start -->

## Parity completion

| Category | ✅ | 🟡 | ❌ | ➖ | Score |
|---|---:|---:|---:|---:|---:|
| [On-disk compatibility](#on-disk-compatibility) | 24 | 1 | 0 | 1 | 98.0% |
| [Library](#library) | 24 | 2 | 1 | 0 | 92.6% |
| [Browse, sources, and global search](#browse-sources-and-global-search) | 31 | 0 | 0 | 1 | 100.0% |
| [Extensions and host lifecycle](#extensions-and-host-lifecycle) | 23 | 0 | 0 | 2 | 100.0% |
| [Shared story / chapter screen](#shared-story--chapter-screen) | 46 | 0 | 0 | 1 | 100.0% |
| [Reader](#reader) | 22 | 1 | 1 | 0 | 93.8% |
| [History and Stats](#history-and-stats) | 21 | 0 | 0 | 0 | 100.0% |
| [Migrate](#migrate) | 23 | 0 | 0 | 0 | 100.0% |
| [Tracking](#tracking) | 4 | 2 | 1 | 0 | 71.4% |
| [Downloads](#downloads) | 6 | 0 | 1 | 1 | 85.7% |
| [Settings, security, and data](#settings-security-and-data) | 16 | 0 | 2 | 0 | 88.9% |
| [Documented TUI deviations](#documented-tui-deviations) | 4 | 0 | 0 | 4 | 100.0% |
| **All categories** | **244** | **6** | **6** | **10** | **96.5%** |

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
| Backups | Tracking entries | Yes | ✅ | ✅ | Field 18 is parsed and exported for AniList and MyAnimeList links. |
| Backups | Viewer/chapter flags and update strategy | Yes | ✅ | ✅ | Viewer mode, chapter filters/sort, update strategy, and next update are preserved; the first two also apply to chapter settings. |
| Backups | Excluded scanlators and next update | Yes | ✅ | ✅ | Fields 108 and 113 are parsed, applied, and exported. |
| Backups | Real Mihon restore verification | Yes | 🟡 | 🟡 | Protobuf round-trip passes; no Android Mihon restore has been performed. |

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
| Sorting | Other Mihon sort fields | Yes | 🟡 | 🟡 | Chapter count, latest chapter, unread, recently read, recently added, and random. Tracker score, last update, and chapter fetch date remain. |
| Sorting | Per-category sort and display settings | Yes | ❌ | ❌ | Sort and display are global. |
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
| Reading | Mark duplicate chapters read | Yes | 🟡 | 🟡 | Marking chapters read also marks same-number chapters; new-chapter rules remain. |
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
| Sources | Source preferences | Yes | ✅ | ✅ | Switch, text, list; multi-select waits on host arrays. |
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
| Header | Copy/share URL | Yes | ✅ | ✅ | OSC 52 clipboard. |
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
| Actions | Tag copy | Yes | ✅ | ✅ | `Y` copies the story tags through OSC 52. |
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
| Modes | Left-to-right / right-to-left / vertical | Yes | ✅ | ✅ | RTL reverses left/right in page mode; paged vertical uses up/down page turns. |
| Modes | Per-series viewer mode and reset to global | Yes | ✅ | ✅ | Chapter settings store a viewer override; “Reset viewer to global” clears it. |
| Modes | Page image fit | Yes | ✅ | ✅ | Fit, fill/crop, and stretch options. |
| Zoom | Page and scroll zoom | Yes | ✅ | ✅ | `+`/`-`; page zoom crops/pads and pans with `h/l`/`j/k`, scroll zoom rebuilds rows, no upscaling at 100%. |
| Display | Crop borders | Yes | ✅ | ✅ | Uniform borders are trimmed; Reader setting. |
| Display | Color filters / grayscale | Yes | ✅ | ✅ | Grayscale toggle; custom color filters remain. |
| Display | Dual-page, split wide pages, rotate wide pages | Yes | ❌ | ❌ | Still planned. |
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
| Services | Kitsu, MangaUpdates, Shikimori, Bangumi, Hikka, MangaBaka | Yes | ❌ | ❌ | Planned tracker adapters. |
| Metadata | Status, score, start/end dates, private flag | Yes | ✅ | ✅ | `T` on a linked story edits status, score, dates, AniList privacy, and unlink; updates sync when logged in. |
| Overview | Tracker stats and mean score | Yes | ✅ | ✅ | More → Statistics shows linked entries, scored entries, and the mean local score. |
| Actions | Link/unlink/search and background progress sync | Yes | ✅ | ✅ | Worker-backed. |

### Downloads

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Management | Folder, queue, series list, cancel, delete | Yes | ✅ | ✅ | More → Downloads. |
| Policies | Concurrent pages/sources settings | Yes | ❌ | ❌ | One chapter at a time. |
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
| Advanced | Custom user agent | Yes | ❌ | ❌ | Planned. |
| Advanced | DNS-over-HTTPS | Yes | ❌ | ❌ | Planned. |
| Advanced | Diagnostics / verbose logging | Yes | ✅ | ✅ | Advanced → Diagnostics shows version, paths, host state, tracker links, queue, and log level. |
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
| Android share sheet | OSC 52 clipboard copy / open file | ✅ | `C` on story; `S` opens the cached cover externally. |
| File picker | Path-entry dialog | ✅ | Custom covers and series import. |
| Swipe actions | Keys and download menu | ➖ | Intentional terminal deviation. |
| Android notifications | Desktop notifications + TUI status/queue markers | ✅ | Best-effort `notify-send`/Notification Center; no background daemon runs while the TUI is closed. |
| WebView-dependent source | None | ➖ | The host has no browser engine, so these sources stay unsupported. |
| Mihon database categories | Portable `categories.json` sidecar | ➖ | A filesystem library cannot share the Android app database; the sidecar keeps categories portable. |
| APK signature verification | TOFU file fingerprints | ➖ | The host cannot verify signing keys; the TUI records a SHA-256 fingerprint instead. |

## Verification snapshot

| Check | Result |
|---|---|
| Core tests | 108 pass, 1 ignored |
| TUI tests | 161 pass |
| Clippy | Passes with `-D warnings` |
| Release build | Passes |
| Bundled host | Patched, rebuilt, embedded archive |
| Live checks | Extension jobs, not-loaded delete, story filtering, downloads, cover overlay, history, categories, read-state mirroring |

## Highest-impact remaining work

The current implementation plan is [`plan.md`](plan.md). In priority order:

1. Browser OAuth for AniList/MAL, remaining tracker services, metadata, and tracker stats.
2. Library layouts, filters, badges, sort extras, smart updates, and continue reading.
3. Updates settings, tracker stats, and complete title-count breakdowns.
4. Migrate notes, custom covers, and per-migration flags.
5. Download policies, CBZ output, and queue controls.
6. Per-series viewer mode, browser-open actions, reader page/skip/display features, and
   Kitty placement zoom with an opt-out.
7. Remaining backup fields and real Mihon restore verification.
8. Advanced networking, diagnostics, and desktop notifications.
9. Novel mode based on LNReader (backlog only).

## Development phases

The phases below are historical implementation phases. The current scope and acceptance
criteria live in [`plan.md`](plan.md).

Principles:

- Manga first, Mihon-compatible on disk.
- `tsuiyomi-core` is UI-agnostic; `tsuiyomi-tui` only renders and handles input.
- Every phase ends with `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test` green.
- Small commits; refactors separate from behavior changes.

---

## Phase 0 — Foundation (done)

- [x] Nix devshell with direnv and `.env.local`.
- [x] `justfile` with run/check/fmt/clippy/test.
- [x] Dependency upgrade (`ratatui 0.30`, `crossterm 0.29`, `confy 2.0`).
- [x] Remove dead modules and warnings.
- [x] Document the Mihon compatibility contract in `AGENTS.md`.

## Phase 1 — Workspace split

- [x] Convert the root `Cargo.toml` into a workspace (`resolver = "2"`).
- [x] Create `crates/tsuiyomi-core` (library scan, metadata, config, errors).
- [x] Create `crates/tsuiyomi-tui` (ratatui widgets, app state, input).
- [x] Create `crates/tsuiyomi` (thin binary that wires core + tui).
- [x] Move `usr::Config` and `utils::files` into core; move `ui` into tui.
- [x] Rename the legacy `Anime`/`anime` identifiers to `Series`/`library`.
- [x] Keep behavior identical; only move code.

**Done when:** `cargo run` shows the same list, `tsuiyomi-core` has no ratatui/crossterm
dependency, and no `anime` identifiers remain.

## Phase 2 — Domain model and scanner

- [x] Define core types: `Library`, `Series`, `Chapter`, `ChapterKind`
      (directory/archive).
- [x] Add `Page` and page discovery for folders and `zip`/`cbz` archives.
- [ ] Add `SeriesId`/`ChapterId` when the reader lands.
- [x] Scan `local/`: one series per direct subdirectory, skip hidden entries, sort
      case-insensitively and naturally.
- [x] Discover chapters: subdirectories plus `.zip`/`.cbz`, `.rar`/`.cbr`, `.7z`/`.cb7`,
      `.tar`/`.cbt`, and `.epub`.
- [x] Derive chapter titles from directory names or archive stems.
- [x] Parse chapter numbers from names (numbers, `c001`, `v2`, decimals, ranges,
      specials). Put this in its own module with heavy tests.
- [x] Discover pages in a directory (image extensions, natural sort).
- [x] List pages inside `zip`/`cbz`, `rar`/`cbr`, `7z`/`cb7`, and `tar`/`cbt` archives
      (ignore archive-internal directories).
- [x] Extract image pages from EPUB spine XHTML (Mihon-style; prose is not rendered).
- [x] Collect per-series errors instead of failing the whole scan.
- [x] Add fixture-based tests using `tempfile`.

**Done when:** scanning a fixture library returns the expected series, chapters, numbers,
and page order, and a broken folder does not abort the scan.

## Phase 3 — Metadata

- [x] Parse legacy `details.json` (`title`, `author`, `artist`, `description`, `genre`,
      `status`).
- [x] Parse series-level `ComicInfo.xml` with `quick-xml`.
- [x] Parse chapter-level `ComicInfo.xml` from folders and `zip`/`cbz`, `rar`/`cbr`,
      `7z`/`cb7`, and `tar`/`cbt` archives (title, number, scanlator, date).
- [x] Read EPUB chapter metadata from the OPF and extract spine images.
- [x] Implement precedence: `ComicInfo.xml` > `details.json` > directory name.
- [x] Detect `cover.*`; expose the path and render it in the TUI details pane.
- [x] Generate `cover.jpg` from the first page of the first chapter when no valid cover
      exists (Mihon-compatible, capped at 512 px).
- [x] Treat malformed metadata as a warning attached to the series, not a failure.

**Done when:** metadata fixtures produce the same values Mihon would show, and malformed
XML/JSON leaves the series usable.

## Phase 4 — Library UI

- [x] Replace the anime panel with a series list + detail pane (title, author, tags,
      description, chapter count).
- [x] Add a status bar (library path, counts, errors, key hints).
- [ ] Move scanning to a background thread; post results through a channel as app events.
- [x] Show loading, empty, and error states.
- [x] Add refresh (`r`); the incremental title filter (`/`) is still missing.
- [x] Add `c` config shortcut and `f` fullscreen (hides title and commands boxes).
- [x] Keep selection stable across refresh.
- [x] Add a category tab row inside Library, navigated with `[` and `]`.
- [x] Add mouse support: click tabs, click a series, scroll the list.
- [x] Add TestBackend rendering tests for loaded and empty/error states.
- [x] Browse the built-in Local source and open its local series in Library.
- [x] Cover tile grids for Library and Browse Local source, with Browse-local series
      details, chapters, and reader; Browse never switches to the Library tab.

**Done when:** the TUI is responsive while scanning and handles empty or broken libraries
without panics.

## Phase 5 — Reader

- [x] Chapter list view per series, sorted by parsed chapter number.
- [x] Open a series with Enter, open a chapter, back out with Esc.
- [x] Open a chapter and build the page list from core.
- [x] Render image pages with `ratatui-image` (Kitty/Sixel/iTerm2, halfblocks fallback).
- [x] Page navigation with `←`/`→`, `PageUp`/`PageDown`, and `Space`.
- [x] Next/previous chapter with `n`/`p`.
- [ ] Fit modes and reading direction (LTR/RTL) in config.
- [x] Persist last-read page per chapter and resume on reopen.
- [x] Continuous vertical scroll mode (`m` toggles page/scroll; arrows/wheel scroll 5 lines).
- [x] Scroll zoom with `+`/`-` (50–300%); no upscaling at 100%.
- [x] Expose scroll cache/prefetch settings in config and the More → Settings section.
- [ ] Expose worker count / idle prefetch in config and settings.
- [x] Reader bottom bar with prev/next chapter buttons and a page gauge (tabs hidden).
- [x] Handle unsupported or corrupt images without aborting.

**Done when:** a chapter can be read end-to-end, progress survives restart, and image
protocol fallback works over SSH.

## Phase 6 — Config and history

- [x] Define the config schema (library path, reader settings) and store it under the XDG
      config dir.
- [x] Preserve unrelated config keys already present on user machines
      (`max_http_requests_per_sec`, `player.*`, `manga_viewer`, `tty.graphics`). Never drop
      unknown keys on save; unknown `theme` values fall back to the default theme.
- [ ] Add CLI flags with `clap`: `--library`, `--config`, `--version`.
- [ ] Precedence: CLI > env (`TSUIYOMI_LIBRARY`) > config file > default.
- [x] Add an editable settings section in More, with thin slider gauges.
- [x] Add selectable themes, named/custom accents with live preview, and a startup
      fullscreen option.
- [x] Mirror Mihon's Settings categories (Appearance, Library, Reader, Downloads,
      Tracking, Browse, Data and storage, Security, Advanced, About), with library sort,
      default category, Incognito progress suppression, and explicit placeholders for
      unsupported Android/remote features.
- [ ] Store read history/progress under the XDG data dir.
- [ ] Start with a JSON store; migrate to SQLite only if it becomes slow.
- [ ] Add config/history migration handling and corrupt-file recovery.

**Done when:** settings and progress survive restarts, and a corrupt config falls back to
defaults with a visible warning.

## Phase 7 — Mihon backups

- [x] Define protobuf structs matching Mihon's `@ProtoNumber` models.
- [x] Detect optional gzip (magic `1f 8b`) and read `BackupManga` entries.
- [x] Import: match local folders by name/title and remote manga by source/title;
      restore read state, bookmarks, and history.
- [x] Report unmatched manga instead of silently dropping them.
- [x] Export: produce `.tachibk` files with local and remote entries.
- [ ] Add round-trip tests with a real-backup fixture (redacted).

**Done when:** a backup exported by Mihon imports with correct progress, and an exported
backup restores in Mihon.

## Phase 8 — Updates and remote sources

- [x] Local updates: rescan the library and detect new chapters.
- [x] Updates tab lists new chapters since the last scan.
- [x] Remote sources: run Mihon extensions through the bundled JVM host.
- [x] Remote search, browsing, and reading through that host.

**Done when:** new local chapters show up in the Updates tab without rescanning manually.

## Phase 9 — Polish and release

- [ ] Structured logging with `tracing`, honoring `TSUIYOMI_LOG`.
- [ ] Friendly error dialogs and a panic hook that restores the terminal.
- [ ] README with screenshots/asciinema, install instructions, and a support matrix.
- [ ] Add a license.
- [ ] CI: `fmt --check`, `clippy -D warnings`, tests, and `nix flake check`.
- [ ] Add a `packages.default` output to the flake.
- [ ] Publish `tsuiyomi-core` and the binary to `crates.io`.
- [ ] Tag `v1.0.0`.

---

## Suggested dependency additions

Add these only in the phase that needs them:

| Phase | Crate |
| --- | --- |
| 1 | `thiserror` (core errors) |
| 2 | `natord` or `alphanumeric-sort`, `walkdir`, `zip`, `unrar`, `sevenz-rust2`, `tar` |
| 3 | `quick-xml`, `serde_json` |
| 5 | `ratatui-image`, `image` |
| 6 | `directories`, `clap`, `toml` |
| 7 | `prost`, `flate2` |
| 8 | source-specific HTTP/HTML crates |
| 9 | `tracing`, `tracing-subscriber` |
