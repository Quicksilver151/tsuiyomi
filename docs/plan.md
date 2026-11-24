# Plan

This is the current implementation plan for the next parity pass. The parity tables
live in the [roadmap](roadmap.md); this file tracks scope, phases, and acceptance
criteria.

> Checkbox completion here is **not** the parity percentage. The roadmap tables are the
> authoritative implementation measure (95.3% today). Unchecked boxes include tests,
> live/external verification, portability work, and backlog items, so counting plan
> checkboxes will understate feature parity.

## Goal

Close the selected Mihon gaps without trying to clone Android-only platform behavior.
The expanded roadmap currently scores **95.3%**; the target is **100% of the expanded,
in-scope rows**, with deliberate deviations recorded as `➖`. Release and distribution
work is tracked separately in [`publishing.md`](publishing.md).

## Scope

### In scope

- Proper browser-based login and token refresh for MyAnimeList and AniList.
- Complete tracking table: additional services, metadata, stats, and link/progress
  behavior.
- Complete library UX except swipe gestures.
- Complete migrate table, including notes, custom covers, and migration options.
- Complete Updates, History, and Stats tables.
- Per-series viewer mode with a reset-to-global action.
- Complete local-library/metadata/backup rows, including remaining backup fields and
  real-restore verification.
- Complete downloads table except desktop-inapplicable Wi-Fi policy.
- Chapter/series “open in browser” actions.
- Kitty placement-only zoom with an opt-out setting.
- Optional selected work: reader page actions, reader skip rules, reader display extras,
  advanced networking, and desktop notifications.
- Novel mode based on LNReader is recorded as a backlog item, not a current milestone.

### Not in scope for this pass

- Android swipe/tap/volume-key gestures and platform integrations.
- WebView-only sources.
- APK signing-key verification; TOFU fingerprints remain the documented deviation.
- Local extension installation from a path and app/security lock settings.
- Full Android settings surface not listed in this plan.

## Feature work

### Phase 1 — Tracker identity and service model

Goal: remove manual token creation and make trackers a proper subsystem.

Progress: browser OAuth/PKCE and token storage are implemented for AniList and MAL,
including MAL refresh, logout, and configurable client credentials. MangaUpdates,
Shikimori, Bangumi, Kitsu, MangaBaka, and Hikka use their supported credential flows;
AniList/MAL and the token-paste services still need live end-to-end verification with
real accounts. Tracker metadata, local stats, and per-service status/score/progress sync
are implemented.

- [x] Add per-service tracker adapters (`search`, `link`, `status`, `progress`, `score`,
      `dates`, `unlink`) while keeping the existing AniList/MAL behavior.
- [x] Add browser-based OAuth 2.0 + PKCE for AniList and MyAnimeList:
      - [x] Read application client IDs and secrets from config.
      - [x] Open the system browser.
      - [x] Listen on a loopback callback with a short timeout.
      - [x] Exchange the code, store access/refresh tokens, and refresh MAL automatically.
      - [x] Add logout/revoke and clear-token actions.
- [x] Migrate existing pasted tokens into the new token store; keep manual token entry
      as a troubleshooting fallback.
- [x] Add service adapters: Kitsu, MangaUpdates, Shikimori, Bangumi, Hikka, MangaBaka.
- [x] Extend links with status, score, start/end dates, and private flag; sync them where
      the service supports them.
- [x] Add tracker overview to More → Statistics: tracked titles, mean score, and status
      summaries.
- [ ] Add worker-side retry/backoff and surface failures without blocking the UI.
- [ ] Verify live OAuth/token flows against registered applications and real accounts.
- [x] Tests: OAuth URL/callback parsing, token refresh, adapter request/response parsing,
      link/unlink, and progress transitions are covered by unit tests; live API behavior
      remains manual.

Acceptance: a fresh user can log in to AniList and MAL from Settings without manually
creating a token. All Tracking rows are implemented; live OAuth/token verification is the
only remaining partial row.

### Phase 2 — Library UX

Goal: match Mihon’s library controls without Android gestures.

