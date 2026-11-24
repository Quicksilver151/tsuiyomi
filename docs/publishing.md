# Publishing plan

This is the release plan for shipping tsuiyomi as open-source, unsigned binaries on
GitHub Releases and as a Nix flake backed by a public binary cache. Completion is tracked
as the Publishing category in [`roadmap.md`](roadmap.md).

## Decisions

- License: MIT.
- Code signing: none. Releases carry SHA-256 checksums only; no minisign, Authenticode,
  Apple notarization, or GitHub attestations.
- Binary cache: Cachix.
- Test release: `v0.2.0`, published as a GitHub pre-release.
- Public release: `v1.0.0`.
- Supported architectures: 64-bit x86 and ARM only. No 32-bit i686/armv7 builds.
- Windows is fully supported for `v1.0.0`; it must pass the portability gate below.

## Target matrix

| Target | Runner | Archive |
|---|---|---|
| `x86_64-unknown-linux-gnu` | `ubuntu-22.04` | `.tar.gz` |
| `aarch64-unknown-linux-gnu` | `ubuntu-22.04-arm` | `.tar.gz` |
| `x86_64-apple-darwin` | `macos-15-intel` | `.tar.gz` |
| `aarch64-apple-darwin` | `macos-15` | `.tar.gz` |
| `x86_64-pc-windows-msvc` | `windows-2025` | `.zip` |
| `aarch64-pc-windows-msvc` | `windows-11-arm` | `.zip` |

Linux binaries are built on Ubuntu 22.04 to keep an older glibc baseline. Nix packaging
covers `x86_64-linux`, `aarch64-linux`, `x86_64-darwin`, and `aarch64-darwin`; Nix is not
a Windows distribution channel.

## Release assets

```text
tsuiyomi-v<version>-<target>.tar.gz   # Linux and macOS
tsuiyomi-v<version>-<target>.zip      # Windows
SHA256SUMS
```

The host runtime is embedded in each release binary, so users do not need Java for
prebuilt binaries. The patched, platform-independent host jar is published separately
under `host-v0.2.9` and is used by the Nix package.

Archives include:

- the `tsuiyomi` binary or `tsuiyomi.exe`
- `README.md`
- `LICENSE`
- `CHANGELOG.md`
- `THIRD_PARTY_NOTICES`
- `INSTALL.md` with checksum and platform notes

## Preflight blockers

These must be resolved before any release workflow is enabled:

- Windows code paths are implemented but have not run on Windows CI: shared openers,
  `java.exe`/`PATHEXT` discovery, `taskkill` stale-host cleanup, reserved-name
  sanitizing, native clipboard, and PowerShell notification/runtime packaging.
- The CI and host-jar workflows are committed but have not had a first GitHub run.
- The flake has no package output and no Cachix configuration.
- No binary release workflow exists yet.

## Implementation order

1. Add the MIT license, Cargo metadata, notices, and changelog.
2. Complete the Windows portability gate and tests.
3. Add CI for formatting, linting, tests, and cross-platform checks.
4. Add the Nix package and flake outputs.
5. Create the Cachix cache and wire its public key and token into CI.
6. Add the host-jar release workflow.
7. Add the multi-target binary release workflow.
8. Run the `v0.2.0` test release, fix failures, and repeat.
9. Update the README and install documentation for the final release.
10. Bump to `1.0.0`, publish `v1.0.0`, and mark it latest.

## Phase 0 — Repository and portability gate

### License and metadata

- [x] Add `LICENSE` with the MIT text.
- [x] Add `CHANGELOG.md` using Keep a Changelog.
- [x] Add `THIRD_PARTY_NOTICES` with:
  - miwayomi (Apache-2.0)
  - Temurin/OpenJDK JRE (GPLv2 + Classpath Exception)
  - UnRAR freeware terms
  - bundled Rust dependency licenses
- [x] Add workspace metadata:
  - `license = "MIT"`
  - `repository = "https://github.com/Quicksilver151/tsuiyomi"`
  - `homepage`, `readme`, `keywords`, `categories`
- [x] Make each crate inherit the workspace license.
- [x] Generate `licenses/RUST-DEPENDENCIES.txt` from `Cargo.lock` with
  `scripts/dependency-notices.py`.

### Windows portability

- [x] Add a shared opener helper in core with per-OS commands:
  - Windows: `cmd /C start "" <target>`
  - macOS: `open <target>`
  - Linux/BSD: `xdg-open <target>`
- [x] Use it from story, store, reader-page, and external-file actions.
- [x] Add `.exe` handling to Java discovery:
  - cached JRE `jre\bin\java.exe`
  - `JAVA_HOME\bin\java.exe`
  - PATH search respecting `PATHEXT`
- [x] Replace stale-process `kill -TERM` with `taskkill /PID <pid> /T /F` on Windows.
- [x] Verify host runtime extraction expects `jre/bin/java.exe` and Windows DLLs.
- [x] Extend filename sanitizing for Windows reserved names:
  `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`,
  trailing dots, and trailing spaces.
