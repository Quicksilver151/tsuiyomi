# Plan

Current plan and reference index. Historical phase details live in the
[roadmap](roadmap.md); feature status lives in the parity tables there.

## Parity references

- [Parity completion summary](roadmap.md#parity-completion)
- [On-disk compatibility](roadmap.md#on-disk-compatibility)
- [Library](roadmap.md#library)
- [Browse, sources, and global search](roadmap.md#browse-sources-and-global-search)
- [Extensions and host lifecycle](roadmap.md#extensions-and-host-lifecycle)
- [Shared story/chapter screen](roadmap.md#shared-story--chapter-screen)
- [Reader](roadmap.md#reader)
- [History and Stats](roadmap.md#history-and-stats)
- [Migrate](roadmap.md#migrate)
- [Settings, security, and data](roadmap.md#settings-security-and-data)
- [Documented TUI deviations](roadmap.md#documented-tui-deviations)

## Category plans

- [Browse](parity/browse.md)
- [History](parity/history.md)
- [Story/chapter](parity/story.md)

## Architecture references

- [Extension host](host.md)
- [`AGENTS.md`](../AGENTS.md) — compatibility contract and contributor rules.
- [`crates/tsuiyomi-core`](../crates/tsuiyomi-core) — UI-agnostic backend.
- [`crates/tsuiyomi-tui`](../crates/tsuiyomi-tui) — terminal frontend.

## Current priorities

1. Optional remote-WebView bridge for WebView-only sources.
2. Tracking integration.
3. Extension shared/signature states and richer store metadata.
4. Background migration apply and remaining synchronous mutations.
5. Real Mihon restore verification for exported backups.
6. Reader retransmit-free Kitty placement zoom.
7. Per-series storage cleanup.
