# Mihon Parity Status

Last updated: **2026-11-21**

This is the single status table for Mihon parity. Update the affected rows whenever a
feature lands, changes behavior, or is intentionally deferred.

Related detail plans:

- [`browse-parity-plan.md`](browse-parity-plan.md)
- [`history-parity-plan.md`](history-parity-plan.md)
- [`story-parity-plan.md`](story-parity-plan.md)
- [`plan.md`](plan.md)

Legend:

- ✅ implemented and verified
- 🟡 partial, limited, or a TUI-specific equivalent
- ❌ not implemented
- ➖ intentionally not applicable to a terminal application

## 1. Mihon on-disk compatibility

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
| Chapters | `.epub` | Yes | Yes | 🟡 | Image targets from spine XHTML only; prose is not rendered. |
| Chapters | Number recognition | Yes | Yes | ✅ | Numbers, `v2`, `c001`, ranges, specials. |
| Chapters | Natural page ordering | Yes | Yes | ✅ | Case-insensitive natural sort. |
| Metadata | `ComicInfo.xml` series metadata | Yes | Yes | ✅ | Preferred over legacy metadata. |
| Metadata | `details.json` fallback | Yes | Yes | ✅ | Legacy `title`, `author`, `artist`, `description`, `genre`, `status`. |
| Metadata | Directory-name fallback | Yes | Yes | ✅ | Metadata failure never hides a series. |
| Metadata | Chapter `ComicInfo.xml` | Yes | Yes | ✅ | Directory and archive chapters. |
| Metadata | EPUB OPF metadata | Yes | Yes | ✅ | `dc:title`, creator/publisher, date. |
| Covers | `cover.jpg` custom cover | Yes | Yes | ✅ | Other valid `cover.*` images also work. |
| Covers | Generated first-page cover | Yes | Yes | ✅ | Best effort, capped at 512 px longest edge. |
| Categories | Mihon database categories | Yes | 🟡 | 🟡 | `categories.json` in the library root; unlisted series use `Default`. |
| Metadata writes | Mihon-readable metadata export | Yes | ❌ | ❌ | No `ComicInfo.xml`/`details.json` writing. |
| Backups | Import `.tachibk` / `.proto.gz` | Yes | ❌ | ❌ | Not implemented. |
| Backups | Export Mihon-restorable backups | Yes | ❌ | ❌ | Not implemented. |

## 2. Library

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Grid | Cover tile grid | Yes | Yes | ✅ | Adjustable tile width. |
| Grid | Generated/placeholder covers | Yes | Yes | ✅ | Initials placeholder when no image. |
| Grid | Saved source manga merged in | Yes | Yes | ✅ | Mirrored in `remote_library`. |
| Categories | Category tabs/filtering | Yes | ✅ | ✅ | `[`/`]`; default category. |
| Categories | Edit categories from story | Yes | ✅ | ✅ | Local and saved source manga; local assignments write `categories.json`. |
| Sorting | Natural / title A–Z / Z–A | Yes | ✅ | ✅ | Persisted `library_sort`. |
| Sorting | Other Mihon sort fields | Yes | ❌ | ❌ | Last-read, unread count, date added, etc. |
| Search | Library title filter | Yes | ✅ | ✅ | `/` or `s` filters the Library grid; Esc clears. |
| Updates | Updates tab | Yes | ❌ | ❌ | Tab is a placeholder. |
| Removal | Remove local series, keep files | Yes | ✅ | ✅ | Path stored in `hidden_series`. |
| Removal | Remove saved source manga + downloads | Yes | ✅ | ✅ | Delete dialog with optional downloads. |
| Covers | Persistent cover cache | Yes | ✅ | ✅ | LRU, 512 px, atomic writes, Kitty reset. |
| Covers | Automatic cover recovery | Yes | ✅ | ✅ | Missing saved covers trigger host refill. |
| Selection | Multi-select and batch delete | Yes | ✅ | ✅ | Stable keys and command-bar count. |
| Scan | Background scan | Yes | 🟡 | 🟡 | Refresh/rescan runs on a worker; the initial startup scan still runs before the UI. |
| Scan | Import browser / file picker | Yes | ❌ | ❌ | Drop folders into `local/` and rescan. |

## 3. Browse, sources, and global search

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
| Stores | Store name / website / counts | Yes | 🟡 | 🟡 | Rows show URL, extension count, and installed count; no website metadata. |
| Networking | Background Browse calls | Yes | 🟡 | 🟡 | Result pages, source lists, favorites, filters, preferences, extension lists, migration matching, and remote details/chapters fetch on workers; mutations and migration apply still block. |
| Search | Extension source URL/id search terms | Yes | 🟡 | 🟡 | Installed source ids/base URLs; repo-only entries expose names/languages. |
| Search | Debounced extension search | Yes | ❌ | ❌ | Local filtering is immediate. |

