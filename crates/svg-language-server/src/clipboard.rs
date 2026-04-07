use std::{
    io::{Read as _, Write as _},
    process::{Command as ProcessCommand, Stdio},
    thread,
    time::{Duration, Instant},
};

const CLIPBOARD_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(25);

struct ClipboardCommandSpec {
    program: &'static str,
    args: &'static [&'static str],
}

#[cfg(target_os = "macos")]
const CLIPBOARD_COMMANDS: &[ClipboardCommandSpec] = &[ClipboardCommandSpec {
    program: "pbcopy",
    args: &[],
}];

#[cfg(target_os = "windows")]
const CLIPBOARD_COMMANDS: &[ClipboardCommandSpec] = &[
    ClipboardCommandSpec {
        program: "clip.exe",
        args: &[],
    },
    ClipboardCommandSpec {
        program: "clip",
        args: &[],
    },
];

#[cfg(all(unix, not(target_os = "macos")))]
const CLIPBOARD_COMMANDS: &[ClipboardCommandSpec] = &[
    ClipboardCommandSpec {
        program: "wl-copy",
        args: &[],
    },
    ClipboardCommandSpec {
        program: "xclip",
        args: &["-selection", "clipboard"],
    },
    ClipboardCommandSpec {
        program: "xsel",
        args: &["--clipboard", "--input"],
    },
];

pub fn copy_text_to_system_clipboard(text: &str) -> std::result::Result<(), String> {
    let mut attempts = Vec::new();

    for command in CLIPBOARD_COMMANDS {
        match run_clipboard_command(command, text) {
            Ok(()) => return Ok(()),
            Err(err) => attempts.push(format!("{}: {err}", command.program)),
        }
    }

    let commands = CLIPBOARD_COMMANDS
        .iter()
        .map(|command| command.program)
        .collect::<Vec<_>>()
        .join(", ");
    if CLIPBOARD_COMMANDS.is_empty() {
        Err(
            "Clipboard unavailable. No supported clipboard command configured for this platform."
                .to_string(),
        )
    } else {
        Err(format!(
            "Clipboard unavailable. Tried {commands}. {}",
            attempts.join("; ")
        ))
    }
}

fn run_clipboard_command(
    command: &ClipboardCommandSpec,
    text: &str,
) -> std::result::Result<(), String> {
    let mut child = ProcessCommand::new(command.program)
        .args(command.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| err.to_string())?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "stdin unavailable".to_owned())?;
    if let Err(err) = stdin.write_all(text.as_bytes()) {
        drop(stdin);
        let _ = child.kill();
        let _ = child.wait();
        return Err(err.to_string());
    }
    drop(stdin);

    let output = wait_with_output_timeout(child)?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        Err(format!("exited with status {}", output.status))
    } else {
        Err(stderr)
    }
}

fn wait_with_output_timeout(
    mut child: std::process::Child,
) -> std::result::Result<std::process::Output, String> {
    let deadline = Instant::now() + CLIPBOARD_COMMAND_TIMEOUT;

    loop {
        match child.try_wait().map_err(|err| err.to_string())? {
            Some(status) if status.success() => {
                return Ok(std::process::Output {
                    status,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                });
            }
            Some(status) => {
                let mut stderr = Vec::new();
                if let Some(mut stderr_pipe) = child.stderr.take() {
                    stderr_pipe
                        .read_to_end(&mut stderr)
                        .map_err(|err| err.to_string())?;
                }
                return Ok(std::process::Output {
                    status,
                    stdout: Vec::new(),
                    stderr,
                });
            }
            None if Instant::now() < deadline => thread::sleep(CLIPBOARD_POLL_INTERVAL),
            None => {
                let _ = child.kill();
                let output = child.wait_with_output().map_err(|err| err.to_string())?;
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                if stderr.is_empty() {
                    return Err("clipboard command timed out".to_owned());
                }
                return Err(format!("clipboard command timed out: {stderr}"));
            }
        }
    }
}

pub fn svg_data_uri(svg: &str) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(svg);
    format!("data:image/svg+xml;base64,{encoded}")
}
