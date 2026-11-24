#requires -Version 5.1
<#
.SYNOPSIS
Build the embedded extension-host runtime archive on Windows.

.DESCRIPTION
Packages a patched miwayomi jar with a jlink'd Java 21 runtime into
crates/tsuiyomi-core/runtime/host.zip. The jar is platform-independent, so Windows
releases reuse the jar published under host-v0.2.9; this script does not rebuild it from
source. Use scripts/build-host-runtime.sh (Git Bash) for a full source build.

.PARAMETER JarPath
Patched miwayomi jar. Defaults to TSUIYOMI_HOST_JAR_PATH, then
target/host-runtime/miwayomi.jar.

.PARAMETER JdkHome
JDK 21 home. Defaults to JAVA_HOME, then java/jlink/jar on PATH.

.EXAMPLE
.\scripts\build-host-runtime.ps1 -JarPath .\miwayomi-0.2.9-patched.jar -JdkHome C:\jdk-21
#>
[CmdletBinding()]
param(
    [string]$JarPath = $env:TSUIYOMI_HOST_JAR_PATH,
    [string]$JdkHome = $env:JAVA_HOME
)

$ErrorActionPreference = 'Stop'
$HostVersion = '0.2.9'

$Root = Split-Path -Parent $PSScriptRoot
$Work = Join-Path $Root 'target\host-runtime'
$Out = Join-Path $Root 'crates\tsuiyomi-core\runtime\host.zip'
$Jar = Join-Path $Work 'miwayomi.jar'
$Jre = Join-Path $Work 'jre'
$Stage = Join-Path $Work 'stage'

function Find-JdkTool([string]$Name) {
    if ($JdkHome) {
        $tool = Join-Path $JdkHome "bin\$Name.exe"
        if (-not (Test-Path $tool)) {
            throw "Missing $tool; point -JdkHome at a JDK 21."
        }
        return $tool
    }
    $command = Get-Command "$Name.exe" -ErrorAction SilentlyContinue
    if (-not $command) {
        throw "$Name.exe was not found; install a JDK 21 or set JAVA_HOME."
    }
    return $command.Source
}

$Java = Find-JdkTool 'java'
$JarTool = Find-JdkTool 'jar'
$Jlink = Find-JdkTool 'jlink'

New-Item -ItemType Directory -Force -Path $Work | Out-Null

if ($JarPath) {
    if (-not (Test-Path $JarPath)) {
        throw "Host jar not found: $JarPath"
    }
    Copy-Item -Force $JarPath $Jar
}
if (-not (Test-Path $Jar)) {
    throw "No host jar at $Jar. Pass -JarPath, set TSUIYOMI_HOST_JAR_PATH, or build it with scripts/build-host-runtime.sh."
}

& $Java -version
if ($LASTEXITCODE -ne 0) {
    throw "The JVM at $Java did not run."
}

$entries = & $JarTool --list --file $Jar
if ($LASTEXITCODE -ne 0) {
    throw "Could not list the host jar: $Jar"
}
if (-not ($entries -match 'eu/kanade/tachiyomi/AppInfo.class')) {
    throw 'Host jar is missing the AppInfo patch; rebuild it with scripts/host-patches.'
}
if (-not ($entries -match 'eu/kanade/tachiyomi/source/model/SMangaUpdate.class')) {
    throw 'Host jar is missing the SMangaUpdate patch; rebuild it with scripts/host-patches.'
}

# A fixed module list is more reliable than jdeps on a shaded jar. A missing module
# breaks the host at runtime; an unused one only costs a little space.
$Modules = @(
    'java.base', 'java.desktop', 'java.logging', 'java.management', 'java.naming',
    'java.net.http', 'java.security.jgss', 'java.sql', 'java.transaction.xa', 'java.xml',
    'jdk.crypto.ec', 'jdk.httpserver', 'jdk.net', 'jdk.unsupported', 'jdk.zipfs'
) -join ','

Write-Host 'host-runtime: building JRE'
if (Test-Path $Jre) {
    Remove-Item -Recurse -Force $Jre
}
& $Jlink --add-modules $Modules --strip-debug --no-header-files --no-man-pages `
    --compress=zip-6 --output $Jre
if ($LASTEXITCODE -ne 0) {
    # JDK 21 accepts zip-N; older JDKs only know numeric levels.
    if (Test-Path $Jre) {
        Remove-Item -Recurse -Force $Jre
    }
    & $Jlink --add-modules $Modules --strip-debug --no-header-files --no-man-pages `
        --compress=2 --output $Jre
    if ($LASTEXITCODE -ne 0) {
        throw 'jlink failed.'
    }
}
if (-not (Test-Path (Join-Path $Jre 'bin\java.exe'))) {
    throw "The generated JRE has no bin\java.exe: $Jre"
}

Write-Host 'host-runtime: staging archive'
if (Test-Path $Stage) {
    Remove-Item -Recurse -Force $Stage
}
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item -Force $Jar (Join-Path $Stage 'miwayomi.jar')
Copy-Item -Recurse -Force $Jre (Join-Path $Stage 'jre')

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
if (Test-Path $Out) {
    Remove-Item -Force $Out
}
Push-Location $Stage
try {
    & $JarTool --create --file $Out miwayomi.jar jre
    if ($LASTEXITCODE -ne 0) {
        throw 'jar failed while writing the runtime archive.'
    }
}
finally {
    Pop-Location
}

$size = '{0:N1} MB' -f ((Get-Item $Out).Length / 1MB)
Write-Host "host-runtime: wrote $Out ($size, host $HostVersion)"
Write-Host 'host-runtime: next run cargo build --release or just dist'
