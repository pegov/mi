use std::io;

use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags},
    terminal::{disable_raw_mode, enable_raw_mode},
};

use crate::{copy::copy_user_prompt_to_clipboard, image::parse_paste, tui::Tui, xdg::open_editor};

const MAX_IMAGE_PASTE_MIB: usize = 28;
const MAX_IMAGE_PASTE: usize = MAX_IMAGE_PASTE_MIB * 1024 * 1024;
const MAX_IMAGES: usize = 8;

pub enum HandleEventsAction {
    None,
    Exit,
}

pub fn handle_events(
    tui: &mut Tui,
    user_prompt: &mut String,
    history: &mut Vec<String>,
) -> anyhow::Result<HandleEventsAction> {
    tui.accepting_input = true;
    tui.command_selection = 0;
    tui.commands_dismissed = false;
    tui.prompt = user_prompt.clone();
    tui.draw()?;
    let mut history_index = history.len();
    let mut draft = String::new();

    loop {
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let code = match key.code {
                    KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        KeyCode::Up
                    }
                    KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        KeyCode::Down
                    }
                    code => code,
                };
                match (code, key.modifiers) {
                    (KeyCode::Char('c'), modifiers)
                        if modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        return Ok(HandleEventsAction::Exit);
                    }
                    (KeyCode::Char('o'), modifiers)
                        if modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        if let HandleEventsAction::Exit = image_menu(tui)? {
                            return Ok(HandleEventsAction::Exit);
                        }
                    }
                    (KeyCode::Char('y'), modifiers)
                        if modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        copy_user_prompt_to_clipboard(user_prompt)?;
                    }
                    (KeyCode::Char('e'), modifiers)
                        if modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        let terminal = tui.terminal.as_mut().unwrap();
                        terminal.show_cursor()?;
                        io::stdout().execute(event::PopKeyboardEnhancementFlags)?;
                        io::stdout().execute(event::DisableBracketedPaste)?;
                        disable_raw_mode()?;
                        let result = open_editor(user_prompt);
                        enable_raw_mode()?;
                        io::stdout().execute(event::PushKeyboardEnhancementFlags(
                            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
                        ))?;
                        io::stdout().execute(event::EnableBracketedPaste)?;
                        terminal.clear()?;
                        *user_prompt = result?;
                    }
                    (KeyCode::Enter, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
                        user_prompt.push('\n');
                    }
                    (KeyCode::Esc, _) => {
                        tui.commands_dismissed = true;
                    }
                    (KeyCode::Tab, _) => {
                        if let Some((name, _)) = tui.command_matches().get(tui.command_selection) {
                            *user_prompt = match user_prompt.split_once(char::is_whitespace) {
                                Some((command, _)) => format!("{command} {name} "),
                                None => format!("{name} "),
                            };
                        }
                    }
                    (KeyCode::Up | KeyCode::Down, _) => {
                        let count = tui.command_matches().len();
                        if count > 0 {
                            tui.command_selection = if code == KeyCode::Down {
                                (tui.command_selection + 1) % count
                            } else {
                                (tui.command_selection + count - 1) % count
                            };
                        } else {
                            match code {
                                KeyCode::Up if history_index > 0 => {
                                    if history_index == history.len() {
                                        draft = user_prompt.clone();
                                    }
                                    history_index -= 1;
                                }
                                KeyCode::Down if history_index < history.len() => {
                                    history_index += 1;
                                }
                                _ => continue,
                            }
                            *user_prompt = history.get(history_index).unwrap_or(&draft).clone();
                        }
                    }
                    (KeyCode::Enter, _) => {
                        if !user_prompt.is_empty() {
                            history.push(user_prompt.clone());
                        }
                        tui.accepting_input = false;
                        tui.prompt.clear();
                        tui.write(&format!("> {user_prompt}\n"))?;
                        return Ok(HandleEventsAction::None);
                    }
                    (KeyCode::Backspace, _) => {
                        user_prompt.pop();
                    }
                    (KeyCode::Char(c), modifiers)
                        if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
                    {
                        user_prompt.push(c);
                    }
                    _ => {}
                }
            }
            Event::Paste(text) => {
                user_prompt.push_str(&text.replace("\r\n", "\n").replace('\r', "\n"))
            }
            Event::Resize(_, _) => {}
            _ => continue,
        }
        if tui.prompt != *user_prompt {
            tui.command_selection = 0;
            tui.commands_dismissed = false;
        }
        tui.prompt = user_prompt.clone();
        tui.draw()?;
    }
}

fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn image_menu(tui: &mut Tui) -> anyhow::Result<HandleEventsAction> {
    let mut paste = None::<String>;
    let mut error = None::<String>;
    loop {
        tui.image_menu = Some(if let Some(text) = &paste {
            let message = error
                .as_deref()
                .unwrap_or("Paste a base64 image or data URL");
            format!(
                "{message}\n{} pasted | Enter: add | Esc: cancel",
                format_size(text.len())
            )
        } else {
            let list = tui
                .images
                .iter()
                .enumerate()
                .map(|(i, url)| {
                    let Some((mime, encoded)) = url
                        .strip_prefix("data:image/")
                        .and_then(|url| url.split_once(";base64,"))
                    else {
                        return format!("{}: image", i + 1);
                    };
                    let bytes = encoded.trim_end_matches('=').len() * 3 / 4;
                    let size = format_size(bytes);
                    format!("{}: {mime} ({size})", i + 1)
                })
                .collect::<Vec<_>>()
                .join(", ");
            let list = if list.is_empty() { "none" } else { &list };
            let add = if tui.images.len() < MAX_IMAGES {
                "n: new"
            } else {
                "queue full"
            };
            format!("Images: {list}\n{add} | Backspace: remove last | Esc: chat")
        });
        tui.draw()?;

        let key = match event::read()? {
            Event::Paste(text) if let Some(paste) = &mut paste => {
                if text.len() <= MAX_IMAGE_PASTE {
                    *paste = text;
                    error = None;
                } else {
                    paste.clear();
                    error = Some(format!("Paste exceeds {MAX_IMAGE_PASTE_MIB} MiB"));
                }
                continue;
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => key,
            _ => continue,
        };
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(HandleEventsAction::Exit);
            }
            (KeyCode::Esc, _) => {
                if paste.take().is_none() {
                    tui.image_menu = None;
                    return Ok(HandleEventsAction::None);
                }
                error = None;
            }
            (KeyCode::Char('n'), KeyModifiers::NONE)
                if paste.is_none() && tui.images.len() < MAX_IMAGES =>
            {
                paste = Some(String::new());
            }
            (KeyCode::Enter, _)
                if let Some(text) = &mut paste
                    && !text.is_empty() =>
            {
                match parse_paste(std::mem::take(text)) {
                    Ok(url)
                        if tui.images.iter().map(String::len).sum::<usize>() + url.len()
                            <= MAX_IMAGE_PASTE =>
                    {
                        tui.images.push(url);
                        paste = None;
                    }
                    Ok(_) => {
                        error = Some(format!(
                            "Images total exceeds {MAX_IMAGE_PASTE_MIB} MiB encoded, remove an image first"
                        ))
                    }
                    Err(err) => error = Some(err.to_string()),
                }
            }
            (KeyCode::Backspace, _) => {
                if let Some(text) = &mut paste {
                    text.pop();
                } else {
                    tui.images.pop();
                }
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT)
                if let Some(text) = &mut paste =>
            {
                if text.len() + c.len_utf8() <= MAX_IMAGE_PASTE {
                    text.push(c);
                    error = None;
                } else {
                    text.clear();
                    error = Some(format!("Paste exceeds {MAX_IMAGE_PASTE_MIB} MiB"));
                }
            }
            _ => {}
        }
    }
}
