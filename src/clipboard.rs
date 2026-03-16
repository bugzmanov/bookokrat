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
        .stderr(Stdio::null());

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

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("Failed to write to {cmd}: {e}"))?;
    }

    // Don't wait — xclip forks a daemon that inherits our pipe fds,
    // so wait_with_output() would block until the daemon exits.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_command_does_not_block_on_forking_child() {
        // Simulates xclip behavior: read stdin, fork a long-lived daemon, parent exits.
        // Before the fix, wait_with_output() with piped stderr would block here
        // because the forked child inherits the stderr fd.
        let start = std::time::Instant::now();
        run_command("sh", &["-c", "cat > /dev/null; (sleep 30) &"], "test").unwrap();
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "run_command blocked for {:?} — likely waiting on forked child",
            start.elapsed()
        );
    }

    #[test]
    fn run_command_delivers_stdin_data() {
        let dir = tempfile::TempDir::new().unwrap();
        let out = dir.path().join("out.txt");
        let script = format!("cat > '{}'", out.display());
        run_command("sh", &["-c", &script], "hello clipboard").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let contents = std::fs::read_to_string(&out).unwrap();
        assert_eq!(contents, "hello clipboard");
    }
}
