# Browse Parity Plan

## 1. Goal

Bring tsuiyomi's Browse experience to functional parity with Mihon's Browse tab:
Sources, source browsing and results, global search, Extensions, and Migrate.
This includes the settings and shared screens that Browse depends on, such as
saving to the library, categories, source filters, source preferences, extension
details, and the manga/chapter screen reached from Browse results.

The plan is written against the current Mihon sources under
`app/src/main/java/eu/kanade/tachiyomi/ui/browse/`,
`app/src/main/java/eu/kanade/presentation/browse/`, and
`app/src/main/java/mihon/feature/migration/`.

## Implementation status

Completed in the first Browse-parity slice:

- Host-client source capability field (`supports_latest`, defaulting to true for
  older hosts).
- Browse config keys: `last_used_source`, `enabled_languages`, and
  `disabled_sources`.
- Source ordering by last used, pinned, Local, then language.
- Source management screen (`o`) for enabling/disabling languages and individual
  sources.
- Disabled sources and languages hidden from Browse and pinned global searches.
- Latest listing hidden for sources that report `supportsLatest = false`.
- Remote result pagination with `n`, deduplicated by manga URL and preserving the
  selected tile.
- Local source title search, recent/latest view, and persisted title ordering.
- Extension update detection, pending count in the Browse tab title, and `U` update-all.
- Background global search with progressive per-source results, failure tolerance, and
  pinned/all source scope.
- Source preferences screen (`C`) for switch, text, and list values using the host's
  existing preferences API.
- Source filters (`z`) with checkbox, tri-state, text, select, sort, nested groups, reset,
  and apply. The host patch adds `GET /filters` and filtered `POST /search`.
- Result display modes (`v`), in-library markers, and saved-result hiding (`h`).
- Duplicate-title detection, add-anyway/open dialogs, remove confirmation, and saved
  remote categories.
- Extension grouping and details with per-source toggles, bulk enable/disable, source
  preferences, per-extension Incognito, clear-cookies, and uninstall.
- Global-search empty/error source rows and the persisted only-with-results filter.

Still open from M0/M1: the Browse extraction and background worker, source
options dialog, source preferences, Local source search/latest/sort, and the
remaining M2+ work.


## 2. Scope and definition of done

"Parity" means every Browse action in Mihon has a usable terminal equivalent,
including the same persisted state and failure handling. Desktop equivalents may
use different key bindings, dialogs, or layout, but may not silently omit a
capability.

### In scope

- Sources list: last used, pinned, language groups, Local source, pinning,
  disabling, language filtering, source options, latest shortcuts, source
  preferences.
- Source browsing: popular, latest, search, filters, pagination, display modes,
  retry/empty states, in-library badges, hiding saved items, saving/removing,
  duplicate detection, category selection, migration from duplicate dialog.
- Local source: title search, latest-by-modified-time, sort order, help.
- Global search: pinned/all sources, progressive per-source results, loading and
  error rows, only-with-results toggle, open-source group action, cancel/retry,
  migration from results.
- Extensions: update detection and badge, update all, grouped sections, search,
  install/update/cancel/retry, uninstall, not-loaded/trusted states, extension
  details, per-source toggles, enable/disable all, clear cookies, per-extension
  incognito, content warnings, language filtering, extension stores.
- Migrate: source favorite counts and sorting, favorite selection, source
  configuration and ordering, automatic/deep matching, manual matching,
  migrate/copy/skip, progress/cancel/exit, and migration of the data tsuiyomi
  actually owns.
- Shared package with Browse: remote story screen, chapter filtering/search/
  sorting, download queue, saved-manga categories, and Browse-related settings.
- Persistence, background networking, tests, and documentation.

### Deliberate deviations

These are Android platform features with no meaningful terminal equivalent, or
features blocked by the current headless-host contract. They do not count as
missing Browse functionality:

- Android package-installer permission prompts and per-app install permissions.
  Extension installs run entirely inside the host.
- The Android "App info" system settings screen.
- Device-credential authentication for NSFW content warnings. The TUI uses an
  explicit confirmation setting instead.
- Embedding a WebView. Sources that require a WebView remain unsupported, and
  "Open in WebView" becomes "open the source home page in the user's browser"
  where useful. Literal WebView parity is a separate browser-engine project, not
  part of this plan.
- Android share sheets and haptic feedback.

### Done criteria

- Every item in the parity checklist in section 9 is checked.
- No Browse network call blocks input or rendering.
- Browse preferences survive restarts and unknown config keys are preserved.
- Core, host-client, and TestBackend tests cover every new parser, matcher,
  persistence path, and screen.
