//! Lifecycle for the bundled extension host.
//!
//! The host is a JVM process. tsuiyomi ships it as a compressed archive inside the
//! executable, extracts it once into a versioned cache directory, starts it on loopback,
//! and stops it when the TUI exits. If the user points `host_url` at an existing server,
//! none of this runs.

use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Cursor, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{error::CoreError, host::HostClient};

/// Pinned host version. Bump together with the embedded archive.
pub const HOST_VERSION: &str = "0.2.9";

const START_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Resolved `java` executable and host jar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRuntime {
    pub java: PathBuf,
    pub jar: PathBuf,
}

impl HostRuntime {
    /// Locate a runnable host.
    ///
    /// Order: explicit jar override, `TSUIYOMI_HOST_JAR`, then the versioned cache.
    /// The JVM is taken from the cache first, then `TSUIYOMI_JAVA`, `JAVA_HOME`, and
    /// finally `java` on `PATH`.
    pub fn discover(jar_override: Option<&Path>) -> Option<Self> {
        let jar = jar_override
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("TSUIYOMI_HOST_JAR").map(PathBuf::from))
            .or_else(|| cached_runtime_dir().map(|dir| dir.join("miwayomi.jar")))?;
        if !jar.is_file() {
            return None;
        }

        let java = java_candidates()
            .into_iter()
            .find(|candidate| candidate.is_file())?;

        Some(Self { java, jar })
    }
}

fn java_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = cached_runtime_dir() {
        candidates.push(dir.join("jre").join("bin").join(java_binary_name()));
    }
    if let Some(java) = std::env::var_os("TSUIYOMI_JAVA") {
        candidates.push(PathBuf::from(java));
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(home).join("bin").join(java_binary_name()));
    }
    if let Some(java) = find_on_path("java") {
        candidates.push(java);
    }
    candidates
}

/// The JVM launcher is `java.exe` on Windows and `java` everywhere else.
fn java_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    }
}

/// Find a program on `PATH`.
///
/// Windows resolves executables through `PATHEXT`, so `java` must be looked up as
/// `java.exe` (or another listed extension) rather than as a literal extensionless file.
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names = executable_names(name);
    for directory in std::env::split_paths(&path) {
        for name in &names {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn executable_names(name: &str) -> Vec<String> {
    if !cfg!(target_os = "windows") {
        return vec![name.to_owned()];
    }
    if Path::new(name).extension().is_some() {
        return vec![name.to_owned()];
    }

    let pathext = std::env::var_os("PATHEXT")
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_owned());
    names_for_pathext(name, &pathext)
}

fn names_for_pathext(name: &str, pathext: &str) -> Vec<String> {
    let mut names = Vec::new();
    for extension in pathext.split(';').filter(|value| !value.is_empty()) {
        let extension = extension.to_ascii_lowercase();
        let candidate = format!("{name}{extension}");
        if !names.contains(&candidate) {
            names.push(candidate);
        }
    }
    if !names.iter().any(|candidate| {
        candidate
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("exe"))
    }) {
        names.push(format!("{name}.exe"));
    }
    names
}

/// `$XDG_CACHE_HOME/tsuiyomi/host/<version>`.
pub fn cached_runtime_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| {
        dirs.cache_dir()
            .join("tsuiyomi")
            .join("host")
            .join(HOST_VERSION)
    })
}

/// `$XDG_DATA_HOME/tsuiyomi/host`, containing extensions and the host database.
pub fn host_data_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("tsuiyomi").join("host"))
}

/// `$XDG_DATA_HOME/tsuiyomi/covers/<hash>.img` for a saved remote manga.
///
/// A source's cover URL is a proxy URL tied to one host process, so saving the URL
/// alone loses the cover after a restart. The image bytes are cached under a stable
/// name derived from the source and manga URL. This lives in the data directory
/// rather than the cache directory so routine cache cleanup does not remove saved
/// library artwork.
pub fn covers_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("tsuiyomi").join("covers"))
}

pub fn remote_cover_path(source_id: &str, manga_url: &str) -> Option<PathBuf> {
    covers_dir().map(|dir| {
        dir.join(format!(
            "{}.img",
            stable_hash(&format!("{source_id}\n{manga_url}"))
        ))
    })
}

/// Previous cover location (`$XDG_CACHE_HOME/tsuiyomi/covers`), used for migration.
pub fn legacy_remote_cover_path(source_id: &str, manga_url: &str) -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| {
        dirs.cache_dir()
            .join("tsuiyomi")
            .join("covers")
            .join(format!(
                "{}.img",
                stable_hash(&format!("{source_id}\n{manga_url}"))
            ))
    })
}

