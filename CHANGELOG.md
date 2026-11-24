# Changelog

All notable changes to tsuiyomi are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases use semantic
versioning. `v0.2.0` is the end-to-end release test; `v1.0.0` is the first public release.

## [Unreleased]

### Added

- Mihon Local source library support: `ComicInfo.xml`, legacy `details.json`,
  `cover.jpg`, generated first-page covers, chapter-number parsing, and directory,
  CBZ, CBR, CB7, CBT, and image-based EPUB chapters.
- Library layouts, filters, sorts, Continue shelf, random series, category assignments,
  reading badges, and smart-update restrictions.
- Background scanning, saved-source story caching, and refresh workers.
- Extension-backed Browse with Sources, Extensions, Migrate, global search, filters,
  source preferences, install/update jobs, and bundled extension host lifecycle.
- Chapter downloads with batch menus, queue pause/reorder/cancel, CBZ output, policies,
  and a per-series download view.
- Reader with paged and continuous modes, zoom/pan, fit modes, dual pages, rotation,
  grayscale, cropped borders, bookmarks, progress, and per-series viewer settings.
- Updates, History, Statistics, and reading-time tracking.
- AniList, MyAnimeList, MangaUpdates, Shikimori, Bangumi, Kitsu, MangaBaka, and Hikka
  tracking, with browser OAuth for AniList and MyAnimeList.
- Mihon backup import/export, `backup-info`, and import reports.
- Desktop notifications, diagnostics, theme selection, and a settings tree mirroring
  Mihon's structure.
- Windows support: native open/clipboard/notification helpers, `java.exe`/`PATHEXT`
  Java discovery, `taskkill` stale-host cleanup, NTFS-safe download names, and a
  PowerShell host runtime packager.
- MIT licensing, third-party notices, and a dependency license inventory.

### Known limitations

- Live AniList/MyAnimeList OAuth and token-based tracker flows need verification against
  real accounts.
- A real Android Mihon restore has not been exercised yet.
- Kitty placement-only zoom, DNS-over-HTTPS, page-level download concurrency, custom
  reader color filters, and verbose application logging are not implemented.
- Windows has not yet run in CI; ARM64 Windows packaging is untested.
