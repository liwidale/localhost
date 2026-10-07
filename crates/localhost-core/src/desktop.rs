use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

fn detached(mut cmd: Command) -> io::Result<()> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn checked_dir(dir: &Path) -> io::Result<&Path> {
    if dir.is_dir() {
        Ok(dir)
    } else {
        Err(io::Error::new(io::ErrorKind::NotFound, "folder not found"))
    }
}

pub fn open_terminal(dir: &Path) -> io::Result<()> {
    let dir = checked_dir(dir)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;
        if let Ok(wt) = which::which("wt")
            && !dir.to_string_lossy().contains(';')
        {
            let mut cmd = Command::new(wt);
            cmd.arg("-d").arg(dir);
            return detached(cmd);
        }
        let shell = which::which("pwsh")
            .or_else(|_| which::which("powershell"))
            .map_err(io::Error::other)?;
        let mut cmd = Command::new(shell);
        cmd.arg("-NoExit")
            .current_dir(dir)
            .creation_flags(CREATE_NEW_CONSOLE);
        cmd.spawn().map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        cmd.args(["-a", "Terminal"]).arg(dir);
        detached(cmd)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let preferred = std::env::var("TERMINAL").ok();
        let candidates = preferred.iter().map(String::as_str).chain([
            "x-terminal-emulator",
            "gnome-terminal",
            "konsole",
            "kitty",
            "alacritty",
            "wezterm",
            "foot",
            "xterm",
        ]);
        for name in candidates {
            if let Ok(path) = which::which(name) {
                let mut cmd = Command::new(path);
                cmd.current_dir(dir);
                return detached(cmd);
            }
        }
        Err(io::Error::new(io::ErrorKind::NotFound, "no terminal found"))
    }
}

pub const EDITORS: &[(&str, &str)] = &[
    ("code", "Visual Studio Code"),
    ("cursor", "Cursor"),
    ("zed", "Zed"),
    ("subl", "Sublime Text"),
    ("idea", "IntelliJ IDEA"),
    ("webstorm", "WebStorm"),
];

pub fn editor() -> Option<(&'static str, &'static str)> {
    EDITORS
        .iter()
        .find(|(bin, _)| which::which(bin).is_ok())
        .copied()
}

pub fn open_editor(dir: &Path) -> io::Result<()> {
    let dir = checked_dir(dir)?;
    let (bin, _) = editor().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no editor"))?;
    let mut cmd = Command::new(which::which(bin).map_err(io::Error::other)?);
    cmd.arg(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    detached(cmd)
}
