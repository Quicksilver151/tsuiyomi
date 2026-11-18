#!/usr/bin/env bash
# Build the embedded extension-host runtime archive.
#
# Sources are pinned to a miwayomi commit, patched for the extensions-lib 1.6 API (see
# scripts/host-patches/), and compiled. The result is:
#
#   crates/tsuiyomi-core/runtime/host.zip
#     miwayomi.jar   patched host
#     jre/           jlink'd Java 21 runtime
#
# `cargo build` embeds the archive (see crates/tsuiyomi-core/build.rs) and the TUI
# extracts it on first run. Run `just dist` to build both in one step.
#
# Requirements: JDK 21, network access for the source tarball and Gradle dependencies.
# Set TSUIYOMI_HOST_JAR_PATH to reuse an already-built jar instead.
set -euo pipefail

HOST_VERSION="0.2.9"
HOST_COMMIT="a3a776ebd26f91f558cd59a4bd57fff9119ee987"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="${ROOT}/target/host-runtime"
OUT="${ROOT}/crates/tsuiyomi-core/runtime/host.zip"
SRC="${WORK}/src"
JAR="${WORK}/miwayomi.jar"
JRE="${WORK}/jre"
STAGE="${WORK}/stage"
PATCH="${ROOT}/scripts/host-patches/miwayomi-${HOST_VERSION}.patch"

die() { echo "host-runtime: $*" >&2; exit 1; }

command -v java >/dev/null 2>&1 || die "java not found; enter the devshell first"
command -v jlink >/dev/null 2>&1 || die "jlink not found; a JDK 21 is required"
command -v zip >/dev/null 2>&1 || die "zip not found"
command -v tar >/dev/null 2>&1 || die "tar not found"

JAVA_VERSION="$(java -version 2>&1 | head -1)"
echo "host-runtime: using $JAVA_VERSION"

mkdir -p "${WORK}"

if [[ -n "${TSUIYOMI_HOST_JAR_PATH:-}" ]]; then
    cp "${TSUIYOMI_HOST_JAR_PATH}" "${JAR}"
elif [[ ! -f "${JAR}" ]]; then
    echo "host-runtime: fetching miwayomi ${HOST_VERSION} (${HOST_COMMIT:0:7})"
    rm -rf "${SRC}"
    mkdir -p "${SRC}"
    curl --fail --location --retry 3 \
        "https://github.com/miwayomi/miwayomi/archive/${HOST_COMMIT}.tar.gz" \
        | tar -xz --strip-components=1 -C "${SRC}" \
        || die "failed to download the host source"

    [[ -f "${PATCH}" ]] || die "missing patch ${PATCH}"
    patch -d "${SRC}" -p1 --forward < "${PATCH}" || die "failed to apply the host patch"

    echo "host-runtime: building the host (this takes several minutes)"
    (cd "${SRC}" && ./gradlew :server:shadowJar --no-daemon -q) || die "host build failed"
    cp "${SRC}/server/build/libs/miwayomi-all.jar" "${JAR}"
fi

command -v unzip >/dev/null 2>&1 || die "unzip not found"
unzip -l "${JAR}" | grep -q "eu/kanade/tachiyomi/AppInfo.class" \
    || die "host jar is missing the AppInfo patch; rebuild with scripts/host-patches"
unzip -l "${JAR}" | grep -q "eu/kanade/tachiyomi/source/model/SMangaUpdate.class" \
    || die "host jar is missing the SMangaUpdate patch; rebuild with scripts/host-patches"

# A fixed module list is more reliable than `jdeps` on a shaded jar. Modules that are
# genuinely unused cost little; a missing one breaks the host at runtime.
MODULES="java.base,java.desktop,java.logging,java.management,java.naming,java.net.http"
MODULES="${MODULES},java.security.jgss,java.sql,java.transaction.xa,java.xml,jdk.crypto.ec"
MODULES="${MODULES},jdk.httpserver,jdk.net,jdk.unsupported,jdk.zipfs"

echo "host-runtime: building JRE"
rm -rf "${JRE}"
if ! jlink \
    --add-modules "${MODULES}" \
    --strip-debug \
    --no-header-files \
    --no-man-pages \
    --compress=zip-6 \
    --output "${JRE}" 2>/dev/null; then
    # JDK 21 accepts zip-N; older JDKs only know numeric levels.
    jlink \
        --add-modules "${MODULES}" \
        --strip-debug \
        --no-header-files \
        --no-man-pages \
        --compress=2 \
        --output "${JRE}"
fi

echo "host-runtime: staging archive"
rm -rf "${STAGE}"
mkdir -p "${STAGE}"
cp "${JAR}" "${STAGE}/miwayomi.jar"
cp -a "${JRE}" "${STAGE}/jre"

mkdir -p "$(dirname "${OUT}")"
rm -f "${OUT}"
(cd "${STAGE}" && zip -q -r -9 "${OUT}" miwayomi.jar jre)

echo "host-runtime: wrote ${OUT} ($(du -h "${OUT}" | cut -f1))"
echo "host-runtime: next run \`cargo build --release\` or \`just dist\`"
