# Plan

This is the current implementation plan for the next parity pass. The parity tables
live in the [roadmap](roadmap.md); this file tracks scope, phases, and acceptance
criteria.

## Goal

Close the selected Mihon gaps without trying to clone Android-only platform behavior.
The expanded roadmap currently scores **90.8%**; the target is **100% of the expanded,
in-scope rows**, with deliberate deviations recorded as `➖`.

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

Progress: browser OAuth/PKCE with a loopback callback, token storage, MAL refresh, logout,
legacy-token migration, and configurable client credentials are implemented. AniList/MAL
auth rows are now partial pending end-to-end verification with registered apps. The
adapter layer, remaining services, full metadata, and tracker stats are still open.

- [ ] Add a tracker adapter interface in core (`search`, `link`, `status`, `progress`,
      `score`, `dates`, `unlink`) and keep the existing AniList/MAL behavior behind it.
- [ ] Add browser-based OAuth 2.0 + PKCE for AniList and MyAnimeList:
      - [ ] Register embedded public client IDs or read them from config for development.
      - [x] Open the system browser.
      - [x] Listen on a loopback callback with a short timeout.
      - [ ] Exchange the code, store access/refresh tokens, and refresh automatically.
      - [x] Add logout/revoke and clear-token actions.
- [x] Migrate existing pasted tokens into the new token store; keep manual token entry
      as a troubleshooting fallback.
- [ ] Add service adapters: Kitsu, MangaUpdates, Shikimori, Bangumi, Hikka, MangaBaka.
      Services without browser OAuth use their supported token/credential flow, stored
      with the same expiry/refresh handling where possible.
- [ ] Extend links with status, score, start/end dates, and private flag; sync them where
      the service supports them.
- [ ] Add tracker overview to More → Statistics: tracked titles, mean score, status
      breakdown, and sync failures.
- [ ] Add worker-side retry/backoff and surface failures without blocking the UI.
- [ ] Tests: OAuth URL/callback parsing, token refresh, adapter request/response parsing,
      link/unlink, and progress transitions.

Acceptance: a fresh user can log in to AniList and MAL from Settings without manually
creating a token; all Tracking table rows are implemented or documented as deviations.

### Phase 2 — Library UX

Goal: match Mihon’s library controls without Android gestures.

- [x] Add comfortable grid, compact grid, and list layouts; persist the choice.
- [ ] Add per-category display and sort settings, with the global setting as fallback.
- [x] Add the library filter dialog:
      - [x] Unread.
      - [x] In-progress.
      - [x] Downloaded.
      - [x] Bookmarked.
      - [x] Tracked.
      - [x] Missing chapters.
      - [x] Combine filters and preserve them while navigating.
- [x] Add unread, download, and language badges to tiles.
- [ ] Add sort modes: tracker score, last update, and chapter fetch date; random is done.
- [x] Add a continue-reading shelf or pinned row.
- [x] Add random-manga action.
- [x] Add downloaded-only library view.
- [x] Add hide-missing-chapter-indicator setting.
- [x] Add duplicate-chapter read rules for existing chapters (new chapters remain).
- [ ] Add smart library update settings:
      - [ ] Unread-only updates.
      - [ ] Skip unstarted, completed, or release-period entries.
      - [ ] Category-scoped update.
      - [ ] Tab badge toggle.
- [ ] Tests: layout persistence, filter combinations, sort ordering, badge counts,
      duplicate rules, and smart-update selection. Layout cycling, filters, shelf, random
      sort, hidden counts, and duplicate marking have tests; badge counts and persistence
      across restarts are covered by config tests only.

Acceptance: every Library table row is ✅ except swipe gestures (`➖`).

### Phase 3 — Updates, History, and Stats

- [x] Add Updates settings: unread-only view, smart-update rules, category scope, and
      tab-badge toggle.
- [x] Make update actions category-aware where applicable.
- [x] Add Stats tracker overview and mean score.
- [x] Add unified local/downloaded/total title counts.
- [ ] Regression-test history delete/clear, time buckets, and live stats.
- [ ] Tests: smart-update selection, update filtering, tracker stats aggregation.
      Smart-update eligibility and tracker aggregation have tests; filtering UI
      regressions and history delete coverage remain.

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

- [ ] Add page/source concurrency settings and respect them in the queue.
- [x] Add download-ahead and auto-download-while-reading settings.
- [x] Add delete-after-read policies:
      - [x] After read.
      - [x] Except bookmarked.
      - [x] Excluded categories.
- [x] Add global/category new-chapter auto-download policy.
- [x] Add CBZ output for downloaded chapters.
- [x] Add queue pause/resume/reorder.
- [ ] Keep Wi-Fi-only as a documented desktop deviation.
- [ ] Tests: queue ordering, concurrency limits, policy eligibility, CBZ round-trip.
      Queue ordering, pause state, CBZ writing/discovery, and category eligibility are
      covered; concurrency and a full host download round-trip are not.

Acceptance: all Downloads rows are ✅ except Wi-Fi-only (`➖`).

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
- [ ] Add display extras: crop borders and grayscale are done; dual-page and split/rotate
      wide pages remain.
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
- [ ] Verify a full round-trip with an Android Mihon build, or document the exact manual
      verification steps and mark the row accordingly.
- [ ] Tests: tolerant parsing, unknown fields, missing fields, local/remote entries.

Acceptance: every Local-library/metadata/backup row is ✅; restore verification is ✅ or
explicitly documented as an external manual check.

### Phase 8 — Advanced networking and notifications

- [ ] Add custom user-agent configuration.
- [ ] Add DNS-over-HTTPS configuration where the transport supports it; otherwise apply
      it in the host and document the split.
- [ ] Add an in-app diagnostics/log view for host, worker, and tracker failures.
- [ ] Add desktop notifications using OSC 9/777/terminal bell, with optional
      `notify-send`/libnotify integration:
      - [ ] New chapters/updates.
      - [ ] Download completion/failure.
      - [ ] Tracker sync failure.
- [ ] Add Settings toggles for each notification class.
- [ ] Tests: notification payload selection and diagnostic log formatting.

### Phase 9 — Novel mode (backlog, not current priority)

Record only; do not block the parity pass.

- [ ] Architecture spike for LNReader-style text/EPUB reading.
- [ ] Text-first chapter model separate from image pages.
- [ ] Typography, scroll position, bookmarks, and progress.
- [ ] EPUB prose parsing beyond the current image-only page extraction.
- [ ] Revisit only after the current parity phases are complete.

## Verification and release

- [ ] `cargo fmt --all`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo build --release`
- [ ] Host integration check if host-side changes are required.
- [ ] Manual checks with the local Mihon checkout and a seeded test library.
- [ ] Update `docs/roadmap.md`, regenerate scores with `scripts/parity-score.py`, and
      record deviations honestly.

## Main risks

- Tracker OAuth client registration and refresh-token expiry.
- Third-party tracker APIs can change or require different auth models.
- Host-side user-agent/DoH support may require a host patch and runtime rebuild.
- Kitty placement-only zoom is terminal-specific; the fallback must remain reliable.
- Real Mihon backup restore needs an Android environment.
- Library layout/filter combinations can cause input and state complexity; keep
  selection/filter state centralized and tested.

## Suggested later work, not in this plan

- Local extension install from a path.
- App/security lock and secure-screen equivalents.
- Full Android reader setting surface (tap zones, volume keys, page transitions,
  orientation, e-ink flash).
- Notifications daemon and background library scheduler outside the TUI.
