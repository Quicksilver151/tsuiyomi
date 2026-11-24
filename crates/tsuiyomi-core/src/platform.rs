//! Operating-system integration shared by the TUI and core services.
//!
//! Opening a target, copying text, and desktop notifications all need one command per
//! platform. Keeping the branches here stops the story, store, reader, and OAuth actions
//! from drifting apart, and gives the Windows paths one place to be tested.

use std::{
    io,
    path::Path,
    process::{Command, Stdio},
};

/// Open a URL, file, or directory with the platform's default handler.
///
/// Windows uses `cmd /C start`; the empty argument is the window title, which `start`
/// would otherwise take from a quoted target.
pub fn open_target(target: &str) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd.exe")
            .args(["/C", "start", "", target])
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(target).spawn().map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(target).spawn().map(|_| ())
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = target;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no system opener for this platform",
        ))
    }
}

/// Copy text to the native clipboard.
///
/// Linux has no single native command, so this returns `Unsupported` and the caller
/// falls back to an OSC 52 terminal sequence. Windows uses PowerShell so non-ASCII text
/// survives; `clip.exe` reads the legacy console code page and mangles UTF-8.
pub fn copy_to_clipboard(text: &str) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(text);
        let script = format!(
            "Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')))"
        );
        let status = powershell(&script)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("Set-Clipboard failed"))
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        let mut child = Command::new("pbcopy").stdin(Stdio::piped()).spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("pbcopy failed"))
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = text;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "use an OSC 52 clipboard sequence",
        ))
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = text;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no native clipboard for this platform",
        ))
    }
}

/// Send a best-effort desktop notification.
///
/// Linux uses `notify-send`, macOS uses Notification Center, and Windows uses a
/// PowerShell toast. None of these are required for the TUI to work; a missing helper
/// only means the notification is dropped.
pub fn notify(title: &str, body: &str) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let xml = format!(
            "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
            xml_escape(title),
            xml_escape(body)
        );
        let script = format!(
            "$ErrorActionPreference = 'SilentlyContinue'; \
             [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
             $xml = New-Object Windows.Data.Xml.Dom.XmlDocument; \
             $xml.LoadXml({}); \
             $toast = [Windows.UI.Notifications.ToastNotification]::new($xml); \
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier().Show($toast)",
            powershell_quote(&xml)
        );
        powershell(&script)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            applescript_escape(body),
            applescript_escape(title)
        );
        Command::new("osascript")
            .args(["-e", &script])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("notify-send")
            .args([title, body])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = (title, body);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no desktop notifications for this platform",
        ))
    }
}

/// Move `temporary` over `target`, replacing an existing file.
///
/// Unix `rename` replaces atomically. Windows `MoveFileEx` fails when the destination
/// exists, so remove it first. The sidecars using this are small JSON/XML files where
/// the short non-atomic window is acceptable.
pub fn replace_file(temporary: &Path, target: &Path) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    match std::fs::remove_file(target) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    std::fs::rename(temporary, target)
}

#[cfg(target_os = "windows")]
fn powershell(script: &str) -> Command {
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        script,
    ]);
    command
}

#[cfg(target_os = "windows")]
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(target_os = "macos")]
fn applescript_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_file_replaces_an_existing_sidecar() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("progress.json");
        let temporary = root.path().join("progress.json.tmp");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(&temporary, b"new").unwrap();

        replace_file(&temporary, &target).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(!temporary.exists());
    }

    #[test]
    fn replace_file_creates_a_missing_target() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("updates.json");
        let temporary = root.path().join("updates.json.tmp");
        std::fs::write(&temporary, b"new").unwrap();

        replace_file(&temporary, &target).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }
}
