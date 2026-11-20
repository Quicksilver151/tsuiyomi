# History Parity and Reading-Time Stats Plan

## 1. Goal

Bring tsuiyomi's History tab to functional parity with Mihon, while adding
reading-time analytics that Mihon does not currently surface in the History tab.

Locked decisions for this plan:

- History uses small cover thumbnails in a vertical list.
- Covers stay compact: roughly 4 terminal columns wide by 3 rows tall.
- Read duration is tracked.
- Duration analytics live in a History mini-tab, using the same
  `History` / `Stats` tab-in-panel pattern as Browse's Sources / Extensions /
  Migrate tabs.
- Deleting history is permanent.
- Deleting history also clears the corresponding chapter progress so Library and
  Browse chapter rows become unread again.
- Multi-select deletion is supported.
- History is unbounded: no 500-entry cap.

The plan also covers the hidden data needed for later Statistics pages, backups,
or tracker integration, but the visible deliverable is the History tab and its
Stats mini-tab.

## 2. Mihon behavior summary

Sources reviewed:

- `ui/history/HistoryTab.kt`
- `ui/history/HistoryViewModel.kt`
- `presentation/history/HistoryScreen.kt`
- `presentation/history/components/HistoryItem.kt`
- `presentation/history/components/HistoryDialogs.kt`
- `domain/.../history/*`
- `data/.../history.sq`
- `data/.../historyView.sq`

### One row per manga, newest chapter only

Mihon stores history per chapter, but the History screen queries only the latest
history row per manga:

```sql
maxReadAtChapterId = historyView.chapterId
ORDER BY readAt DESC
```

Reading chapters 1, 2, and 3 produces one visible History row: chapter 3.
Deleting that history entry makes chapter 2 visible again.

This is the most important semantic difference from tsuiyomi, which currently
shows every chapter.

### Resume behavior

Clicking a History row does not simply reopen the recorded chapter. Mihon:

1. Loads the manga's chapters in ascending order.
2. Finds the recorded chapter.
3. Opens the current chapter if it is not fully read.
4. Opens the next chapter if the current one is fully read.
5. Shows "No next chapter" when there is nothing after it.

Re-selecting the History tab does the same for the most recent history entry.

### Other Mihon History features

- Live title search.
- Date group headers: Today, Yesterday, or a localized date.
- Cover thumbnail; clicking it opens manga details.
- Row title, chapter number, and read time.
- Add-to-library button when the manga is not a favorite.
- Delete action per row.
- Delete dialog option to remove only the chapter or all history for that manga.
- Clear-all-history action with confirmation.
- Empty states for no history and no search results.
- Snackbars for cleared history, internal error, and no next chapter.
- Duplicate detection/category selection when adding to the library.
- Migration entry point when adding a duplicate.

### Hidden Mihon data

Mihon's history table stores:

- `chapter_id`
- `manga_id`
- `read_at`, nullable so a reset can preserve the row for duration purposes
- `read_duration`, accumulated per chapter

Mihon's UI does not show read duration. It is used by Statistics and tracker
first-read calculations. Read duration accumulates active reader time between
resume/pause and chapter switches.

## 3. Current tsuiyomi behavior

Current implementation:

- `HistoryStore` in `crates/tsuiyomi-core/src/history.rs`.
- JSON object at `$XDG_DATA_HOME/tsuiyomi/history.json`.
- One entry per chapter, keyed by local chapter path or remote progress key.
- `recent()` returns every chapter, newest first.
- 500-entry hard cap.
- History tab shows text rows only.
- Enter reopens the same recorded chapter at its saved page.
- Delete removes entries immediately, without confirmation.
- `x`/Delete and multi-select exist.
- No search, date headers, covers, favorite action, next-chapter logic, clear-all,
  read duration, or mini-tabs.
- Incognito and per-extension Incognito suppress recording.
- Library chapter rows show saved page progress; clearing history does not clear
  progress.

## 4. Parity matrix

