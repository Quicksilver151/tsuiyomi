# Chapter and Story-Screen Parity Plan

## 1. Goal

Bring tsuiyomi's shared story screen to functional parity with Mihon's
`MangaScreen`, including the story header, story actions, chapter list, chapter
settings, chapter selection operations, and download controls.

The screen is shared by:

- Library → local series
- Library → saved remote manga
- Browse → local series
- Browse → remote source manga
- History → open details

Parity must therefore be implemented in shared components rather than per entry
point.

## 2. Scope

### In scope

- Story cover and metadata.
- Story-level actions: library, categories, migrate, refresh, continue reading,
  notes, share, source page, cover dialog.
- Chapter header and missing-chapter information.
- Chapter read/unread state and progress.
- Chapter filters: downloaded, unread, bookmarked, scanlator.
- Chapter sorting: source, number, upload date, alphabet, ascending/descending.
- Chapter display: title vs number.
- Chapter selection: multi-select, select all, invert, mark read/unread, previous
  chapters read, bookmark, download, delete downloads.
- Chapter downloads: single, batch, cancel, delete, series queue, progress.
- Per-series persisted chapter settings and global defaults.
- TUI equivalents for Android-only interactions.

### Out of scope

- Reader page rendering, scroll mode, zoom, page caching, and reader chrome.
- Browse source result screens and global search.
- Download storage internals beyond queue/delete commands.
- Literal WebView, Android share sheet, file picker, and tracker OAuth.
- Migration UI itself, except a story-level entry point into Migrate.

## 3. Current state

Implemented:

- Shared local/remote story header with cover, title, author, artist, source,
  status, tags, chapter count, latest chapter, library note, and description.
- Chapter list with title, number, scanlator, progress, bookmark, download marker,
  and multi-select.
- `Enter` opens a chapter.
- `b` bookmarks selected chapters.
- `D` downloads selected remote chapters.
- `F` adds/removes remote manga.
- `r` partially refreshes remote chapters.
- Reader chapter navigation.

Missing:

- Read/unread visual state independent of page progress.
- Chapter dates.
- Downloaded/unread/bookmarked/scanlator filters.
- Sort modes and direction.
- Title-vs-number display mode.
- Select all/invert.
- Mark selected read/unread and mark previous read.
- Chapter-level download cancellation/deletion/progress.
- Download next N, unread, bookmarked, and series queue.
- Continue-reading action.
- Missing-chapter count.
- Per-series chapter settings and defaults.
- Categories editing.
- Story-level migrate and notes.
- Tracking, share/copy URL, source page, cover dialog, tag actions.
- Full refresh of details plus chapters.

## 4. Architecture

### 4.1 Shared chapter settings

Add a core data type:

```text
ChapterSettings {
    downloaded: TriState,
    unread: TriState,
    bookmarked: TriState,
    excluded_scanlators: set<string>,
    sort: ChapterSort,
    descending: bool,
    display: ChapterDisplay,
}
```

Persist per-series settings in config:

```text
chapter_settings: { series_key -> ChapterSettings }
chapter_defaults: ChapterSettings
```

Series key uses the same local/remote identity as history and migration.

### 4.2 Chapter view model

Add a shared filtered/sorted chapter view used by all story screens:

```text
ChapterView {
    source_chapters: local/remote list,
    settings: ChapterSettings,
    visible_indices: Vec<usize>,
    missing_count: usize,
}
```

Filtering:

- downloaded: ignore / only / not downloaded
- unread: ignore / unread only / read only
- bookmarked: ignore / bookmarked only / not bookmarked
- scanlator: exclude selected scanlators

Sorting:

- source order
- chapter number
- upload date
- title alphabetically
- descending toggle

Display:

- chapter title
- chapter number

### 4.3 Read state

Read state is derived from progress:

- no progress: unread
- `page + 1 < pages`: in progress
- `page + 1 >= pages`: read

