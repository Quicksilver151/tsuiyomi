# Extension Host Plan

## Goal

Run Mihon/Tachiyomi extension APKs from a single tsuiyomi executable, with no external
server installation.

## Why a JVM host

Extensions are JVM bytecode that calls `android.*` and the Tachiyomi source API. Running
them requires a classloader, Android compatibility shims, and the source API. That is a
JVM. There is no pure-Rust path.

The chosen host is **miwayomi**: a headless Kotlin/JVM 21 service that loads extension
APKs (or repo-published JVM jars), runs them, and exposes them over a small REST API.

```
tsuiyomi (Rust, single executable)
├── TUI
├── embedded runtime archive (zip)
│   ├── jre/                 jlink'd JRE 21
│   └── miwayomi.jar
└── first run: extract to $XDG_CACHE_HOME/tsuiyomi/host/<version>/
        |
        └── java -jar miwayomi.jar --host 127.0.0.1 --port <free> \
                 --data $XDG_DATA_HOME/tsuiyomi/host --no-open --flaresolverr ""
                |
                └── REST at http://127.0.0.1:<port>/api/v1
```

No WebView support in v1. Sources that need one fail with a clear error. Cloudflare
sources can be served by an optional external FlareSolverr; it is not bundled.

## Host API used

```
GET  /api/v1/health
GET  /api/v1/sources                                  -> { manga: [Source], anime: [...] }
GET  /api/v1/manga/{sourceId}/popular?page=N
GET  /api/v1/manga/{sourceId}/latest?page=N
GET  /api/v1/manga/{sourceId}/search?query=&page=N
GET  /api/v1/manga/{sourceId}/details?url=
GET  /api/v1/manga/{sourceId}/chapters?url=
GET  /api/v1/manga/{sourceId}/pages?url=
GET  /api/v1/image?sourceId=&url=&imageUrl=           (patched; page-aware fetch)
GET  /api/v1/proxy?sourceId=&url=&headers=
GET  /api/v1/extensions/repo?url=                     -> { extensions: [RepoEntry] }
GET  /api/v1/extensions/installed
POST /api/v1/extensions/install    { repoUrl, apk }
POST /api/v1/extensions/uninstall  { pkg }
GET  /api/v1/extensions/repos
POST /api/v1/extensions/repos      { repos: [..] }
```

Details, chapters, and pages are URL-addressed, so browsing needs no host-side library or
database rows. tsuiyomi owns reading progress.

## Phase 1 — Host client (core)

- [x] Replace the Suwayomi GraphQL client with a miwayomi REST client in
      `crates/tsuiyomi-core/src/host.rs`.
- [x] Types: `RemoteSource`, `RemoteManga`, `RemoteMangaPage`, `RemoteChapter`,
      `ExtensionInfo`.
- [x] Map manga status codes, comma-separated genres, and proxy image URLs through the
      host so hotlink-protected images work.
- [x] Surface `ErrorDto.error` and Cloudflare challenge messages instead of raw HTTP
      codes.
- [x] Mock-server tests for sources, search, chapters, pages, extensions, and errors.

## Phase 2 — TUI

- [x] Rename the remote state from Suwayomi-specific names to host-neutral ones.
- [x] Browse: sources, results grid, details, chapters, reader, extensions, stores.
- [x] Search, latest, pagination-ready state.
- [x] Extension management through repos: add/remove repo URLs, install, uninstall,
      update-by-reinstall.
- [x] Progress keys: `remote://<source>/<manga>/<chapter>` hashes, independent of paths.
- [x] Settings shows the host URL instead of a Suwayomi URL.

## Phase 3 — Bundled runtime

- [x] Runtime discovery: external `host_url`, explicit `host_jar`, versioned cache dir,
      and an optional build-time embedded archive.
- [x] Atomic extraction of the embedded archive into the versioned cache.
- [x] Supervisor: loopback-only port, XDG data dir, health check, log file, graceful
      shutdown, stale-process cleanup.
- [x] Self-update disabled in the host patch (Main.kt no longer starts `UpdateManager`),
      plus a defensive directory at the download target in case an external jar is used.
- [x] Archive builder: `scripts/build-host-runtime.sh` pins the miwayomi source commit,
      applies the patch, builds the host, adds a `jlink` JRE 21, and writes
      `crates/tsuiyomi-core/runtime/host.zip` (currently 128 MB).
- [x] Prefer the repository's desktop JVM jar over `dex2jar` output.
- [x] Vendor the host patches: `scripts/host-patches/miwayomi-0.2.9.patch` adds the 1.6
      source API, `AppInfo`, and the page-aware image endpoint.
- [x] Build the archive from pinned source in `scripts/build-host-runtime.sh`.
- [ ] Nix derivation / `nix bundle` output so the JRE keeps valid store paths on NixOS.
- [ ] Cache Gradle dependencies for an offline/reproducible host build.
- [x] Verified the release binary on a clean cache: 135 MB executable, 128 MB embedded
      archive, 157 MB extracted, extraction + host start + health check in 2.2 s. With a
      Keiyoushi MangaDex jar in the data directory it exposed 62 sources.

## Phase 4 — Browse polish

