# Install tsuiyomi

## Prebuilt archives

Release archives are published on
[GitHub Releases](https://github.com/Quicksilver151/tsuiyomi/releases) for:

| Platform | Architectures | Archive |
|---|---|---|
| Linux | x86_64, ARM64 | `.tar.gz` |
| macOS | x86_64, ARM64 | `.tar.gz` |
| Windows | x86_64, ARM64 | `.zip` |

Each release includes a `SHA256SUMS` file. Verify the archive before running it.

Linux:

```sh
sha256sum -c SHA256SUMS --ignore-missing
tar -xzf tsuiyomi-v<version>-x86_64-unknown-linux-gnu.tar.gz
./tsuiyomi-v<version>-x86_64-unknown-linux-gnu/tsuiyomi
```

macOS:

```sh
shasum -a 256 -c SHA256SUMS
tar -xzf tsuiyomi-v<version>-aarch64-apple-darwin.tar.gz
./tsuiyomi-v<version>-aarch64-apple-darwin/tsuiyomi
```

Windows (PowerShell):

```powershell
Get-FileHash .\tsuiyomi-v<version>-x86_64-pc-windows-msvc.zip -Algorithm SHA256
Expand-Archive .\tsuiyomi-v<version>-x86_64-pc-windows-msvc.zip -DestinationPath .
.\tsuiyomi-v<version>-x86_64-pc-windows-msvc\tsuiyomi.exe
```

Release binaries bundle a Java 21 runtime. If the embedded runtime cannot start, install
a JDK 21 and make `java`/`java.exe` available on `PATH`.

### Unsigned binaries

The project does not sign binaries. Linux archives are plain executables. macOS
Gatekeeper may block the first launch: open the binary from Finder with right-click →
Open, or remove the quarantine flag:

```sh
xattr -d com.apple.quarantine ./tsuiyomi
```

Windows SmartScreen may warn about an unknown publisher. Review the checksum and choose
**More info → Run anyway**.

## First run

Point tsuiyomi at a Mihon `local/` library:

```sh
TSUIYOMI_LIBRARY="$HOME/.local/share/mihon/local" tsuiyomi
```

Without `TSUIYOMI_LIBRARY`, the library defaults to the platform data directory:

- Linux: `$XDG_DATA_HOME/tsuiyomi/local` (`~/.local/share/tsuiyomi/local`)
- macOS: `~/Library/Application Support/tsuiyomi/local`
- Windows: `%APPDATA%\tsuiyomi\local`

Set the path permanently under More → Settings → Library, or in
`~/.config/tsuiyomi/config.toml` on Linux (the platform config directory elsewhere). The
in-app settings pages are the source of truth; the file is TOML and preserves unknown
keys.

`i` imports a series folder into the library without a graphical picker.

## Terminal

Use a terminal with graphics support for covers and pages: Kitty, Ghostty, WezTerm,
Konsole, or Warp on Linux; Kitty, Ghostty, WezTerm, or iTerm2 on macOS; WezTerm on
Windows. Without one, images fall back to Unicode halfblocks. See the
[README terminal table](README.md#terminal-requirement) for details.

## Nix

With flakes enabled:

```sh
nix run github:Quicksilver151/tsuiyomi
```

The flake configures the public `tsuiyomi.cachix.org` substituter, so supported systems
install prebuilt closures instead of compiling. Override with `--option substituters ""`
to build locally.

## Build from source

The supported development environment is the Nix flake:

```sh
nix develop
just run
```

A plain Rust toolchain (1.90 or newer) works too. A normal `cargo build --release`
produces a binary that looks for `java` on `PATH`. To embed the extension host and JRE:

```sh
just host-runtime   # Linux/macOS: patched miwayomi + jlink JRE
just dist
```

On Windows:

```powershell
just host-runtime-windows   # packages a published host jar with a native JDK 21
cargo build --release
```

`TSUIYOMI_HOST_JAR` can point at an existing patched jar instead of the bundled runtime.

## Package a release archive

After building for a target:

```sh
TSUIYOMI_VERSION=0.2.0 \
TSUIYOMI_TARGET=x86_64-unknown-linux-gnu \
./scripts/package-release.sh
```

The archive is written to `dist/` with a matching `.sha256` file.

## License

tsuiyomi is MIT-licensed. See [`LICENSE`](LICENSE) and
[`THIRD_PARTY_NOTICES`](THIRD_PARTY_NOTICES).