- [x] Add a Windows clipboard fallback for text copies; keep OSC 52 elsewhere. PowerShell
  `Set-Clipboard` preserves non-ASCII text, unlike `clip.exe`.
- [x] Make notifications best-effort with a PowerShell toast on Windows.
- [x] Count the platform-specific JRE path as part of the host archive contract.
- [x] Package Windows host runtimes with `scripts/build-host-runtime.ps1`, reusing the
  published jar and a native JDK 21.
- [ ] Exercise the host packaging and application on Windows x64 and ARM64 CI.

### Acceptance

- `cargo test --workspace --locked` passes on Linux, macOS, and Windows x86_64.
- `cargo check` passes for both Windows architectures.
- `tsuiyomi --help`, `tsuiyomi backup-info <fixture>`, and
  `TSUIYOMI_HOST_SELFTEST=1 tsuiyomi` pass on Windows x64 and ARM64.
- Opening a folder, URL, and source page works on Windows.
- Stopping the host leaves no orphan JVM.
- Download and generated-cover names are valid on NTFS.

## Phase 1 — CI

`.github/workflows/ci.yml` runs for pushes to `master` and pull requests:

- [x] `cargo fmt --all --check`
- [x] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [x] `cargo test --workspace --locked` on:
  - [x] Linux x86_64 (`ubuntu-22.04`)
  - [x] Linux aarch64 (`ubuntu-22.04-arm`)
  - [x] macOS x86_64 (`macos-15-intel`)
  - [x] macOS arm64 (`macos-15`)
  - [x] Windows x86_64 (`windows-2025`)
  - [x] Windows arm64 (`windows-11-arm`)
- [x] Dependency-notice freshness check (`scripts/dependency-notices.py --check`)
- [ ] First CI run green on every target
- [ ] Optional `cargo audit` / `cargo deny`

Actions are pinned to commit SHAs and use `Swatinem/rust-cache`. The release workflow
adds target-specific `cargo check`/release builds for the remaining architectures.

## Phase 2 — Host jar release

The host jar is platform-independent, so build it once.

Workflow `.github/workflows/host-release.yml`, triggered by `host-v*`:

1. [x] Check out the repository.
2. [x] Install JDK 21 (Temurin).
3. [x] Build the pinned miwayomi source with `scripts/host-patches/miwayomi-0.2.9.patch`.
4. [x] Cache Gradle keyed by the build script and patch hash.
5. [x] Verify the patched classes:
   - `eu/kanade/tachiyomi/AppInfo.class`
   - `eu/kanade/tachiyomi/source/model/SMangaUpdate.class`
6. [x] Publish `miwayomi-0.2.9-patched.jar` plus its SHA-256.
7. [x] Tag or release as `host-v0.2.9`; the tag must match `HOST_VERSION` in the script.
8. [ ] First `host-v0.2.9` workflow run green.

`TSUIYOMI_HOST_SKIP_JRE=1` builds only the jar; each release target creates its own JRE.

Bump the host tag whenever the host commit or patch changes. The Nix package pins this
artifact.

## Phase 3 — Per-target host runtimes

Workflow job matrix in `.github/workflows/release.yml`:

- Download the patched host jar.
- Set `TSUIYOMI_HOST_JAR_PATH`.
- Run the host runtime script with a native JDK 21 for each target:
  `scripts/build-host-runtime.sh` on Unix and
  `scripts/build-host-runtime.ps1` on Windows.
- Upload `host-runtime-<target>.zip`.

`jlink` cannot cross-compile a JRE, so each target builds its JRE natively. The Windows
job must use an ARM64 JDK for `aarch64-pc-windows-msvc`.

## Phase 4 — Binary release workflow

Triggered by `v*` tags.

| Job | Depends on | Output |
|---|---|---|
| `validate` | — | Tag/Cargo version match; host release exists; license files exist |
| `host-jar` | validate | Patched host jar |
| `host-runtime` × 6 | host-jar | Native `host.zip` per target |
| `build` × 6 | host-runtime | Binary and archive per target |
| `nix-cache` × 4 | validate | Cached Nix package per system |
| `publish` | build, nix-cache | GitHub release, archives, `SHA256SUMS` |

Per target:

1. Download `host-runtime-<target>.zip`.
2. Copy it to `crates/tsuiyomi-core/runtime/host.zip`.
3. Run `cargo build --release --locked`.
4. Package the binary with the license, readme, changelog, notices, and install notes.
5. Run smoke tests.
6. Upload the archive as a workflow artifact.

`publish` waits for the Nix cache, so a release is not visible before its Nix outputs are
available. Use `permissions: contents: write`. No signing steps.

Release behavior:

- Tags with `-rc`/`-beta`/`-alpha` are GitHub pre-releases.
- Only stable `v1.0.0` is marked latest.
- Missing target artifacts fail the release.

## Phase 5 — Nix flake and Cachix

### Flake outputs

Add to `flake.nix`:

- `packages.<system>.default`
- `apps.<system>.default`
- `checks.<system>.default`
- existing `devShells`
- existing `formatter`

Systems: `x86_64-linux`, `aarch64-linux`, `x86_64-darwin`, `aarch64-darwin`.

### Package derivation

Use `rustPlatform.buildRustPackage`:

- version read from `Cargo.toml`
- `cargoLock.lockFile = ./Cargo.lock`
- source filter includes only `Cargo.toml`, `Cargo.lock`, and `crates/`
- this keeps README/docs changes from invalidating the cache
- `doCheck = false`; CI owns test coverage
- wrap the binary with `jre21`/`jdk21` on `PATH`
- set `TSUIYOMI_HOST_JAR` to the patched host jar
- include license and notices in the output

The host jar is fetched with `fetchurl` from the `host-v0.2.9` release. Keep its hash in a
small `nix/host-jar.nix` so host changes are explicit.

### Cachix

Create a public Cachix cache named `tsuiyomi`.

Add:

```nix
nixConfig = {
  extra-substituters = [ "https://tsuiyomi.cachix.org" ];
  extra-trusted-public-keys = [
    "tsuiyomi.cachix.org-1:REPLACE_WITH_REAL_KEY"
  ];
};
```

CI:

```yaml
- uses: cachix/install-nix-action@v31
- uses: cachix/cachix-action@v17
  with:
    name: tsuiyomi
    authToken: ${{ secrets.CACHIX_AUTH_TOKEN }}
- run: nix build .#tsuiyomi
```

Cache matrix:

- `ubuntu-24.04` → `x86_64-linux`
- `ubuntu-24.04-arm` → `aarch64-linux`
- `macos-15` → `aarch64-darwin`
- `macos-15-intel` → `x86_64-darwin` if Nixpkgs support is healthy

Pin release closures:

```sh
cachix pin tsuiyomi tsuiyomi-v1.0.0-x86_64-linux "$(nix path-info .#tsuiyomi)"
```

Cachix has a 5 GB free open-source tier. Configure `nix-community.cachix.org` as an
upstream so only project paths are uploaded. If the cache outgrows the tier, move to
self-hosted Attic later; the flake substituter URL/key changes, not the package.

Document both:

```sh
cachix use tsuiyomi
nix profile install github:Quicksilver151/tsuiyomi
```

and, for users who do not accept flake config:

```sh
nix build --accept-flake-config github:Quicksilver151/tsuiyomi
```

## Phase 6 — `v0.2.0` test release

- Keep the workspace version at `0.2.0`.
- Tag `v0.2.0`.
- Publish as a pre-release.
- Exercise every target and both cache paths.
- Verify:
  - all six archives extract and run
  - Windows x64 and ARM64 host self-test
  - macOS binaries launch outside the build runner
  - Linux binaries run on the oldest supported glibc
  - `SHA256SUMS` verifies
  - a clean Nix store installs from Cachix without compiling
  - `nix build` hits the cache for all four systems
- Fix failures with `v0.2.0-rc.N` tags if needed; never move a published tag.

## Phase 7 — Documentation before release

Update before bumping the version:

- `README.md` installation section:
  - release asset table
  - checksum verification
  - Nix install and Cachix commands
  - Windows/macOS/Linux notes
  - unsigned-binary warning
  - Java requirement only for source/Nix builds
- Add a link to this publishing plan.
- `docs/plan.md`/roadmap links if needed.
- Release checklist and known limitations.

## Phase 8 — `v1.0.0`

- Bump the workspace version to `1.0.0`.
- Regenerate the lockfile if needed.
- Run the full CI gate.
- Tag `v1.0.0`.
- Publish as latest.
- Verify release assets, Nix cache, and install commands again.
- Announce only after the release and cache are both healthy.

## Release checklist

- [x] MIT `LICENSE`, Cargo metadata, notices, and changelog committed.
- [ ] Windows portability tests green.
- [ ] CI green on all desktop platforms.
- [ ] `host-v0.2.9` jar release exists and has a stable hash.
- [ ] All six host runtimes built natively.
- [ ] All six binaries pass smoke tests.
- [ ] `SHA256SUMS` generated and verified.
- [ ] Cachix cache public key added to the flake.
- [ ] Cachix secrets configured.
- [ ] Nix cache hit verified from a clean store.
- [ ] README install instructions updated.
- [ ] `v0.2.0` pre-release verified.
- [ ] `1.0.0` version bump and `v1.0.0` release.
- [ ] `v1.0.0` marked latest.

## Known risks

- Windows host/JDK packaging may expose path and process bugs late.
- macOS binaries are unsigned; Gatekeeper may require a manual bypass.
- Windows binaries are unsigned; SmartScreen may warn.
- Bundling a JRE adds size and third-party license obligations.
- The Cachix free tier may be tight; maintain upstreams and pins.
- Nix cache hits require stable source filtering and matching tags.
- GitHub-hosted ARM runners and Windows ARM64 JDK availability can change.