- [x] Mihon-style Sources / Extensions / Migrate tabs, switched with `[`/`]`.
- [x] Sources grouped by language, with a persistent `Pinned` group.
- [x] Inline extension search (name or package).
- [x] Pin all sources of an extension from the Extensions tab.
- [x] Tabs rendered inside the Browse panel; Esc at a tab root returns to Library.
- [x] Chapter and manga `memo` round-trips through the host API, fixing
      "Refresh Chapter List" on Asura Scans and other memo-dependent sources.
- [x] Host state and logs in About, with `H` to stop or restart the sidecar.
- [x] Save remote manga to the host's library (`F`); saved manga merge into the single
      Library grid under the default category and persist in `remote_library`. Saved manga
      open the same details screen as Browse, and their covers are cached on disk when
      saved so they survive restarts without the host.
- [x] Global search across pinned sources, grouped with horizontal result rows.
- [x] Scroll mode: remote chapters now build scroll state at all, page sizes are measured
      in the background, the first page is visible while waiting, and long webtoon pages
      are split into small bounded blocks (with a decoded-page cache) instead of one giant
      protocol. `Default` stays leftmost in the category bar.
- [x] Download remote chapters with `D` into Mihon's
      `<downloads>/<source>/<series>/<chapter>/<page>.<ext>` layout, on a worker thread,
      and read downloaded pages from disk. Chapter rows show fixed bookmark/download
      columns and `b` toggles a persisted bookmark.
- [x] Scroll responsiveness: 1024 px blocks, Kitty compression, one fresh block per
      frame, viewport-aware request cancellation, debounced progress writes, and cached
      download markers/chapter lists.
- [x] Zoom keeps the viewport anchored to the same source pixel across layout rebuilds,
      including on long webtoon pages.
- [x] Remote story details and chapters are cached on disk, so saved stories open
      without the host or network after the first fetch; the host refreshes them in the
      background when it is already running.
- [x] The bundled extension host starts on a worker when Browse is first opened, so the
      tab renders immediately instead of freezing during JVM startup.
- [x] Extensions use separate keys: `i`/Enter installs, `x`/Delete uninstalls after `y`
      confirmation. The commands bar lists both.
- [x] Saved remote covers live in the data directory, survive cache cleanup, migrate from
      the old cache path, and are restored in the background when missing.
- [x] Library delete dialog (`x`/Delete) mirrors Mihon: remove from library and,
      for saved manga, delete downloaded chapters. Local files stay on disk.
- [ ] Series-level download queue and download deletion UI.
- [ ] Per-source preferences exposed by the host.
- [ ] Migrate.

## Phase 5 — Later

- [ ] Background networking; the UI currently blocks on host calls.
- [ ] Pagination controls in the results grid.
- [ ] Read-state mirroring back to the host.
- [ ] Optional FlareSolverr configuration in Settings.
- [ ] Optional remote-WebView bridge for the ~1% of sources that need one.

## Findings from a live host run

Verified against miwayomi 0.2.9 on a real Keiyoushi install:

- The REST shape matches the client. `sources`, repo parsing (`index.pb`, 1714 entries),
  install, and extension listing all work as documented.
- `dex2jar` output from a Keiyoushi APK can fail verification. MangaDex 1.6.4 threw
  `VerifyError: Expecting a stackmap frame at branch target 45` and loaded zero sources.
- The `.jar` published in the same release directory loads that extension cleanly (62
  sources). `index.pb` carries both URLs, so the jar is always available.
- The host omits `eu.kanade.tachiyomi.AppInfo`, which MangaDex calls at request time
  (`ClassNotFoundException`). Upstream Suwayomi ships this class; miwayomi does not.
  Injecting a compiled `AppInfo` class into the release jar fixes it.
- miwayomi 0.2.9 implements the pre-1.6 source API: extensions must implement
  `mangaDetailsParse`/`chapterListParse` and it never calls `getMangaUpdate`. Current
  Keiyoushi extensions (MangaDex 1.6.4 and most sampled jars) implement the 1.6 suspend
  API, so details and chapters fail with `AbstractMethodError` even though the extension
  loads. The library endpoints and `getPageList` are already compatible.
- Required host patch: add `SMangaUpdate`, add `MangaSource.getMangaUpdate`, make the
  legacy parse methods `open` with a throwing default, and route details/chapters through
  `getMangaUpdate`. This is a small, well-scoped change to the host source.
- MangaDex splits a page across a server list (`Page.url`) and a relative path
  (`Page.imageUrl`), so the client cannot GET either field directly. The patch adds
  `GET /api/v1/image?sourceId=&url=&imageUrl=`, which rebuilds the `Page` and lets the
  source execute the request. tsuiyomi uses that endpoint for every remote page.
- The patched build was verified against MangaDex 1.6.4: 62 sources load; popular,
  details, chapters, 11 pages, and a 7.2 MB image all succeed.
- Workaround now implemented: after installing an APK, tsuiyomi downloads the sibling
  `.jar`, writes it beside the APK, and restarts the host so the loader prefers it.
- Remaining gap: extensions that call `AppInfo` still need the class. The long-term fix
  is a small vendored patch to the host, not a tsuiyomi-side workaround.

## Notes

- Keiyoushi publishes `index.pb` (gzip + protobuf). Its entries carry absolute `.apk`
  and `.jar` URLs. The host currently converts the APK with `dex2jar`; preferring the
  published jar is a planned host patch.
- The host auto-downloads updates on boot. The bundled jar must be patched to disable
  that, otherwise the pinned API can drift under the binary.
- JRE and host licenses must ship in the bundle.
