# Mihon Parity: Browse, Sources, and Global Search

Area detail for Mihon parity. Aggregate scores live in the [roadmap](../roadmap.md).

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