- A manual matrix with Local, no-latest, filtered, configurable, paginated,
  failing, and migration-heavy sources passes.
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace` pass.
- README and AGENTS.md describe the new behavior.

## 3. Current baseline

| Area | Current tsuiyomi | Gap |
|---|---|---|
| Sources | Pinned, Local, language groups; pin/multi-select | No last-used, disabling, language filter, source options, latest shortcut, source prefs |
| Source browse | Page 1 grid; popular, latest, modal search | No pagination, filters, listing indicator, display modes, retry states |
| Saving | `F`, batch save, always Default | No in-library badges, hide-saved, duplicates, category choice, remove confirmation |
| Local source | All local series, title sort | No search, latest, or sort controls |
| Global search | Pinned only, synchronous, successful non-empty groups | No all/pinned, progress, errors, only-results, cancel, group open |
| Extensions | Merged installed/available, install/uninstall/update, search, stores | No update detection/badge/update-all, groups, details, prefs, cookies, trust, content warnings |
| Migrate | Placeholder | Entire feature missing |
| Story screen | Cover/metadata/chapters/reader/download/bookmark | No chapter filters/search/sort, saved categories, download queue |
| Settings | Minimal Browse page | Missing hide-saved, content warnings, language/source management, source display mode |

## 4. Implementation principles

- Keep `tsuiyomi-core` UI-agnostic. Put filtering, update detection, duplicate
  matching, migration matching, and persistence in core.
- Keep refactors separate from behavior changes. The first commit of each
  milestone may move code; the following commits add behavior.
- All host calls that can take more than a few milliseconds go through a worker
  with requests, results, progress, and cancellation generations.
- Persist Browse preferences in `Config`, not in hidden runtime files. New keys
  must use serde defaults and must not discard unknown keys.
- Do not create a private replacement for the Mihon library layout. Remote
  categories, progress, and history are tsuiyomi metadata, not on-disk Mihon
  files.
- Treat source and extension failures as data. A failed source in global search,
  a failed filter list, or a failed preference load renders an error row and
  remains retryable.
- Every new Browse action gets a conflict-free key. Existing global bindings
  such as `f` (fullscreen), `s`/`/` (search), and `F` (favorite) must not be
  overloaded. The final key map belongs in the commands bar and README.

## 5. Architecture changes

### 5.1 Split Browse out of `app.rs`

`app.rs` is already large. Before adding parity features, extract Browse without
changing behavior:

- `crates/tsuiyomi-tui/src/browse/mod.rs`
- `crates/tsuiyomi-tui/src/browse/state.rs` — `BrowseState`, selections, grids,
  listing/search/extension/migration state.
- `crates/tsuiyomi-tui/src/browse/actions.rs` — commands and state transitions.
- `crates/tsuiyomi-tui/src/browse/draw.rs` — existing `draw_browse*` and new
  screens.
- Keep shared helpers (`draw_story_*`, `grid_*`, `panel`) in `draw.rs`.

This is a mechanical refactor only: same key bindings, same output, same tests.
It keeps later diffs reviewable and makes the new screens testable in isolation.

### 5.2 Background Browse worker

Add `crates/tsuiyomi-tui/src/browse_worker.rs` using `std::thread` + `mpsc`,
matching the existing host/story/cover worker pattern.

Request types:

- `LoadSources`
- `LoadListing { source, listing, query, page, filters }`
- `LoadFilters { source }`
- `LoadSourcePrefs { source }`
- `SetSourcePrefs { source, values }`
- `GlobalSearch { generation, scope, query, sources }`
- `CancelGlobalSearch { generation }`
- `LoadExtensionRepo { repo }`
- `InstallExtension { repo, apk }`
- `UninstallExtension { pkg }`
- `InstallJobCancel { job }`
- `LoadMigrationMatch { item, sources, deep, flags }`
- `MigrateBatch { items }`
- `LoadStory`, `LoadChapters`, and download/queue requests

Response types carry the request ID or generation. Stale responses are dropped.
Global search uses at most five concurrent source requests, matching Mihon's
`Dispatchers.IO.limitedParallelism(5)`.

The UI thread only mutates state from received events and never performs HTTP.

### 5.3 Core modules

Add or extend:

- `crates/tsuiyomi-core/src/filters.rs`
  - Transport `FilterList` tree and state mutation helpers.
  - Types: header, separator, checkbox, tri-state, text, select, sort, group.
  - Serialization/deserialization from the host contract.
- `crates/tsuiyomi-core/src/source_prefs.rs`
  - Transport source preference definitions and values.
  - Supported types: switch, text, list, multi-select.
- `crates/tsuiyomi-core/src/browse.rs`
  - Source ordering: last-used, pinned, language, Local under `other`.
  - Enable/disable source and language predicates.
  - Local source listing: title search, latest threshold, sort order.
  - Duplicate candidate normalization and comparison.
- `crates/tsuiyomi-core/src/extensions.rs`
  - Merge installed/available entries without losing installed versions.
  - Update detection, obsolete/private classification, content-warning and
    language filtering.
- `crates/tsuiyomi-core/src/migration.rs`
  - Regular and deep title matching.
  - Candidate scoring with normalized Levenshtein (threshold `0.4`, same as
    Mihon `BaseSmartSearchEngine`).
  - Chapter matching by recognized chapter number.
  - Migration flag handling and data rewrite plans.
- Extend `host.rs` with the new endpoints and typed errors.
- Extend `config.rs` with Browse preferences and remote categories.

### 5.4 Config additions

Add flat keys to `Config` (serde defaults, preserved unknown keys):

```toml
last_used_source = "pkg|name|lang"
enabled_languages = ["en"]
disabled_sources = ["pkg|name|lang"]
source_display_mode = "comfortable"   # list | comfortable | compact
browse_hide_saved = false
global_search_scope = "pinned"        # pinned | all
global_search_only_results = false
incognito_extensions = []
enabled_content_warnings = ["safe", "mixed", "nsfw"]
migration_sources = []
migration_sorting = "alphabetical"    # alphabetical | total
migration_direction = "ascending"
migration_deep_search = false
migration_prioritize_chapters = false
migration_hide_unmatched = false
migration_hide_without_updates = false
migration_flags = ["chapters", "categories"]
local_source_sort = "title_asc"
```

Do not store transient state (loading, selection, current page) in config.

Semantics:

- Empty `enabled_languages` means all languages, so a fresh install never hides
  sources. A non-empty list restricts Sources and available extensions.
- Empty `disabled_sources` means nothing is hidden.
- Empty `migration_sources` means use the default selection rules from Mihon:
  pinned sources if any, otherwise enabled sources.
- Empty `categories` on a saved remote entry means `Default`.

### 5.5 Saved remote entries and categories

`RemoteFavorite` becomes the on-disk record for saved remote manga. Extend it
with serde defaults:

```rust
pub struct RemoteFavorite {
    // existing fields
    #[serde(default)]
    pub categories: Vec<String>,   // empty means Default
    #[serde(default)]
    pub saved_at: Option<i64>,
    #[serde(default)]
    pub last_read_url: Option<String>,
    #[serde(default)]
    pub last_read_name: Option<String>,
}
```

Remote categories remain tsuiyomi-local. The Library grid merges local and
remote entries under their configured categories. The host remains the source of
truth for favorite membership only.

## 6. Host API contract additions

The bundled host is patched in `scripts/host-patches/` and rebuilt by
`scripts/build-host-runtime.sh`. All additions must keep the existing endpoints
working for older hosts.

### 6.1 Source capabilities

Extend `SourceDto`:

```json
{
  "id": "123",
  "name": "Example",
  "lang": "en",
  "type": "manga",
  "pkg": "en.example",
  "supportsLatest": true,
  "configurable": false
}
```

`supportsLatest` comes from `CatalogueSource.supportsLatest`; `configurable` from
`ConfigurableSource` / `ConfigurableAnimeSource`.

### 6.2 Source filters

Add:

```http
GET /api/v1/manga/{sourceId}/filters
```

Response:

```json
{
  "filters": [
    { "type": "header", "name": "Tags" },
    { "type": "checkbox", "name": "Completed", "state": false },
    { "type": "tristate", "name": "Adult", "state": 0 },
    { "type": "text", "name": "Author", "state": "" },
    { "type": "select", "name": "Order", "state": 0, "values": ["Popular", "Latest"] },
    { "type": "sort", "name": "Sort", "state": { "index": 0, "ascending": true }, "values": ["Name", "Date"] },
    { "type": "group", "name": "More", "children": [] }
  ],
  "error": null
}
```

If `getFilterList()` throws, return an empty list plus a non-fatal warning, the
same way the current search endpoint falls back to an empty filter list.

Add a filtered listing endpoint. A POST body avoids URL-length limits:

```http
POST /api/v1/manga/{sourceId}/search
Content-Type: application/json