| Feature | Mihon | tsuiyomi now | Plan |
|---|---|---|---|
| One row per manga | Yes | No | Add latest-per-series query |
| Newest first | Yes | Yes | Keep |
| Latest deletion reveals previous | Yes | No | Delete one chapter entry |
| Date headers | Yes | No | Add local-date grouping |
| Cover thumbnail | Yes | No | Small cover list rows |
| Search by title | Yes | No | Inline search |
| Chapter number | Yes | Chapter title | Show number and title |
| Read time | Yes | Relative age | Local time + date header |
| Read duration | Hidden | No | Track + Stats tab |
| Add to library | Yes | No | Reuse Browse save flow |
| Open details from cover | Yes | No | `o` / cover click |
| Delete dialog single/all manga | Yes | No | Add checkbox dialog |
| Clear all history | Yes | No | Add confirmation |
| Resume next chapter | Yes | No | Add chapter-order logic |
| Reselect tab resumes latest | Yes | No | Add |
| No-next-chapter feedback | Yes | No | Status line |
| Multi-select | No | Yes | Keep as extra |
| Batch deletion | No | Yes | Keep as extra |
| Permanent deletion | No, reset | Yes | Keep permanent |
| Progress cleared on delete | No | No | Add by request |
| History cap | None | 500 | Remove |
| Stats tab | No | No | Add |

## 5. Data model

### 5.1 History entry

Keep one record per chapter. Add fields:

```text
chapter_key          stable key for the chapter
series_key           stable key for the manga/series
series_title
chapter_title
chapter_number       optional, for display
source               display name
cover                local path or cache key, optional
local / remote       resume metadata
page, pages
read_at              unix seconds, latest read
read_duration_secs   total accumulated reading time
```

Series key:

- local: canonical series directory path
- remote: source id + manga URL

Chapter key:

- local: chapter path
- remote: `remote://<source>/<manga-hash>/<chapter-hash>`

The existing entry already has most of this. Add:
- explicit `series_key`/`chapter_key` helpers
- `chapter_number`
- `read_duration_secs`

Remote resume metadata should be slimmed for unbounded storage. Keep only what
is needed:

- manga: source id, URL, title, thumbnail URL, memo
- chapter: URL, name, chapter number, scanlator, memo

Do not persist full descriptions, genres, or status for every chapter history
row. Those belong in the story cache and can be fetched when details are opened.

### 5.2 Latest-per-series query

Add:

```text
HistoryStore::latest_per_series()
```

Returns one entry per `series_key`, choosing the greatest `read_at`.
The UI selection and delete operations operate on this visible list, but the
underlying store keeps all chapter entries so deleting the latest reveals the
previous.

Add:

```text
HistoryStore::search_latest(query)
HistoryStore::remove_chapter(key)
HistoryStore::remove_series(series_key)
HistoryStore::clear()
```

### 5.3 Unbounded storage

Remove the 500-entry cap.

For unbounded history, stop rewriting the whole JSON object on every progress
update. Move to an append-only format:

`$XDG_DATA_HOME/tsuiyomi/history.jsonl`

Record types:

```json
{"type":"entry","entry":{...}}
{"type":"delete","key":"..."}
```

Rules:

- Load the file in order.
- `entry` replaces the record with the same chapter key.
- `delete` removes the record and records a tombstone.
- Ignore a malformed final line from a crash mid-write.
- Compact when tombstones or duplicate records exceed 25% of the file, or when
  the file exceeds a size threshold.
- Compaction writes `history.jsonl.tmp`, flushes, renames atomically, and only
  then removes the old file.
- Migration: if `history.json` exists and `history.jsonl` does not, import it,
  write the JSONL file, and keep the old file as `history.legacy.json` until the
  next successful startup.

Deletion remains permanent in the visible model. Internally, tombstones are only
used to avoid a full rewrite on every delete; compaction removes them.

### 5.4 Reading-time ledger

Time statistics must survive permanent history deletion. Deleting history should
not silently erase the fact that time was spent reading, matching Mihon's
behavior where resetting history keeps `read_duration`.

Use a separate small file:

`$XDG_DATA_HOME/tsuiyomi/reading_stats.json`