fn stable_hash(value: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Extract the embedded runtime archive on first use, then return the layout.
///
/// Returns `Ok(None)` when this build has no embedded archive and no runtime is cached.
pub fn ensure_runtime(jar_override: Option<&Path>) -> Result<Option<HostRuntime>, CoreError> {
    if let Some(bytes) = embedded_archive() {
        sync_embedded_runtime(bytes)?;
    }
    Ok(HostRuntime::discover(jar_override))
}

/// Extract the embedded runtime when the cache is missing or was built from another
/// archive. Without the stamp, a rebuilt binary would keep using the previous jar.
fn sync_embedded_runtime(bytes: &[u8]) -> Result<(), CoreError> {
    let Some(target) = cached_runtime_dir() else {
        return Err(CoreError::Host {
            message: "cannot resolve the cache directory".to_owned(),
        });
    };
    let stamp = archive_stamp(bytes);
    let stamp_path = target.join(".archive-stamp");
    let cached = fs::read_to_string(&stamp_path)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok());
    if cached != Some(stamp) {
        // A host left behind by a crash can hold the cached jar or JRE open on Windows.
        // Stop it before trying to replace the directory.
        if let Some(data_dir) = host_data_dir() {
            stop_stale(&data_dir);
        }
        let _ = fs::remove_dir_all(&target);
        if target.is_dir() {
            return Err(CoreError::Host {
                message: format!(
                    "cannot replace the host runtime at {}; another process may be using it",
                    target.display()
                ),
            });
        }
    }
    if !target.is_dir() {
        extract_archive(bytes)?;
        let _ = fs::write(&stamp_path, stamp.to_string());
    }
    Ok(())
}

fn archive_stamp(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(embedded_host)]
fn embedded_archive() -> Option<&'static [u8]> {
    Some(include_bytes!(env!("TSUIYOMI_HOST_ARCHIVE")))
}

#[cfg(not(embedded_host))]
fn embedded_archive() -> Option<&'static [u8]> {
    None
}

/// Unpack the runtime archive into the versioned cache directory.
///
/// Extraction is staged in a sibling directory and renamed into place, so a crash never
/// leaves a half-written runtime that looks usable.
pub fn extract_archive(bytes: &[u8]) -> Result<PathBuf, CoreError> {
    let target = cached_runtime_dir().ok_or_else(|| CoreError::Host {
        message: "cannot resolve the cache directory".to_owned(),
    })?;
    if target.is_dir() {
        return Ok(target);
    }
    let Some(parent) = target.parent() else {
        return Err(CoreError::Host {
            message: format!("invalid runtime path {}", target.display()),
        });
    };
    fs::create_dir_all(parent).map_err(|source| CoreError::ReadDir {
        path: parent.to_path_buf(),
        source,
    })?;

    let staging = parent.join(format!("{}.{}.tmp", HOST_VERSION, std::process::id()));
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    fs::create_dir_all(&staging).map_err(|source| CoreError::ReadDir {
        path: staging.clone(),
        source,
    })?;

    let result = unpack_zip(bytes, &staging);
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    match fs::rename(&staging, &target) {
        Ok(()) => Ok(target),
        Err(_error) if target.is_dir() => {
            // Another process won the race.
            let _ = fs::remove_dir_all(&staging);
            Ok(target)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(CoreError::Host {
                message: format!("failed to install runtime at {}: {error}", target.display()),
            })
        }
    }
}

fn unpack_zip(bytes: &[u8], target: &Path) -> Result<(), CoreError> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| CoreError::Host {
            message: format!("invalid runtime archive: {error}"),
        })?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| CoreError::Host {
            message: format!("invalid runtime entry: {error}"),
        })?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let path = target.join(&relative);

        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(|source| CoreError::ReadDir {
                path: path.clone(),
                source,
            })?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| CoreError::ReadDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let mut file = fs::File::create(&path).map_err(|source| CoreError::ReadFile {
            path: path.clone(),
            source,
        })?;
        io::copy(&mut entry, &mut file).map_err(|source| CoreError::ReadFile {
            path: path.clone(),
            source,
        })?;

        #[cfg(unix)]
        if relative.starts_with("jre/bin") {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o755));
        }
    }

    Ok(())
}

/// A running host process. Dropping it terminates the JVM.
#[derive(Debug)]
pub struct HostProcess {
    child: Child,
    base_url: String,
    data_dir: PathBuf,
}