{
  "page": 1,
  "query": "",
  "filters": [ ...same tree... ]
}
```

Response is the existing `MangasPageDto` shape. Keep the current GET endpoints
unchanged. Popular and latest ignore filters, matching Mihon's `GetRemoteManga`.

The host reconstructs a `FilterList` in the same order and with the same nested
groups, applies the states, then invokes `getPopularManga`, `getLatestUpdates`,
or `getSearchManga`.

### 6.3 Source preferences

The host already has:

```http
GET  /api/v1/sources/{sourceId}/prefs
POST /api/v1/sources/{sourceId}/prefs
```

Use it. Extend the POST body to accept booleans, strings, numbers, and string
arrays so multi-select preferences work:

```json
{ "preferred_language": "en", "show_nsfw": false, "genres": ["action", "drama"] }
```

Reject unknown keys or return a partial-success report. Refresh the preference
list after saving because extensions can have dependent preferences.

### 6.4 Extension metadata and lifecycle

Extend the installed/repo entry DTO with optional fields:

```json
{
  "versionCode": 12,
  "store": "Keiyoushi",
  "storeUrl": "https://...",
  "contentWarning": "safe",
  "isObsolete": false,
  "isShared": true,
  "signature": "sha256..."
}
```

Add update detection inputs. The comparison is done in core, but the host must
preserve installed `version`, `versionCode`, and `signature` instead of
overwriting them during the installed/repo merge.

Add not-loaded reporting:

```http
GET /api/v1/extensions/not-loaded
```

Response entries carry `pkg`, `name`, `reason` (`untrusted`, `filtered`,
`unsigned`, `unsupported`, `malformed`, `failed`), `message`, and
`hasUpdate`. Add trust only if the host can verify signatures safely:

```http
POST /api/v1/extensions/trust  { "pkg": "..." }
```

If signature verification cannot be implemented reliably, keep the not-loaded
list and uninstall action, and mark trust as a documented host limitation rather
than pretending it works.

Add cookie clearing:

```http
POST /api/v1/sources/{sourceId}/cookies/clear
```

It removes cookies for the source base URL and home URL using the host's
`NetworkHelper.cookieJar`, mirroring `ExtensionDetailsViewModel.clearCookies`.

Add install/update progress and cancellation:

```http
POST /api/v1/extensions/install?async=true
-> { "jobId": "..." }

GET  /api/v1/extensions/jobs/{jobId}
-> { "state": "pending|downloading|installing|done|error|cancelled",
     "progress": 0.0, "error": null }

