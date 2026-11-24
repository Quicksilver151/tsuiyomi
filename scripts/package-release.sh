#!/usr/bin/env bash
# Package one built tsuiyomi binary into a release archive.
#
# Inputs (environment):
#   TSUIYOMI_VERSION  release version without a leading v, e.g. 0.2.0
#   TSUIYOMI_TARGET   Rust target triple, e.g. x86_64-unknown-linux-gnu
#   TSUIYOMI_OUT_DIR  output directory (default: dist)
#   TSUIYOMI_BINARY   binary path (default: target/<target>/release/tsuiyomi[.exe])
#
# Outputs:
#   <out>/tsuiyomi-v<version>-<target>.tar.gz   (Unix targets)
#   <out>/tsuiyomi-v<version>-<target>.zip      (Windows targets)
#   <out>/tsuiyomi-v<version>-<target>.sha256   (archive checksum)
#
# The archive contains the binary, README, LICENSE, CHANGELOG, notices, install notes,
# and the bundled license texts. Release workflows combine these per-archive checksums
# into one SHA256SUMS file.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${TSUIYOMI_VERSION:?set TSUIYOMI_VERSION}"
TARGET="${TSUIYOMI_TARGET:?set TSUIYOMI_TARGET}"
OUT_DIR="${TSUIYOMI_OUT_DIR:-${ROOT}/dist}"

case "${TARGET}" in
    *windows*) BINARY_NAME="tsuiyomi.exe" ;;
    *) BINARY_NAME="tsuiyomi" ;;
esac
BINARY="${TSUIYOMI_BINARY:-${ROOT}/target/${TARGET}/release/${BINARY_NAME}}"
if [[ -z "${TSUIYOMI_BINARY:-}" && ! -f "${BINARY}" ]]; then
    # A native `cargo build --release` writes to target/release, not target/<triple>.
    BINARY="${ROOT}/target/release/${BINARY_NAME}"
fi

die() { echo "package-release: $*" >&2; exit 1; }

[[ -f "${BINARY}" ]] || die "binary not found: ${BINARY}"
[[ -f "${ROOT}/LICENSE" ]] || die "LICENSE is missing"
[[ -f "${ROOT}/THIRD_PARTY_NOTICES" ]] || die "THIRD_PARTY_NOTICES is missing"

ARCHIVE_BASE="tsuiyomi-v${VERSION}-${TARGET}"
STAGE="${OUT_DIR}/${ARCHIVE_BASE}"
rm -rf "${STAGE}"
mkdir -p "${STAGE}/licenses"

cp "${BINARY}" "${STAGE}/${BINARY_NAME}"
cp "${ROOT}/README.md" "${ROOT}/LICENSE" "${ROOT}/CHANGELOG.md" \
    "${ROOT}/THIRD_PARTY_NOTICES" "${ROOT}/INSTALL.md" "${STAGE}/"
cp "${ROOT}"/licenses/* "${STAGE}/licenses/"

mkdir -p "${OUT_DIR}"

case "${TARGET}" in
    *windows*)
        ARCHIVE="${OUT_DIR}/${ARCHIVE_BASE}.zip"
        rm -f "${ARCHIVE}"
        if command -v zip >/dev/null 2>&1; then
            (cd "${OUT_DIR}" && zip -qr "$(basename "${ARCHIVE}")" "${ARCHIVE_BASE}")
        else
            # Git Bash on Windows does not always ship zip; Compress-Archive is native.
            stage_path="${STAGE}"
            archive_path="${ARCHIVE}"
            if command -v cygpath >/dev/null 2>&1; then
                stage_path="$(cygpath -w "${STAGE}")"
                archive_path="$(cygpath -w "${ARCHIVE}")"
            fi
            powershell.exe -NoProfile -NonInteractive -Command \
                "Compress-Archive -Path '${stage_path}' -DestinationPath '${archive_path}' -Force"
        fi
        ;;
    *)
        ARCHIVE="${OUT_DIR}/${ARCHIVE_BASE}.tar.gz"
        rm -f "${ARCHIVE}"
        tar -czf "${ARCHIVE}" -C "${OUT_DIR}" "${ARCHIVE_BASE}"
        ;;
esac

if command -v sha256sum >/dev/null 2>&1; then
    (cd "${OUT_DIR}" && sha256sum "$(basename "${ARCHIVE}")" > "${ARCHIVE_BASE}.sha256")
else
    (cd "${OUT_DIR}" && shasum -a 256 "$(basename "${ARCHIVE}")" > "${ARCHIVE_BASE}.sha256")
fi

echo "package-release: wrote ${ARCHIVE}"
echo "package-release: wrote ${OUT_DIR}/${ARCHIVE_BASE}.sha256"