Contents:

```text
total_seconds
daily_seconds: { "YYYY-MM-DD": seconds }
series_seconds: { series_key: { title, seconds } }
```

This file is small and bounded by days + series count, so ordinary atomic JSON
writes are fine.

History entries also carry `read_duration_secs` for per-chapter context, but the
Stats mini-tab reads the ledger.

## 6. Read-session tracking

### 6.1 What counts

Only time with the reader open counts.

Start a session when:

- a local or remote chapter opens
- a new chapter is loaded from the reader
- the app returns from a paused/stopped state if such handling is added later

Flush the session when:

- the reader closes
- the chapter changes
- the app quits
- periodically every 60 seconds while reading

Do not record time in Incognito mode or for an extension marked Incognito.

### 6.2 Session accumulator

State fields:

```text
session_started_at: Instant
session_chapter_key: String
session_series_key: String
session_series_title: String
last_flush_at: Instant
```

On flush:

- `elapsed = now - session_started_at`
- Add elapsed to `HistoryEntry.read_duration_secs`
- Add elapsed to `reading_stats.total_seconds`
- Add elapsed to `reading_stats.daily_seconds[today]`
- Add elapsed to `reading_stats.series_seconds[series_key]`
- Persist both stores with the existing debounce, but use append-only history
  writes.

### 6.3 Idle handling

Initial version counts reader-open wall time. If stats look inflated because the
terminal is left unattended, add an inactivity cutoff later:

- Track last reader input.
- Stop the session timer after 5 minutes without input.
- Resume on the next reader input.

This is an optional refinement, not required for the first parity release.

## 7. History screen

### 7.1 Mini-tabs

Use a panel title similar to Browse:

```text
 History │ Stats
```

- `[` / `]` switches mini-tabs.
- Search is available only on History.
- `Esc`: clears active search first; otherwise returns to Library.
- Both mini-tabs keep the panel title and commands bar.

State:

```text
HistorySection { History, Stats }
```

No top-level tab change. The existing top-level History tab stays at index 2.

### 7.2 Vertical cover list

Use a custom vertical list instead of the current plain `List`.

Row layout:

```text
┌──────┬───────────────────────────────────────────────┐
│ img  │ Title                                         │
│ img  │ Chapter 123 · 14:32                           │
│ img  │ Source · page 12/30                           │
└──────┴───────────────────────────────────────────────┘
```

Cover size:

- 4 terminal columns wide
- 3 rows tall
- preserve aspect ratio
- fall back to the series initial when missing
- hide the cover column on very narrow terminals and use a text-only row

Rows are approximately 3 terminal rows plus a one-row separator/gap.
Date headers are single rows.

Selection highlight spans the full row, including the cover area.

Scrolling:

- selection moves by item, not by visual line
- keep the selected item visible
- store a `history_scroll` offset
- `PageUp` / `PageDown` moves by a screen if the input layer exposes it

### 7.3 Row content

Line 1:

- series title, bold/accent when selected
- multi-select `✓`
- favorite indicator if saved

Line 2:

- chapter number formatted like the chapter list
- chapter title when the number is unavailable or useful
- local read time `HH:MM`

Line 3:

- source name
- page progress if present
- optional duration `read 12m` if space allows

Mihon omits source/page progress from History. Keep them as tsuiyomi extras when
width allows, but never let them hide the title, chapter, or time.

### 7.4 Date headers

Group visible entries by local date:

- `Today`
- `Yesterday`
- `Mon, Jan 5` or a locale-appropriate date

Add a date/time dependency, preferably `chrono`, for local dates and local
`HH:MM` formatting. The stats ledger uses the same local-date keys.

### 7.5 Search

- `/` opens an inline search bar, matching the Extensions search style.
- Filter is live, case-insensitive, on series title only.
- Esc clears search before leaving History.
- Empty state:
  - no query: "No reading history yet."
  - query present: "No history matches \"...\"."

Search always operates on the latest-per-series visible list.

### 7.6 Stats mini-tab

Stats is a text/Unicode dashboard, not an image view.