POST /api/v1/extensions/jobs/{jobId}/cancel
```

Keep the blocking install endpoint for compatibility. The TUI uses the job API
when available and falls back to a blocking install with an indeterminate
spinner.

### 6.5 Extension stores

Mihon stores have name, index URL, website, Discord, and a count. If the host's
repository index exposes metadata, return it from `/api/v1/extensions/repos`;
otherwise return URL-only entries and let the TUI show/copy/delete the URL.
Store creation/deletion already exists through `POST /extensions/repos`.

### 6.6 Host tests

Add a host smoke script or Kotlin tests that cover:

- A source with checkbox/tristate/text/select/sort/group filters.
- A filter list that throws.
- Popular, latest, and search with filters.
- A configurable source with switch/text/list/multi preferences.
- Cookie clearing.
- Installed/repo version merge.
- Async install success, failure, progress, and cancellation.
- Not-loaded reason reporting.

## 7. Milestones

Each milestone is independently shippable and should end with tests and docs.

### M0 — Foundations

**Tasks**

- Extract Browse state/actions/draw code without behavior changes.
- Add `browse_worker` with request IDs, generations, and cancellation.
- Move global search and source pagination onto the worker first; migrate the
  remaining Browse calls during the later milestones.
- Add config fields and persistence tests.
- Add source capability fields to the host DTO and client.
- Add `filters.rs`, `source_prefs.rs`, `browse.rs`, `extensions.rs`, and
  `migration.rs` module skeletons with typed data.
- Add test fixtures for paginated, filtered, configurable, failing, and
  long-running sources.

**Acceptance**

- Browse opens and navigates while source listing, search, or extension work is
  in flight.
- Cancelling a worker request discards its response.
- Old configs load unchanged; new configs round-trip without dropping keys.
- Existing tests pass without behavior changes after the extraction.

**Effort:** L. **Risk:** medium (large refactor).

### M1 — Sources tab parity

**Tasks**

- Source ordering: last used, pinned, language groups, then sources with no
  language; Local uses the `other` language group like Mihon.
- Record last-used source when it is opened, unless Incognito is active.
- Source options dialog: pin/unpin, enable/disable, open latest when supported,
  open source preferences when configurable, open home page in browser.
- Source filter screen: language toggles, individual source toggles, bulk
  enable/disable, disabled sources hidden from Browse and global search.
- Show "Latest" on the source row when supported.
- Show loading, empty, host-starting, and failed-to-fetch states with retry.
- Source search/global-search entry: `/` and reselecting Browse opens global
  search (Mihon `BrowseTab.onReselect`).
- Local source: title search, latest-by-modified-time (seven-day window, like
  `LocalSource.LATEST_THRESHOLD`), title/recent ascending/descending sort, help
  note.

**Acceptance**

- Disabling a language hides all its sources and excludes them from global
  search.
- Disabling one source hides it without affecting the extension.
- Last-used source appears first after restart.
- Local source search, latest, and sort match the expected folder set.
- A source with `supportsLatest = false` has no latest action.
- Multi-select source pinning still works.

**Effort:** M. **Risk:** low.

### M2 — Source filters and source preferences

**Tasks**

- Implement the filter tree transport and host filter endpoint.
- Add a conflict-free filter dialog (global `f` remains fullscreen; use
  `Ctrl+F` or another documented key) with:
  - Header/separator rows.
  - Checkbox toggles.
  - Tri-state cycles ignore/include/exclude.
  - Text input.
  - Select list picker.
  - Sort selection plus ascending/descending.
  - Nested groups with expand/collapse.
  - Reset and Apply actions.
- Show a Filter chip/action only when the source has filters.
- Keep filter state while switching listing modes; reset only on explicit reset.
- Send current filters with popular/latest/search requests.
- Add a source preferences screen driven by the host:
  - Switch rows.
  - Text rows.
  - Single-select rows.
  - Multi-select rows.
  - Apply/reset and reload on save.
- Show preference load/save errors inline without closing the screen.

**Acceptance**

- Every filter type round-trips and changes the request.
- Reset restores source defaults.
- A source whose filter list throws shows "filters unavailable" and still
  browses.
- A configurable source's preferences persist across host restarts.
- No browse action is lost when switching between popular, latest, and search.

**Effort:** L. **Risk:** medium-high (host serialization).

### M3 — Source results parity

**Tasks**

- Listing header: source, listing mode, query, page/end state, result count.
- Pagination:
  - Fetch next page when the last row becomes visible or `n` is pressed.
  - Append and deduplicate by source + URL.
  - Keep selection stable across appends.
  - Stop on `has_next_page = false`; show loading and end states.
- Display modes: comfortable grid, compact grid, list; persist the choice.
- Search toolbar behavior: query retained on the results screen; Back clears a
  non-empty query before leaving (matching `BrowseSourceScreen.navigateUp`).
- Empty/error states: retry, open home page, help; Local source help.
- In-library badge and dimmed cover for saved results.
- `browse_hide_saved`: filter saved results out when enabled.
- Save/remove:
  - Favorites are checked against the loaded list.
  - Add uses the default category if one is set; otherwise shows a category
    chooser; zero categories means Default.
  - Remove shows a confirmation on the detail screen or a remove dialog on a
    grid selection.
- Duplicate detection:
  - Detect the same normalized title in saved remote entries.
  - Show a duplicate dialog with cover, chapter count, source, "open", "migrate
    to this entry", "add anyway", and cancel.
  - Warn when the title matches an existing local series; do not silently add a
    duplicate.
- Source latest shortcut behavior from M1 remains.
- Keep batch save/remove as a tsuiyomi extra.

**Acceptance**

- A source with multiple pages can be browsed to the end.
- Appending does not move or clear the current selection.
- Saved results are marked and can be hidden.
- Adding with no default category opens category selection.
- Duplicate dialog options work, including migration.
- Back from a searched result clears the query first.
- List and compact modes render and navigate correctly in TestBackend.

**Effort:** L. **Risk:** medium.

### M4 — Global search parity

**Tasks**

- Replace the synchronous loop with `browse_worker` and generations.
- Add Pinned/All scope, persisted; default pinned like Mihon.
- Add "only show sources with results" toggle, persisted.
- Per-source rows:
  - Loading spinner.
  - Success with result count.
  - Error with message and retry.
  - Empty row when only-results is off.
- Progressive updates and a progress indicator `finished / total`.
- Sort groups following Mihon's comparator: empty results last, pinned first,
  then source name/language.
- Open source with query from the group header.
- Cancel previous search when a new query starts.
- Reuse previous results when only the scope changes for the same query.
- Retry failed sources without rerunning successful ones.
- Add migration action from a result once M6 exists.
- Preserve multi-select batch saving and the existing detail navigation.

**Acceptance**

- Results appear before all sources finish.
- A failing source shows an error row and does not hide other results.
- Changing Pinned/All reruns or reuses results correctly.
- Starting a new search cancels the old one.
- Enter on a group header opens the source with the query filled in.
- Search remains responsive with dozens of pinned sources.

**Effort:** M. **Risk:** medium.

### M5 — Extensions parity

**Tasks**

- Preserve installed version/versionCode/signature separately from repo data.
- Detect updates and sort updates first.
- Group extensions: Updates pending, Not loaded, Installed, Available by
  language.
- Extension tab title shows the update count.
- Update all for the updates section.
- Search terms: comma-separated, matching extension name, source name, source
  base URL, and source id, debounced.
- Row metadata: store, language, version, source count; status pills for
  pending/downloading/installing/error/obsolete/content warning/private.
- Install/update:
  - Async job with progress and cancel.
  - Retry on error.
  - Fallback to blocking install when the host is old.
- Uninstall confirmation for private/shared extensions, batch preserved.
- Not-loaded section and dialogs:
  - Reason and message.
  - Trust when the host can verify signatures.
  - Uninstall.
- Extension details screen:
  - Icon/name/package/version/language/store.
  - Uninstall and app-info equivalent (show package/path instead).
  - Enable all / disable all sources.
  - Per-source toggle.
  - Per-source preferences action.
  - Clear cookies.
  - Per-extension incognito.
  - Content warning label and description.
  - Store website/index URL copy.
- Extension language filtering through enabled languages.
- Content-warning filtering and display.
- Extension stores screen: add, delete, copy index URL, refresh, open website,
  and store counts where the host exposes metadata.
- Settings → Browse: hide saved items, allowed content warnings, extension
  stores.

**Acceptance**

- An installed extension with a newer repo version shows an update and can be
  updated from the update section or update all.
- An up-to-date extension does not show an update.
- Install can be cancelled mid-flight without leaving a broken extension.
- A not-loaded extension shows why, and can be retried/trusted/uninstalled as
  supported by the host.
- Enabling/disabling a source immediately changes the Sources list.
- Source preferences opened from extension details persist.
- Clear cookies succeeds for an HTTP source.
- Incognito for one extension suppresses progress/history for its sources only.
- Content warnings can hide NSFW extensions.

**Effort:** XL. **Risk:** high (host lifecycle, signatures, trust).

### M6 — Migrate parity

**Tasks**

- Migrate source list:
  - Sources with saved favorites and counts.
  - Sort alphabetically/by count, ascending/descending.
  - Help/guide text.
- Migrate manga list:
  - Favorites for the chosen source.
  - Multi-select with selected background/check markers.
  - Open a title's detail without leaving selection mode.
  - Continue button appears when there is a selection.
- Migration source configuration:
  - Select all, none, enabled, pinned.
  - Order sources; order is respected by automatic matching.
  - Persist in `migration_sources`.
- Matching engine:
  - Regular search: exact title query, normalized similarity.
  - Deep search: remove bracket text, punctuation, chapter references, try the
    cleaned title, two largest words, largest word, first two words, first word.
  - Threshold `0.4`, normalized Levenshtein, highest score wins.
  - Exclude the same source+URL.
  - Fetch target chapters and reject matches with zero chapters.
  - Prioritize by latest chapter when configured.
- Migration list:
  - Searching / success / not found states.
  - Target cover, title, source, chapter count, latest chapter.
  - Manual search override.
  - Skip, migrate now, copy now.
  - Hide unmatched / hide without updates options.
  - Progress count and completion state.
- Batch actions:
  - Migrate (replace) or copy.
  - Progress dialog, cancel, exit confirmation.
  - Per-item failures do not stop the batch.
- Data migration:
  - Add target to saved remote library with chosen categories.
  - Preserve source display name, covers, story cache.
  - Copy reading progress, bookmarks, and history by chapter number.
  - Mark chapters at or below the old maximum read chapter as read.
  - Copy categories.
  - Optionally delete old downloads.
  - Optionally remove the old favorite after replacement.
  - Clear obsolete target caches as needed.
- Duplicate dialog and global-search result actions can invoke migration.

**Acceptance**

- A favorite can be migrated from source A to source B with progress and
  bookmarks preserved.
- Copy leaves the original in the library; migrate removes it when requested.
- Manual search overrides the automatic match.
- Unmatched entries can be skipped or hidden.
- Cancelling a batch leaves completed items migrated and no partial corrupt
  entries.
- Migration works for manga not currently in the tsuiyomi library if the remote
  entry is known.
- A 50-item batch shows progress and finishes without blocking the UI.

**Effort:** XL. **Risk:** high (matching quality, data mapping, host metadata).

### M7 — Shared story and chapter parity

This milestone covers screens reached from Browse results. It is shared with the
Library and should be scheduled with Library work where possible.

**Tasks**

- Chapter filters: unread, downloaded, bookmarked, scanlator.
- Chapter search by title/number.
- Chapter sort: source order, chapter number, date.
- Chapter display modes: list/compact.
- Download queue: queue a series, show progress, cancel, delete downloads.
- Saved remote categories and category editing.
- Remote notes field if notes are added to the data model.
- Custom cover handling if remote covers are added.
- Trackers are explicitly out of this milestone; they require separate service
  authentication and are not a Browse-tab capability. Leave a documented
  placeholder rather than a dead action.

**Acceptance**

- Chapter filters compose (for example downloaded + unread).
- Search and sort survive leaving/returning to the story screen within a session.
- A series download queue can be started, cancelled, and inspected.
- Saved remote categories appear in the Library grid and can be edited.
- No Browse route reaches a dead tracker/notes action.

**Effort:** L. **Risk:** medium.

### M8 — Hardening and release

**Tasks**

- Audit every Browse network call for blocking and cancellation.
- Add bounded caches for source listings, filters, prefs, repo data, and
  migration results.
- Add retry/backoff for cover, detail, and thumbnail lookups already covered by
  the existing cover retry rules.
- Add keyboard help/commands for every screen.
- Add empty/loading/error rendering tests.
- Add config migration tests and preserve unknown keys.
- Update README, AGENTS.md, and `plan.md`.
- Run the manual matrix.
- Add release notes and known limitations.

**Acceptance**

- All automated checks pass.
- The manual matrix passes on a release binary with the bundled host.
- The app remains responsive during global search, pagination, install, and
  migration.
- Documentation matches behavior and lists deliberate deviations.

**Effort:** M. **Risk:** low-medium.

## 8. Detailed parity checklist

Use this as the tracking list for "full parity". Every row must be checked or
explicitly marked as a deliberate deviation.

### Sources

- [ ] Last-used source group and persistence.
- [ ] Pinned group.
- [ ] Language groups, including `other`.
- [ ] Local source grouped according to its language or dedicated position.
- [ ] Source pin/unpin from the row and options dialog.
- [ ] Source disable/enable from the options dialog.
- [ ] Language enable/disable screen.
- [ ] Individual source enable/disable screen.
- [ ] Disabled sources excluded from global search.
- [ ] Disabled languages excluded from available extensions.
- [ ] Latest shortcut only when supported.
- [ ] Open source preferences when configurable.
- [ ] Open source home page in browser.
- [ ] Loading, empty, host-starting, and error states.
- [ ] Global search action and Browse-tab reselect.
- [ ] Local source title search.
- [ ] Local source recent/latest view.
- [ ] Local source sort ascending/descending.
- [ ] Local source help.

### Source browsing

- [ ] Popular listing.
- [ ] Latest listing.
- [ ] Search listing.
- [ ] Query retained and Back clears query first.
- [ ] Listing mode/query/result-count header.
- [ ] Filter action only when filters exist.
- [ ] Filter types: header, separator, checkbox, tri-state, text, select, sort,
      group.
- [ ] Filter reset/apply.
- [ ] Filters sent for popular, latest, and search.
- [ ] Pagination and append behavior.
- [ ] Loading and end-of-list indicators.
- [ ] Retry on error.
- [ ] Empty-state retry, open-home, help.
- [x] Comfortable grid, compact grid, list modes.
- [x] Persisted display mode.
- [x] In-library badge.
- [x] Dimmed saved covers (TUI: ✓ plus dim title/status).
- [x] Hide saved results setting.
- [x] Add to library.
- [x] Remove from library confirmation.
- [x] Default category behavior.
- [x] Category chooser when no default is configured.
- [x] Duplicate detection and dialog.
- [x] Add anyway.
- [x] Open duplicate.
- [ ] Migrate duplicate.
- [x] Source preferences screen.
- [x] Batch save/remove preserved as a tsuiyomi extra.

### Global search

- [x] Pinned-only scope.
- [x] All-sources scope.
- [x] Persisted scope.
- [x] Only-with-results toggle.
- [x] Persisted only-with-results.
- [ ] Per-source loading row.
- [ ] Per-source success row.
- [ ] Per-source empty row when unfiltered.
- [x] Per-source error row and retry.
- [x] Progress count.
- [x] Progressive result updates.
- [x] Cancel previous search.
- [ ] Reuse results for same query/scope changes.
- [x] Group ordering (pinned first, empty last, name/language).
- [x] Open source with query from group header.
- [x] Result cards/navigation.
- [x] Save results.
- [ ] Migrate from results.

### Extensions

- [x] Installed version preserved separately.
- [x] Update detection.
- [x] Update count badge.
- [x] Updates section.
- [x] Update all.
- [x] Not-loaded section.
- [x] Not-loaded reason inspection (status bar, not a modal).
- [ ] Trust extension where supported.
- [x] Installed section.
- [x] Available section grouped by language.
- [ ] Obsolete/private indicators.
- [x] Content-warning indicators.
- [x] Search by name/source/URL/id (installed source ids/base URLs; repo-only rows expose names/languages).
- [x] Comma-separated search terms.
- [ ] Debounced search.
- [x] Install.
- [x] Update.
- [x] Cancel install.
- [x] Retry failed install.
- [x] Install progress.
- [x] Uninstall with confirmation.
- [x] Batch install/uninstall/update preserved.
- [x] Extension details screen.
- [x] Enable all sources.
- [x] Disable all sources.
- [x] Per-source toggle.
- [x] Per-source preferences.
- [x] Clear cookies.
- [x] Per-extension incognito.
- [x] Content warning dialog/info.
- [x] Extension stores create/delete; copy/open/refresh remain.
- [x] Settings → Browse: hide saved and content warnings; stores screen remains.

### Migration

- [ ] Source list with favorite counts.
- [ ] Sort by name/count and direction.
- [ ] Select favorites.
- [ ] Multi-select and selection UI.
- [ ] Open target/current details.
- [ ] Migration source selection: all/none/enabled/pinned.
- [ ] Source reordering.
- [ ] Persisted source selection/order.
- [ ] Regular matching.
- [ ] Deep matching.
- [ ] Similarity threshold.
- [ ] Same source/URL exclusion.
- [ ] Empty-chapter rejection.
- [ ] Prioritize by latest chapter.
- [ ] Hide unmatched.
- [ ] Hide without updates.
- [ ] Manual search override.
- [ ] Skip.
- [ ] Migrate now.
- [ ] Copy now.
- [ ] Batch migrate.
- [ ] Batch copy.
- [ ] Progress.
- [ ] Cancel.
- [ ] Exit confirmation.
- [ ] Chapter/progress migration.
- [ ] Bookmark migration.
- [ ] History migration.
- [ ] Category migration.
- [ ] Custom cover migration where applicable.
- [ ] Notes migration where applicable.
- [ ] Old download deletion flag.
- [ ] Old favorite removal on replace.
- [ ] Duplicate-dialog migration integration.
- [ ] Global-search migration integration.

### Shared story and chapter screen

- [ ] Chapter filter by unread.
- [ ] Chapter filter by downloaded.
- [ ] Chapter filter by bookmarked.
- [ ] Chapter filter by scanlator.
- [ ] Chapter search.
- [ ] Chapter sort.
- [ ] Chapter display mode.
- [ ] Series download queue.
- [ ] Download progress/cancel/delete.
- [ ] Saved remote categories.
- [ ] Category editing.
- [ ] No dead tracker/notes actions.

### Cross-cutting

- [ ] Browse worker and cancellation.
- [ ] No UI-thread network calls.
- [ ] Persistence and unknown-key preservation.
- [ ] Error surfaces for every network path.
- [ ] Multi-select preserved.
- [ ] Keyboard help/commands.
- [ ] Core tests.
- [ ] Host tests.
- [ ] TUI rendering tests.
- [ ] Manual matrix.
- [ ] README/AGENTS/plan updates.

## 9. Test plan

### Core tests

- Filter tree serialization/deserialization and state updates for every type.
- Source ordering with last-used, pinned, languages, empty language, and Local.
- Enable/disable language and source predicates.
- Local listing search/latest/sort.
- Extension merge preserving installed version and detecting updates.
- Content warning and language filtering.
- Duplicate normalization and candidate detection.
- Migration deep-search cleaning and query generation.
- Normalized Levenshtein scoring and threshold behavior.
- Chapter matching by number, including specials and missing numbers.
- Migration flags produce the expected rewrite plan.
- Progress/bookmark/history data copying.
- Config round-trip and unknown-key preservation.
- Remote categories round-trip.

### Host-client tests

- New source capability fields.
- Filter endpoint success and throwing-filter fallback.
- Filtered browse POST for all three listings.
- Source prefs GET/POST, including multi-select arrays and errors.
- Extension installed/repo merge and metadata.
- Not-loaded list.
- Async install job success/progress/failure/cancel.
- Cookie clearing success/failure.
- Pagination `has_next_page`.
- Error DTO and Cloudflare propagation.

### TUI tests

- Browse extraction keeps existing tests green.
- Sources: ordering, disable/language filters, last-used, options dialog,
  latest availability, settings persistence.
- Source browse: pagination append, selection stability, filters, query clearing,
  empty/error actions, display modes, in-library badges, duplicate/category
  dialogs.
- Global search: progressive rows, errors, retry, cancel, scope changes,
  only-results, group opening.
- Extensions: update badge/groups/update all, search terms, install progress,
  cancel, details, source toggles, prefs, cookies, incognito, content warnings.
- Migrate: selection, source config, matching states, manual override, batch
  progress/cancel, data preservation.
- Story: chapter filters/search/sort/queue.
- Render each screen at narrow and normal terminal sizes.

### Manual matrix

- Local library with nested folders, CBZ/RAR/7z/TAR/EPUB chapters.
- Remote source with popular/latest/search.
- Remote source without latest.
- Remote source with filters of every type.
- Remote source with configurable preferences.
- Source returning multiple pages.
- Source returning an empty page and a network error.
- Extension with no update, update available, install failure, cancel.
- Extension with multiple sources, one disabled.
- Two sources with the same manga for migration, regular and deep matches.
- A migration with progress/bookmarks/history and categories.
- Global search with pinned sources, all sources, one failure, zero results.
- Save duplicate manga and add anyway.
- Restart after each persisted change.

## 10. Files likely touched

### Core

- `crates/tsuiyomi-core/src/config.rs`
- `crates/tsuiyomi-core/src/host.rs`
- `crates/tsuiyomi-core/src/library.rs`
- `crates/tsuiyomi-core/src/progress.rs`
- `crates/tsuiyomi-core/src/bookmarks.rs`
- `crates/tsuiyomi-core/src/history.rs`
- `crates/tsuiyomi-core/src/downloads.rs`
- `crates/tsuiyomi-core/src/filters.rs` (new)
- `crates/tsuiyomi-core/src/source_prefs.rs` (new)
- `crates/tsuiyomi-core/src/browse.rs` (new)
- `crates/tsuiyomi-core/src/extensions.rs` (new)
- `crates/tsuiyomi-core/src/migration.rs` (new)
- `crates/tsuiyomi-core/src/lib.rs`

### TUI

- `crates/tsuiyomi-tui/src/app.rs` (shrinks as Browse moves out)
- `crates/tsuiyomi-tui/src/lib.rs` (input routing)
- `crates/tsuiyomi-tui/src/draw.rs` (shared helpers)
- `crates/tsuiyomi-tui/src/browse/mod.rs` (new)
- `crates/tsuiyomi-tui/src/browse/state.rs` (new)
- `crates/tsuiyomi-tui/src/browse/actions.rs` (new)
- `crates/tsuiyomi-tui/src/browse/draw.rs` (new)
- `crates/tsuiyomi-tui/src/browse_worker.rs` (new)
- `crates/tsuiyomi-tui/src/host_worker.rs` (reuse patterns)
- `crates/tsuiyomi-tui/src/palette.rs` only if new semantic colors are needed

### Host and scripts

- `scripts/host-patches/miwayomi-0.2.9.patch`
- `scripts/build-host-runtime.sh`
- Host Kotlin routes, DTOs, mappers, extension manager, and source preference
  handling under the vendored host source tree.
- `scripts/host-smoke.sh` or an equivalent host test entry point (new)

### Documentation

- `README.md`
- `AGENTS.md`
- `plan.md`
- `browse-parity-plan.md` (this file)

## 11. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Filter serialization diverges from extension expectations | Use a single recursive DTO, round-trip tests, and a throwing-filter fallback. |
| Host patch maintenance becomes difficult | Keep changes small and documented; add smoke tests; keep old endpoints working. |
| Browse state extraction destabilizes existing behavior | Mechanical refactor first, no behavior changes, existing tests must stay green. |
| Global search overloads the host or UI | Five concurrent requests, generations, cancellation, bounded result state. |
| Pagination duplicates or jumps selection | Deduplicate by source/URL, append-only, preserve selected key rather than index. |
| Extension trust/signature support is unrealistic in the host | Implement not-loaded reporting first; mark trust as a documented limitation if signature verification cannot be safe. |
| Migration matching picks bad targets | Show target preview, manual override, similarity threshold, chapter-count checks, and skip/undo paths. |
| Migration corrupts progress/bookmarks | Migrate through a pure rewrite plan, test with fixtures, write atomically, and never delete old data until the target write succeeds. |
| Categories for remote manga break Library filters | Add serde defaults, migrate empty categories to Default, and test both local and remote grids. |
| Async worker introduces stale responses | Request IDs/generations; discard stale events; one owner per state slice. |
| WebView-only sources remain unsupported | Keep the documented project limitation; do not promise WebView parity in this plan. |

## 12. Suggested commit sequence

1. `refactor: extract browse state and drawing`
2. `host: add source capabilities and filtered browse endpoints`
3. `core: add filter and source preference models`
4. `core: add browse source ordering and local listing`
5. `tui: add source management and source filter screen`
6. `tui: add source filters and source preferences`
7. `tui: add source result pagination and display modes`
8. `tui: add library duplicate/category dialogs`
9. `tui: add background global search`
10. `host: add extension metadata, jobs, cookies, and not-loaded reporting`
11. `core: add extension update detection and merge rules`
12. `tui: add extension groups, updates, details, and source toggles`
13. `core: add migration matching and data rewrite`
14. `tui: add migration source/list/config/batch flows`
15. `tui: add chapter filters and series download queue`
16. `docs: document browse parity and known limitations`

## 13. Open decisions

These should be decided before M5/M6 implementation:

1. **WebView parity.** Keep the current headless-host limitation, or start a
   separate browser-engine project? This plan assumes the limitation stays.
2. **Extension trust.** Implement signature verification in the host, or ship
   not-loaded reporting and uninstall without trust? Trust cannot be faked.
3. **Remote categories storage.** Keep them in `Config` beside
   `remote_library`, or introduce a small JSON store for remote library
   metadata? Config is simpler and matches the current architecture.
4. **Tracker/notes scope.** Treat them as shared manga-screen features outside
   this Browse plan, or expand M7 into full MangaScreen parity. This plan treats
   them as outside Browse, with no dead actions.
5. **Migration cover/notes.** Remote custom covers and notes do not exist in
   tsuiyomi yet. Add them to the data model first if literal parity for those
   migration flags is required.


## M5 implementation status

Implemented through the bundled host patch and TUI:

- Host extension DTOs carry installed version plus repository version, version code,
  store URL, content warning, obsolete/shared flags, and signature when available.
- Not-loaded files are collected with package, file, reason, and message. The client
  groups them, shows the reason, and can delete the failed APK/JAR.
- Async install jobs report pending/downloading/installing/done/error/cancelled with
  progress; the client polls them, falls back to the blocking endpoint on old hosts,
  and supports cancellation and retry.
- Cookie clearing now compiles against the 1.6 source API by clearing `JvmCookieJar`
  through the source base URL.
- Extension rows show update/install state, language, source count, 18+ markers, and
  job progress. Language and NSFW filters apply to available entries; installed entries
  stay visible for management.
- The host patch is regenerated with real new-file diffs, so `patch -p1` creates
  `FilterRoutes.kt` correctly.

Still open: signature trust, private/shared warnings, obsolete filtering, comma-separated
debounced search across source URLs/ids, store website/copy/open actions, and a modal
not-loaded reason dialog.


## Browse polish status

Latest pass:

- Global results can open the selected item, open the selected source with the query
  filled in (`o`), retry the search (`r`), save results, and hide empty/failed sources.
- Extension search accepts comma-separated terms and matches names, packages, languages,
  and source names.
- Store rows copy (`C`) and open (`o`) their index URL and refresh with `r`.
- Saving a new remote manga opens a category chooser instead of silently using Default.

Still open: debounced extension search over source URLs/ids, a full background Browse
worker, and category migration from duplicate dialogs. Result pagination now loads
automatically at the last row.