impl HostProcess {
    /// Start the host on a free loopback port and wait until it answers `/health`.
    pub fn start(
        runtime: &HostRuntime,
        data_dir: &Path,
        flaresolverr_url: Option<&str>,
    ) -> Result<Self, CoreError> {
        fs::create_dir_all(data_dir).map_err(|source| CoreError::ReadDir {
            path: data_dir.to_path_buf(),
            source,
        })?;
        disable_self_update(data_dir);
        stop_stale(data_dir);

        let port = free_port()?;
        let base_url = format!("http://127.0.0.1:{port}");

        let log_path = data_dir.join("host.log");
        let log = fs::File::create(&log_path).map_err(|source| CoreError::ReadFile {
            path: log_path.clone(),
            source,
        })?;
        let log_err = log.try_clone().map_err(|source| CoreError::ReadFile {
            path: log_path.clone(),
            source,
        })?;

        let child = Command::new(&runtime.java)
            .arg("-jar")
            .arg(&runtime.jar)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--data")
            .arg(data_dir)
            .arg("--no-open")
            .arg("--flaresolverr")
            .arg(flaresolverr_url.unwrap_or(""))
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err))
            .spawn()
            .map_err(|error| CoreError::Host {
                message: format!("failed to start {}: {error}", runtime.java.display()),
            })?;

        let mut process = Self {
            child,
            base_url,
            data_dir: data_dir.to_path_buf(),
        };
        process.write_pid()?;
        process.wait_until_ready()?;
        Ok(process)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn log_path(&self) -> PathBuf {
        self.data_dir.join("host.log")
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn write_pid(&mut self) -> Result<(), CoreError> {
        let path = self.data_dir.join("host.pid");
        let mut file = fs::File::create(&path).map_err(|source| CoreError::ReadFile {
            path: path.clone(),
            source,
        })?;
        write!(file, "{}", self.child.id()).map_err(|source| CoreError::ReadFile { path, source })
    }

    fn wait_until_ready(&mut self) -> Result<(), CoreError> {
        let client = HostClient::new(self.base_url.clone());
        let deadline = Instant::now() + START_TIMEOUT;

        loop {
            if client.health().is_ok() {
                return Ok(());
            }
            if let Some(status) = self.child.try_wait().map_err(|error| CoreError::Host {
                message: error.to_string(),
            })? {
                return Err(CoreError::Host {
                    message: format!(
                        "host exited with {status}; see {}",
                        self.log_path().display()
                    ),
                });
            }
            if Instant::now() >= deadline {
                self.stop();
                return Err(CoreError::Host {
                    message: format!(
                        "host did not become ready within {}s; see {}",
                        START_TIMEOUT.as_secs(),
                        self.log_path().display()
                    ),
                });
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(self.data_dir.join("host.pid"));
    }
}

impl Drop for HostProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

fn free_port() -> Result<u16, CoreError> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| CoreError::Host {
        message: format!("no free loopback port: {error}"),
    })?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| CoreError::Host {
            message: error.to_string(),
        })
}

/// Block miwayomi's self-updater without patching the jar.
///
/// The host downloads `update/miwayomi-all.jar.new` and swaps it over the running jar on
/// the next boot. In a pinned bundle that would silently replace the version tsuiyomi was
/// built against. Creating a directory at the download path makes both the download and
/// the swap fail harmlessly; the host treats both as best-effort.
///
/// Replace this with a patched jar once the distribution build vendors the host.
fn disable_self_update(data_dir: &Path) {
    let update_dir = data_dir.join("update");
    let _ = fs::create_dir_all(&update_dir);
    let guard = update_dir.join("miwayomi-all.jar.new");
    if !guard.is_dir() {
        let _ = fs::remove_file(&guard);
        let _ = fs::create_dir(&guard);
    }
}

/// Terminate a host left behind by a previous crash, if the pid still belongs to one.
fn stop_stale(data_dir: &Path) {
    let path = data_dir.join("host.pid");
    let Ok(contents) = fs::read_to_string(&path) else {
        return;
    };
    let Ok(pid) = contents.trim().parse::<u32>() else {
        let _ = fs::remove_file(&path);
        return;
    };
    if pid == std::process::id() || !looks_like_host(pid) {
        let _ = fs::remove_file(&path);
        return;
    }
    stop_pid(pid);
    thread::sleep(Duration::from_millis(150));
    let _ = fs::remove_file(&path);
}

fn stop_pid(pid: u32) {
    let pid = pid.to_string();
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("taskkill");
        command.args(["/PID", pid.as_str(), "/T", "/F"]);
        command
    };
    #[cfg(not(target_os = "windows"))]
    let mut command = {
        let mut command = Command::new("kill");
        command.arg("-TERM").arg(pid.as_str());
        command
    };
    let _ = command.stdout(Stdio::null()).stderr(Stdio::null()).status();
}