Recommended layout:

```text
 History │ Stats

 All time    42h 18m
 Today        1h 04m
 Last 7 days  6h 12m
 Last 30 days 19h 40m

 This week
 Mon ███
 Tue ██████
 Wed ██
 ...

 Top series
 1. One Piece        12h 04m  ███████████
 2. Berserk           8h 20m  ███████
 ...

 Chapters tracked: 1,284 · Series: 87 · Streak: 6 days
```

Metrics:

- total reading time
- today
- last 7 days
- last 30 days
- 14-day or 30-day daily bar chart
- top 10 series by total duration
- chapters tracked in live history
- unique series in live history
- current reading streak in days
- optional average per active day
- optional most-used source

Stats uses the reading-time ledger, not the live history list, so deleting
history does not erase accumulated time. Add a footnote/help line explaining
this.

No reset-statistics action is required for parity. If added later, it needs a
separate confirmation.

## 8. Actions and key map

History mini-tab:

| Key | Action |
|---|---|
| `[` / `]` | Switch History / Stats |
| `/` | Search |
| `Esc` | Clear search, then leave History |
| `↑` / `↓` | Move selection |
| `Enter` | Resume next appropriate chapter |
| `o` | Open manga details without resuming |
| `F` | Add remote manga to library |
| `x` / `Delete` | Open delete dialog |
| `X` | Clear all history |
| `Space` | Multi-select |
| `q` | Quit |

Stats mini-tab:

| Key | Action |
|---|---|
| `[` / `]` | Switch mini-tab |
| `Esc` | Leave History |
| `q` | Quit |

Mouse:

- click a row: resume
- click the cover area: open details
- click delete affordance if a dedicated column is drawn

## 9. Delete semantics

### 9.1 Delete one or selected entries

Open a dialog with:

- title: "Remove history?"
- checkbox: "Also remove all history for this manga"
- focused confirm/cancel buttons

If unchecked:

- remove only the selected history chapter record(s)
- clear progress for each removed chapter

If checked:

- remove every history record for each selected manga
- clear progress for every chapter of each selected series

After deletion:

- the previous chapter history record, if any, becomes the visible row
- Library/Browse chapter rows no longer show saved page progress and appear unread
- bookmarks and downloads are untouched
- history file compacts if tombstones grow

Multi-select:

- `x` applies to every selected visible series
- the dialog says "Remove N selected entries"
- the all-chapters checkbox applies to every selected series
- clear progress for every removed chapter

### 9.2 Clear all history

`X` opens a confirmation:

> Remove all reading history and mark every chapter unread?

On confirm:

- clear the history store permanently
- clear all progress entries
- leave bookmarks, downloads, library entries, and reading-time statistics intact
- show "History cleared"

This is intentionally more destructive than Mihon's reset because the requested
behavior is that chapter read state in the Library follows history deletion.

### 9.3 Progress clearing API

Add to `ProgressStore`:

```text
remove(chapter_key)
remove_prefix(remote_source_manga_prefix)
remove_for_local_series(series_path)
clear()
```

Media-sensitive actions:

- Local chapter: remove its path key.
- Remote chapter: remove its `remote://...` key.
- Series deletion: remove all progress keys for the series.
- Clear all: clear the progress store entirely.

Persist progress immediately after deletion.

### 9.4 Statistics after deletion

Deleting history does not delete reading-time statistics. This mirrors Mihon,
where resetting history keeps `read_duration`. The Stats mini-tab can therefore
show more time than the currently visible history accounts for.

Document this behavior in the Stats mini-tab.

## 10. Resume / next-chapter logic

### 10.1 Selecting the chapter to open

For each visible history row:

1. Load the series chapter list.
   - local: from the current library scan
   - remote: cached story/chapter list, host fetch, or downloaded path
2. Sort chapters ascending using the reader's normal order.
3. Find the recorded chapter.
4. Decide whether it is finished:
   - no progress entry: unread
   - `page + 1 >= pages`: finished
   - otherwise: in progress
