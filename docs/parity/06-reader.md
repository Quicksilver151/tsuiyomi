# Mihon Parity: Reader

Area detail for Mihon parity. Aggregate scores live in the [roadmap](../roadmap.md).

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
| Progress | Host read-state mirror | Yes | ✅ | ✅ | Remote progress/bookmarks upload in batches and merge back when local state is missing. |
| Navigation | Previous/next chapter | Yes | ✅ | ✅ | Follows filtered list. |
| Bookmarks | Toggle bookmark | Yes | ✅ | ✅ | `b`. |
| Downloads | Download current chapter | Yes | ✅ | ✅ | `D`, active cancellation now works. |
| Privacy | Incognito suppresses history/time | Yes | ✅ | ✅ | Global and per-extension. |
