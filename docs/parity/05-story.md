# Mihon Parity: Shared Story and Chapter Screen

Area detail for Mihon parity. Aggregate scores live in the [roadmap](../roadmap.md).

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
