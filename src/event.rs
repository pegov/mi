use std::io::{self, Write};

use crossterm::{
    ExecutableCommand, QueueableCommand,
    cursor::{MoveToColumn, MoveUp},
    event::{self, Event, KeyCode, KeyModifiers},
    queue,
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};

use crate::{copy::copy_user_prompt_to_clipboard, xdg::open_editor};

pub enum HandleEventsAction {
    None,
    Exit,
}

pub fn handle_events(
    stdout: &mut io::Stdout,
    user_prompt: &mut String,
) -> anyhow::Result<HandleEventsAction> {
    enable_raw_mode()?;
    stdout.execute(event::PushKeyboardEnhancementFlags(
        event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
    ))?;
    stdout.execute(event::EnableBracketedPaste)?;

    loop {
        let event = event::read()?;
        if let Event::Key(key_event) = event {
            if key_event.code == KeyCode::Char('c')
                && key_event.modifiers.contains(KeyModifiers::CONTROL)
            {
                return Ok(HandleEventsAction::Exit);
            }

            if key_event.code == KeyCode::Char('y')
                && key_event.modifiers.contains(KeyModifiers::CONTROL)
            {
                copy_user_prompt_to_clipboard(&user_prompt)?;
                continue;
            }

            if key_event.code == KeyCode::Char('e')
                && key_event.modifiers.contains(KeyModifiers::CONTROL)
            {
                let mut new_lines_count = 0;
                for char in user_prompt.chars() {
                    if char == '\n' {
                        new_lines_count += 1;
                    }
                }

                if new_lines_count > 0 {
                    queue!(stdout, MoveUp(new_lines_count))?;
                }
                queue!(stdout, MoveToColumn(2), Clear(ClearType::FromCursorDown))?;
                stdout.flush()?;

                *user_prompt = open_editor(&user_prompt)?;
                for char in user_prompt.trim().chars() {
                    if char == '\n' {
                        write!(stdout, "\r\n")?;
                    } else {
                        write!(stdout, "{char}")?;
                    }
                }
                stdout.flush()?;

                continue;
            }

            if key_event.code == KeyCode::Enter && key_event.modifiers.contains(KeyModifiers::SHIFT)
            {
                user_prompt.push('\n');
                write!(stdout, "\r\n")?;
                stdout.flush()?;
                continue;
            }

            if key_event.code == KeyCode::Enter {
                break;
            }

            if key_event.code == KeyCode::Backspace {
                if !user_prompt.is_empty() {
                    if user_prompt.ends_with('\n') {
                        user_prompt.pop();
                        let total_lines = user_prompt.lines().count();
                        let last_line_len = user_prompt.lines().last().unwrap_or("").len() as u16;
                        stdout.queue(MoveUp(1))?;
                        if total_lines <= 1 {
                            stdout.queue(MoveToColumn(last_line_len + 2))?;
                        } else {
                            stdout.queue(MoveToColumn(last_line_len))?;
                        }
                    } else {
                        user_prompt.pop();
                        write!(stdout, "\x08 \x08")?;
                    }
                    stdout.flush()?;
                }
                continue;
            }

            if key_event.modifiers.is_empty() {
                if let KeyCode::Char(c) = key_event.code {
                    user_prompt.push(c);
                    write!(stdout, "{}", c)?;
                    stdout.flush()?;
                }
                continue;
            }
            if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                if let KeyCode::Char(c) = key_event.code {
                    let up = c.to_uppercase().to_string();
                    user_prompt.push_str(&up);
                    write!(stdout, "{}", &up)?;
                    stdout.flush()?;
                }
                continue;
            }
        }

        if let Event::Paste(s) = event {
            for c in s.chars() {
                match c {
                    '\n' => {
                        user_prompt.push('\n');
                        write!(stdout, "\r\n")?;
                        stdout.flush()?;
                    }
                    _ => {
                        user_prompt.push(c);
                        write!(stdout, "{}", c)?;
                        stdout.flush()?;
                    }
                }
            }
            continue;
        }
    }

    disable_raw_mode()?;
    stdout.execute(event::PopKeyboardEnhancementFlags)?;
    stdout.execute(event::DisableBracketedPaste)?;
    stdout.write_all("\n".as_bytes())?;
    stdout.flush()?;

    return Ok(HandleEventsAction::None);
}
