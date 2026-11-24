# Release runbook

Operational steps for the `v0.2.0` test release and the `v1.0.0` public release. The
design and acceptance criteria live in [`publishing.md`](publishing.md).

**Status:** `v0.2.0` is published as a pre-release. `host-v0.2.9`, CI, six binary
archives, checksums, and four Nix package builds are complete. The remaining work is the
Cachix cache and `v1.0.0`.

Run everything from the repository root. The GitHub CLI must be authenticated as a
repository admin:

```sh
gh auth status
```

## 1. Push and get CI green

The release workflows trigger from pushes and tags, so CI must pass first.

```sh
git status
git push origin master
gh run list --workflow CI --limit 1
gh run watch --exit-status
```

Expected jobs: `rustfmt`, `clippy`, `minimum Rust 1.90`, `dependency notices`, and tests
on Linux, macOS, and Windows x86_64/ARM64.

If a job fails:

```sh
gh run view --log-failed
```

Fix, commit, and push again. Do not tag a release until every CI job is green.

## 2. Publish the patched host jar

The tag must match `HOST_VERSION` in `scripts/build-host-runtime.sh` (`0.2.9`).

```sh
git tag host-v0.2.9
git push origin host-v0.2.9
gh run list --workflow "Host runtime jar" --limit 1
gh run watch --exit-status
```

The workflow verifies the tag, builds the pinned miwayomi source with the tsuiyomi
patch, checks the patched classes, writes a SHA-256 checksum, and creates the
`host-v0.2.9` GitHub release.

Verify the assets:

```sh
gh release view host-v0.2.9
tmp="$(mktemp -d)"
gh release download host-v0.2.9 --pattern '*.sha256' --dir "$tmp"
cat "$tmp"/*.sha256
```

Expected assets:

```text
miwayomi-0.2.9-patched.jar
miwayomi-0.2.9-patched.jar.sha256
```

If the workflow fails, inspect the log, fix the host script or patch, then move the tag:

```sh
gh release delete host-v0.2.9 --yes
git tag -d host-v0.2.9
git push origin :refs/tags/host-v0.2.9
# commit and push the fix, then tag again
```

`host-v0.2.9` is a release asset, not a binary release. It is not marked latest.

## 3. Add the Nix package and Cachix cache

The flake package is committed and pins the published jar:

```text
https://github.com/Quicksilver151/tsuiyomi/releases/download/host-v0.2.9/miwayomi-0.2.9-patched.jar
```

It builds with `rustPlatform.buildRustPackage` and `Cargo.lock`, wraps `tsuiyomi` with
`jdk21` and `TSUIYOMI_HOST_JAR`, and exposes `packages.<system>.default`,
`apps.<system>.default`, and `checks.<system>.default` for `x86_64-linux`,
`aarch64-linux`, `x86_64-darwin`, and `aarch64-darwin`. Nixpkgs 26.11 dropped
`x86_64-darwin`, so that system uses the pinned `nixpkgs-26.05-darwin` input.

What remains is building the derivation, creating the cache, and adding the `nix-cache`
release job.

### Cachix setup

1. Sign in at <https://app.cachix.org/> with GitHub and create a public cache named
   `tsuiyomi` from the dashboard. The CLI cannot create caches.

2. Copy the cache's public key from the dashboard, and create an auth token on the
   account/token page. Configure it locally:

   ```sh
   nix profile install nixpkgs#cachix
   cachix authtoken <auth-token-from-cachix>
   ```

3. Copy the cache public key (`tsuiyomi.cachix.org-1:...`) into the flake's
   `nixConfig.extra-trusted-public-keys`.

4. Add the Cachix auth token as a repository secret:

   ```sh
   gh secret set CACHIX_AUTH_TOKEN
   ```

5. After the package builds, seed the cache once:

   ```sh
   nix build .# -L
   cachix push tsuiyomi ./result
   ```

6. Verify substitution from a clean profile or store path:

   ```sh
   nix run .# -- --help
   ```

The release workflow needs a `nix-cache` job matrix that builds each system, pushes with
`cachix/cachix-action`, and makes `publish` depend on it.

## 4. Test release `v0.2.0`

The workspace version must be `0.2.0` when tagging. The release workflow refuses a tag
that does not match `[workspace.package] version` in `Cargo.toml`.

```sh
git tag v0.2.0
git push origin v0.2.0
gh run list --workflow Release --limit 1
gh run watch --exit-status
```

`v0.2.0` is published as a pre-release because it is a `0.x` version. The workflow also
builds the Nix package on four systems before publishing. It contains:

```text
tsuiyomi-v0.2.0-<target>.tar.gz / .zip   (six targets)
SHA256SUMS
```

Verify:

```sh
dest="$(mktemp -d)"
gh release download v0.2.0 --dir "$dest"
(cd "$dest" && sha256sum -c SHA256SUMS)
gh release view v0.2.0
```

Run at least one archive on each platform you can access. The workflow already runs
`--help` and `TSUIYOMI_HOST_SELFTEST=1` on all six runners.

If the release fails or the assets are wrong, delete and retry:

```sh
gh release delete v0.2.0 --yes
git tag -d v0.2.0
git push origin :refs/tags/v0.2.0
```

## 5. Public release `v1.0.0`

After the test release is healthy:

1. Set `[workspace.package] version = "1.0.0"` in `Cargo.toml`.
2. Update `Cargo.lock` by running `cargo check --workspace`.
3. Move the `Unreleased` entries in `CHANGELOG.md` under `## [1.0.0] - <date>` and add a
   `## [0.2.0] - <date>` section for the test release.
4. Refresh `docs/roadmap.md`, `docs/plan.md`, `docs/publishing.md`, `README.md`, and
   `AGENTS.md`; run `python3 scripts/parity-score.py`.
5. Commit, push, and wait for CI.
6. Tag and publish:

   ```sh
   git tag v1.0.0
   git push origin v1.0.0
   gh run list --workflow Release --limit 1
   gh run watch --exit-status
   gh release view v1.0.0
   ```

`v1.0.0` is marked latest. Verify the Nix cache is populated and the install commands in
`INSTALL.md` work before announcing.

## What to send back

After creating the Cachix cache, send its public key (`tsuiyomi.cachix.org-1:...`). It goes
into the flake so users can substitute cached builds.
