//! "Start with Windows": a value named Thrasher under the current user's Run key.

use std::process::{Command, Stdio};

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

fn reg(args: &[&str]) -> bool {
    let mut cmd = Command::new("reg");
    cmd.args(args).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashes up.
        cmd.creation_flags(0x0800_0000);
    }
    cmd.status().is_ok_and(|s| s.success())
}

pub fn enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", "Thrasher"])
}

/// Turns starting with Windows on or off. Returns false if Windows refused.
pub fn set(on: bool) -> bool {
    if !on {
        return reg(&["delete", RUN_KEY, "/v", "Thrasher", "/f"]);
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let value = format!("\"{}\"", exe.display());
    reg(&[
        "add", RUN_KEY, "/v", "Thrasher", "/t", "REG_SZ", "/d", &value, "/f",
    ])
}
