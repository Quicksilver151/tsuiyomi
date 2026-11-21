# Mihon Parity: Library

Area detail for Mihon parity. Aggregate scores live in the [roadmap](../roadmap.md).

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
