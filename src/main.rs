use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    time::Duration,
};

use mi::{
    assembler::Assembler,
    chat,
    cmd::{self, Cli, OpenRouterPreset},
    completions::{Answer, FinishReason},
    credits, image,
    printer::{self, Printer},
    skill::{format_skills, parse_skills},
    tool::{self, Tool},
    xdg::must_parse_config,
};

use clap::Parser;
use crossterm::{
    ExecutableCommand, QueueableCommand, cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use time::OffsetDateTime;
use time::macros::format_description;

fn cursor(session: &credits::Session) -> String {
    format!(
        "SESS: {} | REMN: {}\n> ",
        session.delta_string(),
        session.remaining_string()
    )
}

fn run_chat(
    preset: OpenRouterPreset,
    credits_gateway: Option<&str>,
    chat_gateway: &str,
) -> anyhow::Result<()> {
    let mut session = if let Some(credits_gateway) = credits_gateway {
        let credits = credits::get_credits(credits_gateway)?;
        credits::Session::from(&credits)
    } else {
        credits::Session::default()
    };

    let mut system_prompt = String::new();

    let skills = parse_skills()?;
    if !skills.is_empty() {
        if !system_prompt.is_empty() {
            system_prompt.push_str("\n\n");
        }
        system_prompt.push_str(&format_skills(&skills));
    }

    let read_tool = tool::Read;
    let write_tool = tool::WriteTool;
    let edit_tool = tool::EditTool;
    let bash_tool = tool::BashTool;
    let tools = Some(vec![
        read_tool.json(),
        write_tool.json(),
        edit_tool.json(),
        bash_tool.json(),
    ]);

    let mut tool_map: HashMap<String, Box<dyn Tool>> = HashMap::new();
    tool_map.insert(read_tool.name().to_owned(), Box::new(read_tool));
    tool_map.insert(write_tool.name().to_owned(), Box::new(write_tool));
    tool_map.insert(edit_tool.name().to_owned(), Box::new(edit_tool));
    tool_map.insert(bash_tool.name().to_owned(), Box::new(bash_tool));

    std::io::stdout().write_all(cursor(&session).as_bytes())?;
    std::io::stdout().flush()?;

    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    stdout.execute(event::PushKeyboardEnhancementFlags(
        event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
    ))?;
    stdout.execute(event::EnableBracketedPaste)?;

    let mut user_prompt = String::new();
    loop {
        let event = event::read()?;
        if let Event::Key(key_event) = event {
            if key_event.code == KeyCode::Char('c')
                && key_event.modifiers.contains(KeyModifiers::CONTROL)
            {
                return Ok(());
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
                        stdout.queue(cursor::MoveUp(1))?;
                        if total_lines <= 1 {
                            stdout.queue(cursor::MoveToColumn(last_line_len + 2))?;
                        } else {
                            stdout.queue(cursor::MoveToColumn(last_line_len))?;
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
    std::io::stdout().write_all("\n".as_bytes())?;
    std::io::stdout().flush()?;

    let (model, provider) = (preset.model(), preset.provider());

    let mut chat = chat::Chat::new(
        model,
        provider,
        Some("low".into()),
        tools,
        &system_prompt,
        &user_prompt,
    );

    let mut printer = Printer::default();

    loop {
        loop {
            printer.hide_cursor()?;
            let now = OffsetDateTime::now_utc();
            let format = format_description!("[year]-[month]-[day]-[hour]-[minute]-[second]");
            let formatted_time = now.format(&format)?;
            let filename = format!("/tmp/{}-sse.log", formatted_time);
            let raw_log_file = File::create(filename)?;
            let mut raw_log_writer = BufWriter::new(raw_log_file);

            let mut a = Assembler::default();

            let client = reqwest::blocking::ClientBuilder::default()
                .timeout(Duration::from_secs(600))
                .build()?;
            let res = client.post(chat_gateway).json(&chat).send()?;

            if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                if let Some(retry_after) = res.headers().get(reqwest::header::RETRY_AFTER) {
                    writeln!(stdout, "Retry-After: {:?}", retry_after)?;
                } else {
                    writeln!(stdout, "no header")?;
                }
            }
            let res = res.error_for_status()?;

            let reader = BufReader::new(res);

            for line in reader.lines() {
                let line = line?;

                writeln!(raw_log_writer, "{}", line)?;
                raw_log_writer.flush()?;

                let line = line.trim();

                if line.is_empty() {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        break;
                    }

                    match serde_json::from_str::<Answer>(data) {
                        Ok(v) => {
                            a.push_delta(&v);

                            let choice = &v.choices[0];

                            if let Some(ref reasoning) = choice.delta.reasoning {
                                if !reasoning.is_empty() {
                                    printer.print(printer::Mode::Reasoning, &reasoning)?;
                                }
                            }

                            if let Some(ref content) = choice.delta.content {
                                if !content.is_empty() {
                                    printer.print(printer::Mode::Content, &content)?;
                                }
                            }

                            if let Some(ref finish_reason) = choice.finish_reason {
                                match finish_reason {
                                    FinishReason::Stop => {
                                        printer.new_line()?;
                                        break;
                                    }
                                    FinishReason::ToolCalls => {
                                        printer.new_line()?;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to parse json: {}", e);
                        }
                    }
                }
            }

            let am = a.assemble();

            let content = match am.content {
                Some(content) => Some(content.clone()),
                None => None,
            };

            let mut tools: Vec<chat::Tool> = Vec::with_capacity(am.tool_calls.len());
            let mut chat_tool_calls: Vec<chat::ToolCall> = Vec::with_capacity(am.tool_calls.len());

            for tool_call in am.tool_calls.iter() {
                printer.reset()?;
                printer.tool(&tool_call.function, &tool_call.arguments)?;

                chat_tool_calls.push(chat::ToolCall::from(tool_call));

                if let Some(ref mut tool_impl) = tool_map.get_mut(&tool_call.function) {
                    let result = tool_impl
                        .call(&tool_call.arguments)
                        .unwrap_or("failed to call tool with this arg".into());
                    printer.print(printer::Mode::Content, &result)?;
                    printer.new_line()?;
                    tools.push(chat::Tool::new(tool_call.id.clone(), result));
                } else {
                    tools.push(chat::Tool::new(
                        tool_call.id.clone(),
                        "Wrong tool name! Tool not found!".into(),
                    ));
                }
            }

            let assistant_message = chat::AssistantMessage::new(content, chat_tool_calls);
            chat.messages
                .push(chat::Message::Assistant(assistant_message));

            if tools.is_empty() {
                break;
            }

            for tool in tools {
                chat.messages.push(chat::Message::Tool(tool));
            }
        }

        if let Some(credits_gateway) = credits_gateway {
            let credits = credits::get_credits(credits_gateway)?;
            session.update(&credits);
        }

        printer.show_cursor()?;
        printer.reset()?;

        std::io::stdout().write_all(cursor(&session).as_bytes())?;
        std::io::stdout().flush()?;

        let mut stdout = std::io::stdout();
        enable_raw_mode()?;
        stdout.execute(event::PushKeyboardEnhancementFlags(
            event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
        ))?;
        stdout.execute(event::EnableBracketedPaste)?;

        let mut user_prompt = String::new();
        loop {
            if let Event::Key(key_event) = event::read()? {
                if key_event.code == KeyCode::Char('c')
                    && key_event.modifiers.contains(KeyModifiers::CONTROL)
                {
                    return Ok(());
                }

                if key_event.code == KeyCode::Enter
                    && key_event.modifiers.contains(KeyModifiers::SHIFT)
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
                            let last_line_len =
                                user_prompt.lines().last().unwrap_or("").len() as u16;
                            stdout.queue(cursor::MoveUp(1))?;
                            if total_lines <= 1 {
                                stdout.queue(cursor::MoveToColumn(last_line_len + 2))?;
                            } else {
                                stdout.queue(cursor::MoveToColumn(last_line_len))?;
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
        }
        disable_raw_mode()?;
        stdout.execute(event::PopKeyboardEnhancementFlags)?;
        stdout.execute(event::EnableBracketedPaste)?;
        std::io::stdout().write_all("\n".as_bytes())?;
        std::io::stdout().flush()?;
        if user_prompt == "" {
            break;
        } else {
            let sm = chat::SimpleMessage::new(&user_prompt);
            chat.messages.push(chat::Message::User(sm));
        }
    }

    println!("Chat is over!");
    Ok(())
}

fn reset_terminal() {
    print!("\x1b[0m");
    print!("\x1b[?25h");
    print!("\x1b[<u");
    disable_raw_mode().unwrap();
    let mut stdout = std::io::stdout();
    stdout.execute(event::DisableBracketedPaste).unwrap();
    stdout.flush().unwrap();
}

fn main() -> anyhow::Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        reset_terminal();
        default_hook(panic_info);
    }));

    ctrlc::set_handler(move || {
        reset_terminal();
        println!();
        std::process::exit(130);
    })
    .expect("failed to set ctrlc");

    let cli = cmd::Cli::parse();
    match start(cli) {
        Ok(_) => {
            reset_terminal();
        }
        Err(e) => {
            reset_terminal();
            return Err(e);
        }
    }

    Ok(())
}

fn start(cli: Cli) -> anyhow::Result<()> {
    let config = must_parse_config();

    match cli.command {
        cmd::Command::Local => {
            let base_url = config.local.base_url;
            let chat_completions_url = format!("{base_url}/chat/completions");

            run_chat(OpenRouterPreset::None, None, &chat_completions_url)?;
        }
        cmd::Command::OpenRouter { preset } => {
            dbg!(&preset);
            let base_url = config.openrouter.base_url;
            let chat_completions_url = format!("{base_url}/chat/completions");
            let credits_url = format!("{base_url}/credits");

            if matches!(preset, OpenRouterPreset::None) {
                eprintln!("choose preset");
                return Ok(());
            }

            run_chat(preset, Some(&credits_url), &chat_completions_url)?
        }
        cmd::Command::ImageGen {
            model,
            prompt,
            resolution,
            aspect_ratio,
            input_reference_path,
        } => {
            let base_url = config.openrouter.base_url;
            let image_url = format!("{base_url}/images");
            image::generate_image(
                &image_url,
                model,
                prompt,
                resolution,
                aspect_ratio,
                &input_reference_path,
            )?;
        }
    }

    Ok(())
}