- [x] Add comfortable grid, compact grid, and list layouts; persist the choice.
- [x] Add per-category display and sort settings, with the global setting as fallback.
- [x] Add the library filter dialog:
      - [x] Unread.
      - [x] In-progress.
      - [x] Downloaded.
      - [x] Bookmarked.
      - [x] Tracked.
      - [x] Missing chapters.
      - [x] Combine filters and preserve them while navigating.
- [x] Add unread, download, and language badges to tiles.
- [x] Add sort modes: tracker score, last update, chapter fetch date, and random.
- [x] Add a continue-reading shelf or pinned row.
- [x] Add random-manga action.
- [x] Add downloaded-only library view.
- [x] Add hide-missing-chapter-indicator setting.
- [x] Add duplicate-chapter read rules for existing and newly fetched chapters.
- [x] Add smart library update settings:
      - [x] Unread-only updates.
      - [x] Skip unstarted, completed, or release-period entries.
      - [x] Category-scoped update.
      - [x] Tab badge toggle.
- [ ] Tests: layout persistence, filter combinations, sort ordering, badge counts,
      duplicate rules, and smart-update selection. Layout cycling, filters, shelf, random
      sort, hidden counts, duplicate marking, and smart-update eligibility have tests;
      badge counts and persistence across restarts remain manual.

Acceptance: every Library table row is ✅ except swipe gestures (`➖`).

### Phase 3 — Updates, History, and Stats

- [x] Add Updates settings: unread-only view, smart-update rules, category scope, and
      tab-badge toggle.
- [x] Make update actions category-aware where applicable.
- [x] Add Stats tracker overview and mean score.
- [x] Add unified local/downloaded/total title counts.
- [x] Regression-test history delete/clear, time buckets, and live stats.
- [x] Tests: smart-update selection, update filtering, tracker stats aggregation.
      History delete/clear and time-bucket tests exercise the live stores.

Acceptance: all Updates, History, and Stats rows are ✅.

### Phase 4 — Migrate completeness

- [x] Transfer per-story notes.
- [x] Transfer custom covers.
- [x] Add a migration options step with flags:
      - [x] Chapters/progress/bookmarks/history.
      - [x] Categories.
      - [x] Custom cover.
      - [x] Notes.
      - [x] Remove old downloads instead of moving them.
- [x] Persist flag defaults.
- [x] Tests: notes/cover transfer, flag combinations, remove-download behavior, and
      replace/copy modes.

Acceptance: all Migrate rows are ✅. The Migrate category now scores 100%; end-to-end
verification against a live source pair is still manual.

### Phase 5 — Downloads

- [x] Add chapter concurrency settings and respect them in the queue; page fetches inside
      one chapter remain sequential.
- [x] Add download-ahead and auto-download-while-reading settings.
- [x] Add delete-after-read policies:
      - [x] After read.
      - [x] Except bookmarked.
      - [x] Excluded categories.
- [x] Add global/category new-chapter auto-download policy.
- [x] Add CBZ output for downloaded chapters.
- [x] Add queue pause/resume/reorder.
- [x] Keep Wi-Fi-only as a documented desktop deviation.
- [ ] Tests: queue ordering, concurrency limits, policy eligibility, CBZ round-trip.
      Queue ordering, pause state, CBZ writing/discovery, and category eligibility are
      covered; chapter concurrency and a full host download round-trip are not.

Acceptance: all Downloads rows are ✅ except Wi-Fi-only (`➖`) and page-level concurrency
(`🟡`).

### Phase 6 — Reader

- [x] Add per-series viewer mode:
      - [x] Store viewer choice in per-series settings.
      - [x] Reset-to-global action in reader settings and story settings.
      - [x] Fall back to the global reading mode.
- [x] Add open-in-browser:
      - [x] Story/chapter-list `o` opens the series page.
      - [x] Reader `o` opens the current chapter page.
      - [x] Local series/chapters open their folder/file-manager location.
      - [x] Resolve relative source URLs through extension metadata; fall back to copied
            text when no base URL exists.
- [x] Add reader page actions: copy/save page, set as cover, open externally.
- [x] Add skip rules: read, filtered, duplicate chapters, and chapter transitions.
- [x] Add display extras: crop borders, grayscale, dual-page spreads, and split/rotate
      wide pages.