## 4. Extensions and host lifecycle

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
| Trust | Signature verification / trust action | Yes | ❌ | ❌ | Host cannot verify signatures; intentionally not faked. |
| Uninstall | Confirmation and batch uninstall | Yes | ✅ | ✅ | |
| Details | Package, version, store, sources | Yes | ✅ | ✅ | |
| Details | Enable/disable all sources | Yes | ✅ | ✅ | |
| Details | Per-source toggle | Yes | ✅ | ✅ | |
| Details | Per-source preferences | Yes | ✅ | ✅ | |
| Details | Clear cookies | Yes | ✅ | ✅ | Fixed against the 1.6 source API. |
| Details | Per-extension Incognito | Yes | ✅ | ✅ | |
| Warnings | NSFW/content warning indicator | Yes | ✅ | ✅ | `18+` in rows and details. |
| Warnings | Hide NSFW extensions | Yes | ✅ | ✅ | Settings → Browse. |
| Warnings | Private/shared/obsolete indicators | Yes | 🟡 | 🟡 | Private (installed outside a repo) and obsolete flags shown; shared/signature states remain. |
| Languages | Enabled-language filtering | Yes | ✅ | ✅ | Installed entries remain manageable. |
| Search | Comma-separated extension terms | Yes | ✅ | ✅ | Matches names, packages, languages, source names. |
| WebView | WebView-dependent sources | Yes | ❌ | ❌ | Unsupported by design. |

## 5. Shared story / chapter screen

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Shared screen | Library local / saved remote / Browse / History | Yes | ✅ | ✅ | One story component set. |
| Header | Cover, title, author, artist, source | Yes | ✅ | ✅ | |
| Header | Status, tags, description | Yes | ✅ | ✅ | |
| Header | Chapter count and latest chapter | Yes | ✅ | ✅ | |
| Header | Notes | Yes | ✅ | ✅ | `N`, persisted per story. |
| Header | Missing-chapter count | Yes | ✅ | ✅ | |
| Header | Source/stub warning | Yes | ❌ | ❌ | |
| Header | Next update / fetch interval | Yes | ❌ | ❌ | Host does not expose scheduling metadata. |
| Header | Add/remove library | Yes | ✅ | ✅ | `F`. Remove confirmation exists; downloads checkbox is Library-level. |
| Header | Edit categories | Yes | ✅ | ✅ | Saved source manga; `c`. |
| Header | Migrate from story | Yes | ✅ | ✅ | `M`, preloads Migrate. |
| Header | Tracking action | Yes | ❌ | ❌ | Separate integration milestone. |
| Header | Copy/share URL | Yes | ✅ | ✅ | OSC 52 clipboard. |
| Header | Open source page | Yes | 🟡 | 🟡 | Opens absolute URLs or local folders; relative source URLs need copy. |
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
| Downloads | Series download queue screen | Yes | ❌ | ❌ | Queue is transient; no dedicated screen. |
| Actions | Copy/open, cover overlay | Yes | ✅ | ✅ | `C`, `o`, `w`. |
| Actions | Custom cover | Yes | ✅ | ✅ | Local and saved source manga; `W`. |
| Actions | Remote cover share/delete | Yes | ❌ | ❌ | Replace/remove only. |
| Actions | Tag search | Yes | ✅ | ✅ | `t` picker. |
| Actions | Tag copy | Yes | ❌ | ❌ | |
| Gestures | Swipe to read/bookmark/download | Yes | ❌ | ❌ | TUI uses keys and menu actions. |

## 6. Reader

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Formats | Folders, zip/cbz, rar/cbr, 7z/cb7, tar/cbt | Yes | ✅ | ✅ | |
| Formats | Image EPUB | Yes | ✅ | ✅ | Prose ignored. |
| Images | Kitty / Sixel / iTerm2 | Yes | ✅ | ✅ | Auto-detected. |
| Images | Unicode halfblock fallback | Yes | ✅ | ✅ | |
| Modes | Page mode | Yes | ✅ | ✅ | |
| Modes | Webtoon/continuous scroll | Yes | ✅ | ✅ | Sliced blocks, lazy decode, caching. |
| Modes | Left-to-right / right-to-left / vertical | Yes | 🟡 | 🟡 | RTL reverses page-turn keys in page mode; vertical still uses paged mode. |
| Modes | Page image fit | Yes | ✅ | ✅ | Fit, fill/crop, and stretch options. |
| Zoom | Page and scroll zoom | Yes | ✅ | ✅ | `+`/`-`, no upscaling at 100%. |
| Zoom | Retransmit-free Kitty placement zoom | Yes | ❌ | ❌ | Pending. |
| Progress | Resume last page | Yes | ✅ | ✅ | Debounced save. |
| Progress | Read-state derivation | Yes | ✅ | ✅ | Page/pages. |
| Navigation | Previous/next chapter | Yes | ✅ | ✅ | Follows filtered list. |
| Bookmarks | Toggle bookmark | Yes | ✅ | ✅ | `b`. |
| Downloads | Download current chapter | Yes | ✅ | ✅ | `D`, active cancellation now works. |
| Privacy | Incognito suppresses history/time | Yes | ✅ | ✅ | Global and per-extension. |

