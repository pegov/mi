use std::io;

use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags},
    terminal::{disable_raw_mode, enable_raw_mode},
};

use crate::{copy::copy_user_prompt_to_clipboard, tui::Tui, xdg::open_editor};

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
                    (KeyCode::Char('y'), modifiers)
                        if modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        copy_user_prompt_to_clipboard(user_prompt)?;
                    }
                    (KeyCode::Char('e'), modifiers)
                        if modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        tui.terminal.show_cursor()?;
                        io::stdout().execute(event::PopKeyboardEnhancementFlags)?;
                        io::stdout().execute(event::DisableBracketedPaste)?;
                        disable_raw_mode()?;
                        let result = open_editor(user_prompt);
                        enable_raw_mode()?;
                        io::stdout().execute(event::PushKeyboardEnhancementFlags(
                            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
                        ))?;
                        io::stdout().execute(event::EnableBracketedPaste)?;
                        tui.terminal.clear()?;
                        *user_prompt = result?;
                    }
                    (KeyCode::Enter, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
                        user_prompt.push('\n');
                    }
                    (KeyCode::Esc, _) => {
                        tui.commands_dismissed = true;
                    }
                    (KeyCode::Up | KeyCode::Down, _) if !tui.command_matches().is_empty() => {
                        let count = tui.command_matches().len();
                        tui.command_selection = if code == KeyCode::Down {
                            (tui.command_selection + 1) % count
                        } else {
                            (tui.command_selection + count - 1) % count
                        };
                    }
                    (KeyCode::Tab, _) if !tui.command_matches().is_empty() => {
                        if let Some((name, _)) = tui.command_matches().get(tui.command_selection) {
                            *user_prompt = format!("{name} ");
                        }
                    }
                    (KeyCode::Up | KeyCode::Down, _) => {
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
