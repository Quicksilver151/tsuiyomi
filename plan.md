# Mihon Extension Support Plan

## Goal

Run the Mihon/Tachiyomi extension ecosystem from the tsuiyomi terminal UI.

Direct APK execution in Rust is not viable. Mihon loads extension APKs with
Android's `DexClassLoader`; Suwayomi proves that desktop support requires a JVM plus an
Android compatibility layer and `dex2jar`. tsuiyomi will use a JVM extension host as a
local sidecar and talk to it over Suwayomi's GraphQL API.

## Architecture

```
tsuiyomi TUI (Rust)
    |
    | GraphQL + image HTTP
    v
Suwayomi Server (JVM)
    | AndroidCompat + dex2jar + URLClassLoader
    v
Mihon/Tachiyomi extension APK
```

- `tsuiyomi-core` owns the Suwayomi client, remote source types, and extension metadata.
- `tsuiyomi-tui` owns source browsing, extension management, chapters, and reader flow.
- Local Mihon-compatible libraries remain a built-in source.
- Suwayomi is configured by URL. tsuiyomi never loads APK bytecode itself.

## Phase 1 — Sources and remote reading

- [x] Add `suwayomi_url` to config.
- [x] Add a blocking HTTP/GraphQL client in core with timeouts.
- [x] Implement source list, search/popular, chapter list, and page image URLs.
- [x] Add remote source models separate from path-based local series.
- [x] Browse sources list includes Local and all Suwayomi sources.
- [x] Remote source results use the cover grid.
- [x] Remote manga detail and chapter list stay inside Browse.
- [x] Reader pulls remote page bytes through `read_page`.
- [x] Mock-server tests for GraphQL and page URL resolution.
- [x] Add latest browsing to the remote result grid.
- [ ] Add result pagination controls.
- [ ] Move remote network calls off the UI thread.

## Phase 2 — Extension management

- [x] Browse → Extensions view.
- [x] List installed and available extensions with language, version, and update state.
- [x] Add/remove extension store URLs via GraphQL.
- [x] Install, update, uninstall extensions.
- [x] Surface server errors and unavailable-server state.
- [ ] Show source counts and content-warning badges in the extension list.
- [ ] Mock GraphQL tests for extension mutations.

## Phase 3 — Library, categories, and progress

- [x] Use stable synthetic remote keys: source ID + manga ID + chapter ID.
- [x] Persist and resume remote chapter page progress through the existing store.
- [x] Keep local category behavior intact.
- [ ] Add remote manga to Suwayomi's library via GraphQL.
- [ ] Mirror read state to Suwayomi.
- [ ] Show remote history entries.
- [ ] Replace synthetic keys with a typed local/remote progress key enum.

## Later

- [ ] Auto-start/stop Suwayomi as a managed sidecar.
- [ ] Nix package for the JVM sidecar.
- [ ] Suwayomi library/category sync.
- [ ] Optional Android device bridge.