Mark read creates a progress entry at the last page. Mark unread removes the
progress entry. Story-screen actions update progress, history, and stats through
the existing progress/history pipeline.

### 4.4 Download state

Add a shared download queue model:

```text
DownloadState { NotDownloaded, Queued, Downloading(progress), Downloaded, Error }
```

The existing worker downloads one chapter at a time. Extend it with:

- queue of chapter keys
- state per chapter
- cancel
- delete downloaded files
- batch enqueue
- series-level enqueue
- progress events

Persist no new queue data across restarts initially; downloaded files remain the
source of truth.

## 5. Screen layout

Keep the existing two-column story layout:

- Left: chapter list.
- Right: cover and metadata.

Add a shared chapter header above the list:

```text
Chapters 142 · 3 missing
Filter: unread · Sort: number ↓ · Display: title
```

The header is also the entry point to chapter settings.

Chapter rows:

```text
●  Chapter 128 · 2h ago       12/20   [bookmark] [download]
   scanlator
```

- unread dot for chapters with no progress
- dimmed title for read chapters
- progress for in-progress chapters
- bookmark marker
- download state
- chapter date
- scanlator

## 6. Key map

Story screen:

| Key | Action |
|---|---|
| Enter | Read selected chapter |
| Space | Multi-select |
| a | Select all |
| i | Invert selection |
| r | Refresh details and chapters |
| f | Chapter filters/settings |
| s | Cycle sort mode |
| v | Cycle display mode |
| m | Mark selected read |
| u | Mark selected unread |
| U | Mark previous chapters read |
| b | Bookmark/unbookmark selected |
| D | Download selected |
| x/Delete | Delete downloaded selected |
| F | Add/remove library |
| c | Edit categories |
| M | Migrate this manga |
| n | Notes |
| C | Copy manga URL |
| o | Open source page in browser |
| x | Cover dialog when no chapter selection |
| Esc | Back |

Key conflicts must be resolved in the implementation phase; the commands bar is
the source of truth.

## 7. Implementation phases

### Phase 1 — Chapter state and settings foundation

- Add `TriState`, `ChapterSort`, `ChapterDisplay`, `ChapterSettings`.
- Add config persistence for per-series settings and defaults.
- Add filtered/sorted chapter view for local and remote.
- Add read-state helpers and mark read/unread operations.
- Migrate settings/defaults.
- Tests for filtering, sorting, display, read state, and config round-trip.

Acceptance:

- Settings persist per series.
- Filters and sort produce stable visible indices.
- Mark read/unread updates progress and history consistently.

### Phase 2 — Chapter list UI

- Add shared chapter header with count, missing count, and active settings.
- Add unread marker, read dimming, chapter date, progress, bookmark, download.
- Add selection count and select all/invert.
- Add chapter settings modal with Filter/Sort/Display sections.
- Add mark read/unread/previous actions to the commands bar.
- Tests for row rendering, empty filtered list, selection, and header.

Acceptance:

- Unread/read/in-progress chapters are visually distinct.
- Filters can produce an empty list without breaking selection.
- Select all/invert operate on visible chapters.
- Commands bar exposes every action.

### Phase 3 — Downloads

- Extend the download worker with queue state and progress.
- Add single-chapter start/cancel/delete.
- Add download next 1/5/10/25, unread, bookmarked.
- Add series queue.
- Add downloaded filter integration.
- Add delete confirmation.
- Tests for queue transitions, cancellation, deletion, and filter updates.

Acceptance:

- A series can be queued and cancelled.
- Download progress updates chapter rows.
- Downloaded/delete actions update the downloaded filter immediately.

### Phase 4 — Story actions

- Continue reading: next unread chapter.
- Full refresh: details + chapters, preserving settings.
- Edit categories for remote saved manga.
- Story-level migrate entry.
- Notes editor.
- Missing-chapter count.
- Tests for continue-reading selection, refresh state, categories, migration entry.

Acceptance:

