# Mihon Parity: On-disk Compatibility

Area detail for Mihon parity. Aggregate scores live in the [roadmap](../roadmap.md).

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
| Backups | Import `.tachibk` / `.proto.gz` | Yes | 🟡 | 🟡 | Remote favorites/categories/notes/progress/bookmarks/history; local-source entries and some metadata are skipped. |
| Backups | Export Mihon-restorable backups | Yes | 🟡 | 🟡 | Local and remote library/categories/chapter state/history written; Mihon restore is not yet round-trip verified. |
