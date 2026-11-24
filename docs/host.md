# Extension Host

## Why a JVM

Mihon extensions are JVM bytecode that calls `android.*` and the Tachiyomi source API.
Running them needs a classloader, Android compatibility shims, and the source API, so
there is no pure-Rust path.

tsuiyomi uses **miwayomi**, a headless Kotlin/JVM 21 service that loads APK/JVM-jar
extensions and exposes them over loopback REST.

```text
tsuiyomi (single Rust binary)
├── TUI
├── embedded runtime archive
│   ├── jre/
│   └── miwayomi.jar (patched)
└── first run: extract to $XDG_CACHE_HOME/tsuiyomi/host/<version>/
        └── java -jar miwayomi.jar --host 127.0.0.1 --port <free>
                 --data $XDG_DATA_HOME/tsuiyomi/host --no-open
                 --flaresolverr <url-or-empty>
```

The launcher is `jre/bin/java` on Unix and `jre/bin/java.exe` on Windows; everything else
in the archive is platform-independent. The host is supervised on loopback. `host_url` can
point at an external server instead. WebView-only sources are unsupported.

## API surface used by the client

```text
GET  /api/v1/health
GET  /api/v1/sources
GET  /api/v1/manga/{sourceId}/popular|latest?page=N
GET  /api/v1/manga/{sourceId}/search?query=&page=N
GET  /api/v1/manga/{sourceId}/filters
POST /api/v1/manga/{sourceId}/search
POST /api/v1/sources/{sourceId}/cookies/clear
GET  /api/v1/manga/{sourceId}/details|chapters|pages?url=
GET  /api/v1/image?sourceId=&url=&imageUrl=
GET  /api/v1/proxy?sourceId=&url=&headers=
GET  /api/v1/extensions/repo?url=
GET  /api/v1/extensions/installed
GET  /api/v1/extensions/not-loaded
POST /api/v1/extensions/not-loaded/delete
POST /api/v1/extensions/install[?async=true]
GET  /api/v1/extensions/jobs/{jobId}
POST /api/v1/extensions/jobs/{jobId}/cancel
POST /api/v1/extensions/uninstall
GET|POST /api/v1/extensions/repos
GET  /api/v1/favorites
POST /api/v1/favorites
POST /api/v1/favorites/progress
GET|POST /api/v1/sources/{sourceId}/prefs
GET  /api/v1/state
POST /api/v1/state/chapters
```

Remote progress and bookmarks are mirrored through `/api/v1/state*`, backed by
`read_state.json` in the host data directory.

## Host patch

`scripts/host-patches/miwayomi-0.2.9.patch` is generated from a pristine miwayomi checkout
and adds:

- extensions-lib 1.6 `SMangaUpdate` / `AppInfo`
- memo round-tripping for details/chapters/pages
- page-aware image fetching
- filters and filtered search
- cookie clearing
- extension metadata, async install jobs, and not-loaded reporting
- persistent read-state endpoints

`just host-runtime` (or `scripts/build-host-runtime.sh`) pins the source commit, applies the
patch, builds the host, creates a `jlink` JRE, and writes
`crates/tsuiyomi-core/runtime/host.zip`.

## Host behavior notes

- Prefer the repository's published JVM `.jar` over `dex2jar`; R8 output can fail
  verification.
- `AppInfo` is patched into the host because some extensions call it at request time.
- The host patch routes details/chapters through the 1.6 update API so current Keiyoushi
  extensions work.
- Desktop-jar installation and async install jobs are handled by the host; the client
  restarts the bundled host when a jar swap requires it.

## Deferred host work

- Nix package/cache integration is tracked in [`publishing.md`](publishing.md); the Nix
  package uses the platform-independent patched jar plus a nixpkgs JRE.
- Cached Gradle dependencies for a fully offline, reproducible host build.
- Optional remote-WebView bridge for the few sources that require one (out of scope for
  the current parity pass).
- APK signing-key verification; the TUI records and trusts SHA-256 fingerprints instead,
  which is the documented deviation.