- [ ] Kitty placement-only zoom:
      - [ ] Explain: Kitty can reposition/resize an already transmitted image without
            resending pixels. The current page zoom crops/pads and re-encodes.
      - [ ] Add a placement-only path for supported Kitty terminals.
      - [ ] Add a Reader setting to disable it and force the re-encode path.
      - [ ] Fall back safely for tmux/SSH/other terminals.
- [ ] Tests: per-series viewer fallback/reset, URL resolution, page-action state, skip
      ordering, and zoom mode selection.

Acceptance: all listed reader rows are ✅; Kitty placement zoom has an explicit toggle
and a working fallback.

### Phase 7 — Backup completeness and verification

- [x] Parse and export `BackupTracking` entries.
- [x] Preserve viewer flags, chapter flags, update strategy, excluded scanlators, and
      `nextUpdate`.
- [x] Confirm which custom-cover fields current Mihon actually stores; no custom-cover
      byte field exists in backup manga entries, so none is invented.
- [x] Add fixture-based protobuf tests for every new field.
- [x] Add `tsuiyomi backup-info <file>` to inspect a real backup without importing, and
      an import report with counts plus unmatched entries.
- [ ] Verify a full round-trip with an Android Mihon build, or document the exact manual
      verification steps and mark the row accordingly. The CLI and import report are the
      pre-import half; an Android restore still needs a device.
- [x] Tests: tolerant parsing, unknown fields, missing fields, local/remote entries.

Acceptance: every Local-library/metadata/backup row is ✅ except real restore verification,
which is explicitly documented as an external manual check.

### Phase 8 — Advanced networking and notifications

- [x] Add custom user-agent configuration for direct page/cover/image fetches; the host
      still uses its own client for extension requests.
- [ ] Add DNS-over-HTTPS configuration where the transport supports it; otherwise apply
      it in the host and document the split.
- [x] Add an in-app diagnostics view for host, worker, and tracker failures.
- [x] Add desktop notifications through `notify-send` (Linux) and Notification Center
      (macOS), with status/queue markers as the fallback:
      - [x] New chapters/updates.
      - [x] Download completion/failure.
      - [x] Tracker sync failure.
- [x] Add Settings toggles for each notification class.
- [ ] Tests: notification payload selection and diagnostic log formatting. Diagnostics
      content has a test; notification delivery is manual and platform-specific.

### Phase 9 — Novel mode (backlog, not current priority)

Record only; do not block the parity pass.

- [ ] Architecture spike for LNReader-style text/EPUB reading.
- [ ] Text-first chapter model separate from image pages.
- [ ] Typography, scroll position, bookmarks, and progress.
- [ ] EPUB prose parsing beyond the current image-only page extraction.
- [ ] Revisit only after the current parity phases are complete.

## Verification and release

- [x] `cargo fmt --all`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] `cargo build --release`
- [ ] Host integration check if host-side changes are required.
- [ ] Manual checks with the local Mihon checkout and a seeded test library.
- [x] Update `docs/roadmap.md`, regenerate scores with `scripts/parity-score.py`, and
      record deviations honestly.

## Main risks

- Tracker OAuth client registration and refresh-token expiry.
- Third-party tracker APIs can change or require different auth models.
- Host-side user-agent/DoH support may require a host patch and runtime rebuild.
- Kitty placement-only zoom is terminal-specific; the fallback must remain reliable.
- Page-level download concurrency and custom reader color filters remain unimplemented.
- Windows code paths are implemented and type-check, but Windows CI execution and the
  release pipeline are still tracked in [`publishing.md`](publishing.md).
- Real Mihon backup restore needs an Android environment.
- Library layout/filter combinations can cause input and state complexity; keep
  selection/filter state centralized and tested.

## Suggested later work, not in this plan

- Local extension install from a path.
- App/security lock and secure-screen equivalents.
- Full Android reader setting surface (tap zones, volume keys, page transitions,
  orientation, e-ink flash).
- Notifications daemon and background library scheduler outside the TUI.