5. If unfinished, open it at the saved page.
6. If finished, open the next chapter at page 0.
7. If there is no next chapter, show "No next chapter".

Special cases:

- Current chapter no longer exists: try the next recognized chapter; otherwise
  show "Chapter not found."
- Local series removed from Library: show "Series not found."
- Remote series unavailable: show the host error and keep the History row.
- Downloaded remote chapter: open from disk when available.

### 10.2 Adjustment for history entries

History entry still points at the recorded chapter. The actual opened chapter may
be the next one after it, so do not rewrite the history entry until the reader
records new progress.

### 10.3 History tab reselect

When the History top-level tab is already active and the user selects it again:

1. Switch the mini-tab to History if needed.
2. Take the most recent visible series.
3. Run the same next-chapter logic.
4. Open the reader.

No next chapter shows a status message instead.

## 11. Library and Browse integration

### 11.1 Add to library

For remote history rows, `F` reuses the Browse save flow:

- check duplicates by normalized title
- duplicate dialog: add anyway, open existing, or migrate when migration exists
- select default category or show category dialog
- cache cover
- refresh Library grid and history row favorite marker

Local history rows are always in the library; `F` shows "Already local."

### 11.2 Open details

`o`, or a click on the cover area:

- local: switch to Library → Series for that path
- remote saved: open the Library remote story screen
- remote unsaved: open Browse remote details for that manga

Do not leave the History tab until the detail screen is ready.

### 11.3 Covers

Local covers:

- use `HistoryEntry.cover` when present
- otherwise resolve the current series cover by path
- regenerate the generated local cover if missing

Remote covers:

- resolve `remote_cover_path(source_id, manga_url)`
- request a background details/cover fetch when missing
- never erase an existing cached remote cover after a details response without a
  thumbnail

History covers use the shared byte-bounded LRU cover cache and are downscaled to
the 512 px cap. The 4x3 row cover is rendered from that cache.

If the terminal evicted covers after reading a large webtoon, the existing
reader-close cache reset re-transmits them on the next History draw.

## 12. Stats calculations

### 12.1 Totals

- Total: `reading_stats.total_seconds`
- Today: `daily_seconds[today]`
- Last 7 days: sum of local-date keys for today and the previous six days
- Last 30 days: same for 30 days
- Chapters tracked: count of latest-per-series entries or all history entries?
  Show both if useful:
  - "chapters tracked" = all history chapter records
  - "series tracked" = latest-per-series count
- Streak: consecutive local dates with `daily_seconds > 0`, ending today or
  yesterday.

### 12.2 Top series

Read `series_seconds` sorted descending by seconds. Keep the latest display title
in the ledger so deleted history rows do not make the title disappear.

Show top 10. Format durations as `Xh YYm` / `Ym Zs`.

### 12.3 Daily chart

Use the last 14 days by default. Render horizontal or vertical bars with Unicode
blocks. Scale by the maximum day, not a fixed maximum. Show an axis label for the
range and a max value.

If all days are zero, show "No reading time recorded yet."

### 12.4 Duration formatting

Examples:

- `< 1m` → `42s`
- `1h 04m`
- `12m`
- `2d 3h` if per-day durations ever need it

Use compact formatting in summary cards and item extras.

## 13. Storage and performance

### 13.1 History JSONL

- Append-only update records keep writes O(1).
- Deletion appends a tombstone.
- Startup loads the file and builds:
  - chapter map
  - latest-per-series map
  - series title index
  - search cache if needed
- Compaction threshold:
  - tombstone/duplicate records > 25% of lines, or
  - file > 16 MiB
- Compact at startup or when the threshold is crossed after a delete.

### 13.2 Stats JSON

- Small, bounded by days and series count.
- Atomic write with temp + rename.
- Flush on the same debounce as history, plus once on quit.
- If corrupted, rebuild what can be recovered from history read durations as a
  fallback; otherwise start empty and log a warning.

### 13.3 Search

- Use the in-memory latest-per-series list.
- Case-insensitive substring on series title.
- No database required.

### 13.4 Memory

