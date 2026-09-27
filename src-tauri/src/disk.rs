//! Is the disk holding the clinic's data encrypted (FileVault on macOS, BitLocker on Windows)?
//! Backups are not encrypted by the app (owner decision, DEC-038), so disk encryption is what
//! protects the database and backups if the computer is lost or stolen. Checked once per run.

use std::path::Path;
use std::sync::OnceLock;

static STATUS: OnceLock<&'static str> = OnceLock::new();

/// "ON", "OFF" or "UNKNOWN".
pub fn encryption_status(data_dir: &Path) -> &'static str {
    *STATUS.get_or_init(|| detect(data_dir))
}

#[cfg(target_os = "macos")]
fn detect(_data_dir: &Path) -> &'static str {
    // `fdesetup status` needs no administrator rights: "FileVault is On." / "FileVault is Off."
    match std::process::Command::new("/usr/bin/fdesetup").arg("status").output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            if text.contains("FileVault is On") {
                "ON"
            } else if text.contains("FileVault is Off") {
                "OFF"
            } else {
                "UNKNOWN"
            }
        }
        Err(_) => "UNKNOWN",
    }
}

#[cfg(windows)]
fn detect(data_dir: &Path) -> &'static str {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // The drive of the data folder, e.g. "C:". The shell's BitLockerProtection property needs no
    // administrator rights: 1 = on, 3 = encrypting, 2 = off.
    let drive: String = data_dir.to_string_lossy().chars().take(2).collect();
    if drive.len() != 2 || !drive.ends_with(':') {
        return "UNKNOWN";
    }
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(New-Object -ComObject Shell.Application).NameSpace($env:SKINDOC_DRIVE).Self.ExtendedProperty('System.Volume.BitLockerProtection')",
        ])
        .env("SKINDOC_DRIVE", format!("{drive}\\"))
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()) {
        Ok(value) if value == "1" || value == "3" => "ON",
        Ok(value) if value == "2" => "OFF",
        _ => "UNKNOWN",
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn detect(_data_dir: &Path) -> &'static str {
    "UNKNOWN"
}