## 7. History and Stats

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
| Stats | Keyboard/wheel scrolling | Yes | ✅ | ✅ | |
| Time | Per-chapter and per-day duration | Yes | ✅ | ✅ | |
| Time | Idle cutoff | Yes | ✅ | ✅ | Two-minute inactivity cutoff. |
| Migration | Migrate from duplicate dialog | Yes | ✅ | ✅ | `M`. |

## 8. Migrate

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
| Entry | Story-level Migrate | Yes | ✅ | ✅ | `M` on story. |
| Entry | Duplicate-dialog Migrate | Yes | ✅ | ✅ | `M` in duplicate dialog. |
| Config | Migration source configuration/order | Yes | ❌ | ❌ | |
| Matching | Prioritize by chapters / hide options | Yes | ❌ | ❌ | |
| Apply | Manual search override / skip | Yes | ❌ | ❌ | |
| Apply | Batch progress / cancel | Yes | 🟡 | 🟡 | Matching runs in the background with a status; applying is synchronous and cancellation drops the result. |
| Downloads | Move/delete old downloads | Yes | ❌ | ❌ | |

## 9. Settings, security, and data

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Appearance | Theme / accent / fullscreen | Yes | ✅ | ✅ | Catppuccin default. |
| Appearance | Thumbnail size | Yes | ✅ | ✅ | |
| Library | Path / rescan / sort / default category | Yes | ✅ | ✅ | |
| Reader | Mode / zoom / prefetch / cache | Yes | ✅ | ✅ | |
| Browse | Host URL / NSFW extensions | Yes | ✅ | ✅ | |
| Advanced | FlareSolverr URL | Yes | ✅ | ✅ | Validated, persisted, host restart. |
| Security | Global Incognito | Yes | ✅ | ✅ | |
| Security | Per-extension Incognito | Yes | ✅ | ✅ | |
| Downloads | Download settings/management page | Yes | 🟡 | 🟡 | Configurable downloads folder; no per-series download list/queue screen yet. |
| Tracking | Tracker services | Yes | ❌ | ❌ | Placeholder. |
| Data | Backup import/export | Yes | ❌ | ❌ | |
| Data | Storage statistics/cleanup | Yes | ❌ | ❌ | |
| Categories | Category management screen | Yes | ✅ | ✅ | More → Categories adds/renames/deletes; no reordering yet. |
| Statistics | Dedicated statistics screen | Yes | 🟡 | 🟡 | History → Stats mini-tab. |

## 10. Documented TUI deviations

| Android feature | TUI equivalent | Status | Notes |
|---|---|---|---|
| WebView source page | Copy URL (`C`) / open absolute URL (`o`) | 🟡 | Relative source URLs cannot be opened externally without a base URL. |
| Android share sheet | OSC 52 clipboard copy | ✅ | `C` on story. |
| File picker | Path-entry dialog | ✅ | Custom covers. |
| Swipe actions | Keys and download menu | ➖ | Intentional terminal deviation. |
| Tracker OAuth dialogs | None | ❌ | Separate integration. |
| Android notifications | TUI status bar/queue markers | ➖ | No background daemon notifications. |

## 11. Verification snapshot

| Check | Result |
|---|---|
| Core tests | 102 total: 101 pass, 1 ignored |
| TUI tests | 141 pass |
| Clippy | Passes with `-D warnings` |
| Release build | Passes |
| Bundled host | Patched, rebuilt, embedded archive |
| Live checks | Extension jobs, not-loaded delete, story filtering, downloads, cover overlay, history, categories |

## 12. Highest-impact remaining work

1. Background Browse/host networking so source lists, results, details, and chapters do
   not block the UI.
2. Read-state mirroring back to the host.
3. Optional remote-WebView bridge for WebView-only sources.
4. Tracking integration.
5. Extension signature trust, private/shared/obsolete indicators, and store metadata.
6. Reader direction/fit modes and retransmit-free Kitty zoom.
7. Backup import/export.
8. Library title filtering and a background scanner.
