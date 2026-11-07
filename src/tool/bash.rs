use std::{
    fs,
    io::{Read, Seek},
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

use serde::Deserialize;

use super::Tool;

#[derive(Deserialize)]
struct BashToolArgs {
    command: String,
    cwd: Option<String>,
    timeout: Option<u64>,
}

pub struct BashTool {
    stdout_last_n_lines: usize,
    stderr_last_n_lines: usize,
}

impl BashTool {
    pub fn new(stdout_n: usize, stderr_n: usize) -> Self {
        Self {
            stdout_last_n_lines: stdout_n,
            stderr_last_n_lines: stderr_n,
        }
    }
}

impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json! {
            {
                "type": "function",
                "function": {
                    "name": "bash",
                    "description": "Run a shell command via `bash -c ...`. Returns combined stdout+stderr plus exit code. If the output is too long, it will save full output to temp file and leave a mark `|Omitted N lines(s)|` in the result, use `read` tool to read it if you need to.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": {
                                "type": "string",
                                "description": "shell command to run"
                            },
                            "cwd": {
                                "type": "string",
                                "description": "Working directory to run the command in. Omitted or empty uses the current directory"
                            },
                            "timeout": {
                                "type": "integer",
                                "description": "Timeout in seconds in range 1..120. Defaults to 60. Command is killed when it exceeds the timeout"
                            }
                        },
                        "required": ["command"]
                    }
                }
            }
        }
    }

    fn call(&mut self, args: &str) -> anyhow::Result<String> {
        self.call_cancellable(args, &AtomicBool::new(false))
    }

    fn call_cancellable(&mut self, args: &str, cancelled: &AtomicBool) -> anyhow::Result<String> {
        let args: BashToolArgs = serde_json::from_str(args)?;
        let timeout = args.timeout.unwrap_or(60);
        if !(1..=120).contains(&timeout) {
            anyhow::bail!("timeout must be in range 1..120, got {timeout}");
        }
        let mut cmd = Command::new("timeout");
        cmd.arg("-s").arg("KILL");
        cmd.arg(format!("{timeout}s"));
        cmd.arg("bash").arg("-c").arg(&args.command);
        if let Some(cwd) = args.cwd.filter(|cwd| !cwd.is_empty()) {
            cmd.current_dir(cwd);
        }
        let mut stdout_file = tempfile::tempfile()?;
        let mut stderr_file = tempfile::tempfile()?;
        cmd.stdin(Stdio::null())
            .stdout(stdout_file.try_clone()?)
            .stderr(stderr_file.try_clone()?)
            .process_group(0);
        if cancelled.load(Ordering::Relaxed) {
            return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
        }
        let mut child = cmd.spawn()?;
        let status = loop {
            if cancelled.load(Ordering::Relaxed) {
                let killed = Command::new("kill")
                    .args(["-KILL", "--", &format!("-{}", child.id())])
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                anyhow::ensure!(killed?.success(), "failed to kill command process group");
                return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            thread::sleep(Duration::from_millis(10));
        };
        stdout_file.rewind()?;
        stderr_file.rewind()?;
        let mut stdout_bytes = Vec::new();
        let mut stderr_bytes = Vec::new();
        stdout_file.read_to_end(&mut stdout_bytes)?;
        stderr_file.read_to_end(&mut stderr_bytes)?;
        let mut stdout = String::from_utf8_lossy(&stdout_bytes);
        let mut stderr = String::from_utf8_lossy(&stderr_bytes);
        let exit_code = status.code().unwrap_or(-1);

        let should_cut_stdout = stdout.lines().count() > self.stdout_last_n_lines;
        let should_cut_stderr = stderr.lines().count() > self.stderr_last_n_lines;

        let combined = if should_cut_stdout || should_cut_stderr {
            let temp_file_path = tempfile::NamedTempFile::new()?.into_temp_path().keep()?;
            fs::write(
                &temp_file_path,
                [stdout.as_ref(), stderr.as_ref()].join("\n"),
            )?;

            if !stdout.is_empty() && should_cut_stdout {
                stdout = format_cut(&stdout, self.stdout_last_n_lines).into();
            }

            if !stderr.is_empty() && should_cut_stderr {
                stderr = format_cut(&stderr, self.stderr_last_n_lines).into();
            }

            format_output(Some(&temp_file_path), exit_code, &stdout, &stderr)
        } else {
            format_output(None, exit_code, &stdout, &stderr)
        };

        Ok(combined)
    }
}

fn format_output(path: Option<&Path>, exit_code: i32, stdout: &str, stderr: &str) -> String {
    let header = match path {
        Some(path) => format!(
            "Full output file path: {}\nExit code: {}",
            path.display(),
            exit_code
        ),
        None => format!("Exit code: {}", exit_code),
    };
    if stdout.is_empty() && stderr.is_empty() {
        header
    } else if stdout.is_empty() {
        format!("{}\n{}", header, stderr)
    } else if stderr.is_empty() {
        format!("{}\n{}", header, stdout)
    } else {
        format!("{}\nSTDOUT:\n{}\nSTDERR:\n{}", header, stdout, stderr)
    }
}

fn format_cut(s: &str, cut: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let n = lines.len() - cut;
    let s = lines[lines.len() - cut..].join("\n");
    let line_str = if n == 1 {
        "1 line".to_string()
    } else {
        format!("{n} lines")
    };
    format!("|Omitted {line_str}|\n...\n{s}")
}
