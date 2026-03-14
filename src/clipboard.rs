use std::process::{Command, Stdio};
use std::sync::OnceLock;

#[derive(Debug)]
#[allow(dead_code)]
enum Provider {
    XClip,
    XSel,
    Arboard,
}

static PROVIDER: OnceLock<Provider> = OnceLock::new();

#[cfg(all(unix, not(target_os = "macos")))]
fn binary_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn detect() -> Provider {
    // X11 clipboard is broken with arboard in TUI apps (provider model conflict).
    // Shell out to xclip/xsel instead — same fix as Helix editor.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let is_x11 = std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty())
            && std::env::var_os("WAYLAND_DISPLAY").map_or(true, |v| v.is_empty());

        if is_x11 {
            if binary_exists("xclip") {
                return Provider::XClip;
            }
            if binary_exists("xsel") {
                return Provider::XSel;
            }
        }
    }

    Provider::Arboard
}

fn run_command(cmd: &str, args: &[&str], text: &str) -> Result<(), String> {
    use std::io::Write;

    let mut command = Command::new(cmd);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn {cmd}: {e}"))?;

    let write_err = child.stdin.take().and_then(|mut stdin| {
        let result = stdin.write_all(text.as_bytes());
        drop(stdin);
        result.err()
    });

    let output = child
        .wait_with_output()
        .map_err(|e| format!("{cmd} failed: {e}"))?;

    if let Some(e) = write_err {
        return Err(format!("Failed to write to {cmd}: {e}"));
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{cmd} failed: {stderr}"));
    }

    Ok(())
}

pub fn init() {
    PROVIDER.get_or_init(|| {
        let p = detect();
        log::info!("Clipboard provider: {p:?}");
        p
    });
}

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let provider = PROVIDER.get_or_init(|| {
        let p = detect();
        log::info!("Clipboard provider: {p:?}");
        p
    });

    match provider {
        Provider::XClip => run_command("xclip", &["-selection", "clipboard"], text),
        Provider::XSel => run_command("xsel", &["-i", "-b"], text),
        Provider::Arboard => {
            let mut cb = arboard::Clipboard::new()
                .map_err(|e| format!("Failed to access clipboard: {e}"))?;
            cb.set_text(text)
                .map_err(|e| format!("Failed to copy to clipboard: {e}"))
        }
    }
}