- Do not retain descriptions/genres/status in remote history rows.
- Reuse the story cache for details.
- Cover protocols are already byte-bounded and LRU.

### 13.5 Unbounded history caveat

"No cap" means no silent data loss. It does not mean unbounded memory is free.
Monitor startup time and file size. If JSONL loading becomes too slow, the
format can move to SQLite later without changing the UI contract.

## 14. Implementation phases

### Phase 0 — Storage foundation

- Add compact history entry fields: series/chapter keys, chapter number,
  duration.
- Implement JSONL load/append/delete/compaction.
- Migrate `history.json` to `history.jsonl`.
- Add `reading_stats.json` and its daily/series ledger.
- Remove the 500-entry cap.
- Add progress removal APIs.
- Tests for migration, append/tombstone/compaction, duplicate records, stats
  persistence, and corrupt final lines.

Acceptance:

- Existing history loads.
- New reads append without rewriting the entire history.
- Deleting appends a tombstone and disappears after reload.
- Unbounded file reloads correctly.
- Stats survive restart.

### Phase 1 — History query and grouping

- `latest_per_series()`.
- Search filtering.
- Date grouping.
- `remove_chapter`, `remove_series`, `clear`.
- Count and selection helpers.
- Tests for latest-per-series, delete-reveals-previous, search, and date
  grouping.

Acceptance:

- Only one row per manga is visible.
- Deleting the visible chapter reveals the previous one.
- Search matches only series titles.
- Date headers are correct around midnight and across days.

### Phase 2 — Vertical cover list

- Custom history list renderer with 4x3 covers.
- Date headers, selection highlight, scrolling, multi-select markers.
- Cover resolution and background fetch for local/remote.
- Empty states.
- Inline search bar.
- Tests for fixed-size rendering, narrow-terminal fallback, cover fallback,
  headers, and selection.

Acceptance:

- Two series read today show one Today header and two compact cover rows.
- Covers do not dominate the list.
- Missing covers fall back gracefully.
- Scrolling keeps the selected row visible.
- Search and empty states work.

### Phase 3 — Delete, clear, and progress propagation

- Delete dialog with all-chapters checkbox.
- Multi-select batch delete.
- Clear-all confirmation.
- Remove chapter progress and series progress.
- Persist and refresh Library/Browse chapter rows.
- Tests for single, series, multi-select, clear-all, and progress clearing.

Acceptance:

- Delete one history row marks that chapter unread.
- Delete all for a series marks every chapter unread.
- Clear all history makes every chapter unread and leaves bookmarks/downloads.
- Deleting the latest history row reveals the previous history row.
- Selection remains valid after deletion.

### Phase 4 — Resume next chapter and reselect

- Chapter loading for local and remote.
- Current chapter finished/unread detection.
- Next-chapter selection and no-next status.
- History tab reselect resumes latest.
- Tests for unfinished current, finished current, last chapter, missing series,
  and downloaded chapters.

Acceptance:

- Clicking a partially read chapter resumes at its page.
- Clicking a finished chapter opens the next chapter.
- Clicking the last finished chapter shows "No next chapter".
- Pressing the History tab again resumes the latest read.

### Phase 5 — Library actions

- `F` add to library for remote history.
- Duplicate/category dialogs reused from Browse.
- `o` and cover click open details.
- Favorite marker.
- Migration hook when migration exists.
- Tests for duplicate flow, already-saved, local rows, cover click, and detail
  navigation.

Acceptance:

- Add-to-library from History matches Browse's duplicate/category behavior.
- Already-saved rows do not offer a broken action.
- Local rows open the local series detail.
- Remote rows open the correct detail screen.

### Phase 6 — Stats mini-tab

- History/Stats mini-tabs with `[`/`]`.
- Summary metrics.
- Daily chart.
- Top series bars.
- Streak and tracked counts.
- Duration formatting.
- Tests for aggregation, 7/30-day windows, top sorting, streak, empty stats, and
  rendering at narrow sizes.

Acceptance:

- Stats totals match recorded sessions.
- Deleting history does not change accumulated time.
- Daily chart scales correctly.
- Tabs switch without losing History search state.

