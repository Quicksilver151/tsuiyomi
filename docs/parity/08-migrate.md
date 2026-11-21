# Mihon Parity: Migrate

Area detail for Mihon parity. Aggregate scores live in the [roadmap](../roadmap.md).

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
| Config | Migration source configuration/order | Yes | ✅ | ✅ | Target sources are selectable, reorderable, and persisted. |
| Matching | Prioritize by chapters / hide options | Yes | ❌ | ❌ | |
| Apply | Manual search override / skip | Yes | 🟡 | 🟡 | Per-item skip implemented; manual target search pending. |
| Apply | Batch progress / cancel | Yes | 🟡 | 🟡 | Matching runs in the background with a status; applying is synchronous and cancellation drops the result. |
| Downloads | Move/delete old downloads | Yes | ❌ | ❌ | |