#[cfg(target_os = "linux")]
fn looks_like_host(pid: u32) -> bool {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|bytes| {
            let command = String::from_utf8_lossy(&bytes);
            command.contains("miwayomi")
        })
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn looks_like_host(pid: u32) -> bool {
    // Prefer the command line: after a crash the pid may belong to an unrelated Java
    // process. CIM is available on Windows 10 and 11; tasklist is the fallback when
    // PowerShell or WMI is disabled.
    let script = format!("(Get-CimInstance Win32_Process -Filter 'ProcessId = {pid}').CommandLine");
    if let Ok(output) = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script.as_str(),
        ])
        .output()
    {
        if output.status.success() {
            let command = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
            return command.contains("miwayomi");
        }
    }

    let filter = format!("PID eq {pid}");
    Command::new("tasklist")
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .output()
        .map(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .to_ascii_lowercase()
                    .contains("java")
        })
        .unwrap_or(false)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn looks_like_host(pid: u32) -> bool {
    Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .map(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .to_ascii_lowercase()
                    .contains("miwayomi")
        })
        .unwrap_or(false)
}

#[cfg(not(any(unix, target_os = "windows")))]
fn looks_like_host(_pid: u32) -> bool {
    false
}

/// SHA-256 fingerprint of an installed extension file.
pub fn extension_fingerprint(apk_name: &str) -> Option<String> {
    let dir = host_data_dir()?.join("extensions");
    std::fs::read(dir.join(apk_name))
        .ok()
        .map(|bytes| fingerprint_bytes(&bytes))
}

pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_are_stable_sha256_hex() {
        let first = fingerprint_bytes(b"extension");
        assert_eq!(first.len(), 64);
        assert_eq!(first, fingerprint_bytes(b"extension"));
        assert_ne!(first, fingerprint_bytes(b"other"));
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
    }

    #[test]
    fn cover_cache_paths_are_stable_and_distinct() {
        let first = remote_cover_path("7", "/manga/1").unwrap();
        let again = remote_cover_path("7", "/manga/1").unwrap();
        let other = remote_cover_path("7", "/manga/2").unwrap();

        assert_eq!(first, again);
        assert_ne!(first, other);
        assert!(first
            .parent()
            .is_some_and(|parent| parent.ends_with(Path::new("tsuiyomi").join("covers"))));
    }

    #[test]
    fn windows_path_extensions_are_expanded() {
        assert_eq!(
            names_for_pathext("java", ".COM;.EXE;.BAT;.CMD"),
            vec!["java.com", "java.exe", "java.bat", "java.cmd"]
        );
        assert_eq!(
            names_for_pathext("java", ".COM"),
            vec!["java.com", "java.exe"]
        );
        assert_eq!(
            names_for_pathext("java", ".COM;.COM"),
            vec!["java.com", "java.exe"]
        );
    }

    #[test]
    fn extracts_a_runtime_archive() {
        let root = tempfile::tempdir().unwrap();
        let zip_path = root.path().join("runtime.zip");
        write_test_archive(&zip_path);

        let bytes = fs::read(&zip_path).unwrap();
        let target = root.path().join("runtime");
        let archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        assert_eq!(archive.len(), 2);

        unpack_zip(&bytes, &target).unwrap();
        assert!(target.join("miwayomi.jar").is_file());
        assert!(target.join("jre/bin/java").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn marks_jre_binaries_executable() {
        let root = tempfile::tempdir().unwrap();
        let zip_path = root.path().join("runtime.zip");
        write_test_archive(&zip_path);
        let bytes = fs::read(&zip_path).unwrap();
        let target = root.path().join("runtime");

        unpack_zip(&bytes, &target).unwrap();

        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(target.join("jre/bin/java"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
    }

    fn write_test_archive(path: &Path) {
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("miwayomi.jar", options).unwrap();
        zip.write_all(b"jar").unwrap();
        zip.start_file("jre/bin/java", options).unwrap();
        zip.write_all(b"#!/bin/sh\n").unwrap();
        zip.finish().unwrap();
    }

    #[cfg(embedded_host)]
    #[test]
    #[ignore = "extracts into the real XDG cache directory"]
    fn extracts_the_embedded_runtime() {
        let runtime = ensure_runtime(None)
            .expect("extraction")
            .expect("embedded runtime");
        assert!(runtime.jar.is_file());
        assert!(runtime.java.is_file());
        assert!(runtime.jar.file_name().unwrap() == "miwayomi.jar");
    }

    #[test]
    fn blocks_host_self_updates() {
        let root = tempfile::tempdir().unwrap();
        disable_self_update(root.path());
        assert!(root.path().join("update/miwayomi-all.jar.new").is_dir());
    }

    #[test]
    fn missing_runtime_is_not_an_error_when_nothing_is_embedded() {
        // `discover` must never panic on a machine with no JVM or jar.
        let _ = HostRuntime::discover(Some(Path::new("/nonexistent/miwayomi.jar")));
    }
}