### Phase 7 — Hardening

- Reader session timing: start, chapter switch, close, quit, periodic flush.
- Idle behavior decision.
- Incognito and per-extension suppression for both history and stats.
- Corrupt/missing file handling.
- Startup performance check with a large generated history.
- README, AGENTS.md, and plan.md updates.
- Full test and manual matrix.

## 15. Test plan

### Core tests

- JSON history migration.
- JSONL append, update, tombstone, compaction, partial final line.
- Unbounded load with >500 entries.
- latest-per-series selection and tie-breaking.
- Delete single/all-series/clear.
- Search matching and case folding.
- Date grouping.
- Progress removal for local and remote keys.
- Reading stats accumulation.
- Daily buckets around midnight and timezone changes.
- Streak calculation.
- Corrupt stats fallback.
- Remote history entry serialization remains resumable.

### TUI tests

- History mini-tab switch.
- Cover row layout at normal and narrow widths.
- Date headers and selection.
- Inline search and empty states.
- Delete dialog checkbox and confirmation.
- Multi-select delete.
- Clear-all dialog.
- Add-to-library dispatch.
- Open details dispatch.
- Next-chapter selection.
- Reselect History.
- Stats summary, bar chart, and top-series rendering.

### Manual matrix

- Local history with a reader-only chapter.
- Remote history from saved and unsaved manga.
- Local and remote no-next-chapter cases.
- Large webtoon followed by cover display.
- Missing local cover.
- Missing remote cover with host offline.
- Incognito and per-extension Incognito.
- Delete latest → previous appears.
- Delete all for series → chapter progress gone.
- Clear all → all progress gone, bookmarks/downloads intact.
- 10,000-entry history load and search.
- Restart with JSONL and stats files.
- Stats after deleting history.

## 16. Files likely touched

Core:

- `crates/tsuiyomi-core/src/history.rs`
- new `crates/tsuiyomi-core/src/history_store.rs` or split modules
- new `crates/tsuiyomi-core/src/reading_stats.rs`
- `crates/tsuiyomi-core/src/progress.rs`
- `crates/tsuiyomi-core/src/lib.rs`

TUI:

- `crates/tsuiyomi-tui/src/app.rs`
- `crates/tsuiyomi-tui/src/draw.rs`
- `crates/tsuiyomi-tui/src/lib.rs`
- `crates/tsuiyomi-tui/src/input.rs`
- maybe a new `history_action.rs`/`history_draw.rs` if `draw.rs` grows too far

Dependencies:

- Add a date/time crate, likely `chrono`, for local date grouping and `HH:MM`
  formatting.
- No new image dependency; reuse the existing cover cache.

Docs:

- `README.md`
- `AGENTS.md`
- `plan.md`
- this file

## 17. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Unbounded JSON becomes slow | Append-only JSONL, compaction, slim remote entries |
| Deleting latest history confuses users | Date row changes to previous chapter; status explains |
| Clearing progress is destructive | Confirmation text explicitly says chapters become unread |
| Read time inflated by idle terminal | Add inactivity cutoff if needed |
| Stats and history diverge | Stats ledger is separate and documented |
| Missing remote cover while offline | Placeholder initial; retry later |
| JSONL crash truncation | Skip malformed final line; atomic compaction |
| Duplicate series keys after moving local folders | Key by canonical series path; fall back to title match for details |
| Next-chapter logic differs by source | Reuse reader chapter ordering and page completion rules |
| Migration removes progress unexpectedly | Only touch progress for explicitly deleted chapters/series |

## 18. Decisions locked by this plan

- Vertical list with 4-column × 3-row covers.
- History and Stats are mini-tabs inside the History panel.
- Read duration is tracked per chapter and in a daily/series ledger.
- History deletion is permanent.
- History deletion clears corresponding chapter progress.
- Multi-select deletion is supported.
- History storage is unbounded.
- Time statistics survive history deletion.
- Clear-all history clears all chapter progress after confirmation.
- Bookmarks and downloads are not affected.