- Continue reading opens the correct chapter.
- Refresh updates metadata and chapters.
- Category changes appear in Library immediately.
- Migrate opens the existing Migrate flow for this manga.

### Phase 5 — TUI adaptations

- Copy manga URL.
- Open source page in system browser.
- Full-cover overlay.
- Custom cover via path input.
- Tag copy and tag search/global search.
- Tests for URL/source actions and cover overlay.

Acceptance:

- Every story-level Mihon action has a TUI command or a documented deviation.
- No action is a silent no-op.

### Phase 6 — Tracking

Tracking is a separate service integration. The story screen should expose a
tracking entry only when tracker support exists; until then, document it as a
platform/integration gap rather than a dead action.

## 8. Test plan

Core:

- Chapter filter tri-state combinations.
- Sort modes and direction.
- Display title/number.
- Read-state derivation.
- Mark read/unread progress updates.
- Missing chapter count.
- Settings/defaults persistence and migration.
- Download queue state transitions.

TUI:

- Local and remote story rendering.
- Unread/read/in-progress rows.
- Chapter settings modal.
- Empty filtered list.
- Select all/invert.
- Mark read/unread/previous.
- Download queue/progress/cancel/delete.
- Continue reading.
- Categories and migration entry.
- Commands bar and key conflicts.

Manual:

- Local series with dates, scanlators, read/unread mix.
- Remote series with downloaded chapters.
- Switch between local, saved remote, and Browse remote stories.
- Restart and verify per-series settings.
- Delete downloaded chapters and verify filter.
- Run series queue and cancel.

## 9. Risks

| Risk | Mitigation |
|---|---|
| Key conflicts with existing reader/story actions | Finalize key map before Phase 2 |
| Filtered indices break selection/open | Use source indices and stable keys |
| Read state diverges from history/stats | Mark read/unread through the same progress pipeline |
| Download queue complexity | Keep in-memory queue first; files remain truth |
| Per-series settings grow config | Use compact keys and preserve unknown keys |
| Tracking needs OAuth | Separate milestone; no dead UI |
| Android-only interactions | Document TUI equivalents and deviations |

## 10. Definition of done

Chapter/story-screen parity is complete when:

- Every Mihon story/chapter action has a TUI key, command, or documented
  platform deviation.
- Chapter settings persist per series and support global defaults.
- Read/unread, bookmark, and download state are accurate and mutable.
- Filters, sorting, display, selection, and batch actions work for local and
  remote stories.
- Downloads can be queued, cancelled, deleted, and filtered.
- Continue reading and full refresh work.
- Library/category/migrate/notes/share/cover/source actions are reachable.
- The shared story screen behaves consistently from Library, Browse, and History.


## 11. Implementation status

Implemented:

- Per-series chapter settings and global defaults.
- Tri-state filters, scanlator exclusion, sort modes/direction, and display mode.
- Shared filtered/sorted chapter list with read, in-progress, and unread markers,
  dates, scanlators, and source-index cursor mapping.
- Chapter settings modal.
- Select all/invert and mark selected read/unread/previous.
- Continue reading in display order.
- Missing-chapter count in the chapter header.
- Download menu (selected, next 1/5/10/25, unread, bookmarked), UI queue,
  per-chapter state markers, cancellation of pending chapters, and confirmed
  download deletion.
- Remote details + chapter refresh and local rescan.
- Categories for saved source manga.
- Story notes persisted in config.
- Story-level migration entry.
- URL copy through OSC 52, local folder or absolute source URL opening, cover
  overlay, custom covers for local and saved source manga, and tag search.

Remaining or intentionally deferred:

- Tracking integration (separate milestone).
- Android swipe gestures; TUI keys and the download menu replace them.
- Source-site WebView; `C` and `o` cover copy/open behavior instead.
- Cover share and delete beyond replacing/removing a saved source cover.
- Tag copy actions (search is implemented).
- Per-series tracking/update fields the host does not expose.
