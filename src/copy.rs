use std::{
    io::{self, Write},
    process::{Command, Stdio},
};

use base64::{Engine, engine::general_purpose::STANDARD};

fn copy_via_osc52(text: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    let encoded = STANDARD.encode(text);

    write!(stdout, "\x1b]52;c;{}\x07", encoded)?;
    stdout.flush()?;
    Ok(())
}

fn copy_via_xclip(user_prompt: &str) -> anyhow::Result<()> {
    let mut cmd = Command::new("xclip")
        .args(&["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = cmd.stdin.take() {
        stdin.write_all(user_prompt.as_bytes())?
    }

    cmd.wait()?;
    Ok(())
}

pub fn copy_user_prompt_to_clipboard(user_prompt: &str) -> anyhow::Result<()> {
    if copy_via_xclip(user_prompt).is_ok() {
        return Ok(());
    }

    if copy_via_osc52(user_prompt).is_ok() {
        return Ok(());
    }

    anyhow::bail!("failed to copy prompt")
}
