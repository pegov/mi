use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

use mi::{
    assembler::Assembler,
    chan, chat,
    cmd::{self, Cli, OpenRouterModel, OpenRouterReasoning},
    compaction,
    completions::{Answer, FinishReason},
    credits,
    event::{HandleEventsAction, handle_events},
    image, jev, log,
    mcp::{echo_server::echo_mcp_server, http_server::mcp_http_server, sync::SyncMcpClient},
    printer::{self, Cursor},
    skill::{Skill, format_skills, manually_invoke_skill, parse_skills},
    tool::{self, Tool},
    tui::Tui,
    xdg::{CompactionConfig, get_root_agents_md, load_system_prompt_preset, parse_config},
};

use clap::Parser;
use crossterm::{ExecutableCommand, event::DisableBracketedPaste, terminal::disable_raw_mode};
use reqwest::Url;
use serde::Deserialize;

#[derive(Default)]
struct Tools {
    defs: Vec<serde_json::Value>,
    map: HashMap<String, Box<dyn Tool + Send>>,
}

impl Tools {
    fn push(&mut self, tool: Box<dyn Tool + Send>) {
        self.defs.push(tool.json());
        self.map.insert(tool.name().to_owned(), tool);
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Box<dyn Tool + Send>> {
        self.map.get_mut(name)
    }
}

macro_rules! make_tools {
    ($($tool:expr),+ $(,)?) => {{
        let mut tools = Tools::default();
        $(tools.push(Box::new($tool));)+
        tools
    }};
}

fn is_skill_invocation(prompt: &str) -> Option<(&str, &str)> {
    let prompt = prompt.trim();
    let rest = prompt
        .strip_prefix("/skill:")
        .or_else(|| prompt.strip_prefix('/'))?;
    let (name, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    if name.is_empty() {
        return None;
    }
    Some((name, args.trim_start()))
}

fn apply_skill(skills: &[Skill], user_prompt: &mut String, tui: &mut Tui) -> anyhow::Result<()> {
    if let Some((name, args)) = is_skill_invocation(&user_prompt.clone()) {
        if let Some(res) = manually_invoke_skill(skills, name, args) {
            tui.tool("skill", name)?;
            *user_prompt = res;
        } else {
            user_prompt.push('\n');
            user_prompt.push_str(&format!("Failed to find skill by name: {name}."));
        }
    }
    Ok(())
}

fn draw_banner(text: &str) -> String {
    let border = format!("+{}+\n", "-".repeat(text.len() + 2));
    format!("\n{border}| {text} |\n{border}\n")
}

const SYSTEM_PROMPT: &str = "You are an expert coding assistant operating inside mi, \
                            a simple and minimal coding agent harness.";

const SYSTEM_PROMPT_SKILLS_HOWTO: &str = "If the skill is not in the system prompt, \
                                          then it is not available for auto-discovery! \
                                          Do not try to find it! \
                                          If you don't have the skill, \
                                          do not make up the logic for this action \
                                          and immediately tell the user that \
                                          you don't have the skill and information about it.";

fn build_system_prompt(
    system_prompt_presets: &[String],
    disable_default_system_prompt: bool,
    ignore_root_agents_md: bool,
    skills: &[Skill],
) -> anyhow::Result<String> {
    let mut prompt_parts = Vec::new();
    if !disable_default_system_prompt {
        prompt_parts.push(SYSTEM_PROMPT.to_owned());
    }
    for name in system_prompt_presets {
        let spp = load_system_prompt_preset(name)?;
        prompt_parts.push(spp.trim_end_matches("\n").to_owned());
    }
    let mut system_prompt = prompt_parts.join("\n\n");

    let mut skills_to_prompt = Vec::with_capacity(skills.len());
    for skill in skills.iter() {
        if skill.disable_model_invocation {
            continue;
        }
        skills_to_prompt.push(skill.clone());
    }
    if !skills_to_prompt.is_empty() {
        if !system_prompt.is_empty() {
            system_prompt.push_str("\n\n");
        }
        system_prompt.push_str(&format_skills(&skills_to_prompt));

        system_prompt.push('\n');
        system_prompt.push_str(SYSTEM_PROMPT_SKILLS_HOWTO);
    }

    if !ignore_root_agents_md && let Some(agents_md) = get_root_agents_md()? {
        if !system_prompt.is_empty() {
            system_prompt.push_str("\n\n");
        }
        system_prompt.push_str(&format!(
            "Project-specific instructions and guidelines:\n\n\
             <project_instructions path=\"AGENTS.md\">\n{agents_md}\n</project_instructions>"
        ));
    }

    Ok(system_prompt)
}

#[derive(Default)]
struct ChatOptions {
    resume: bool,
    disable_default_system_prompt: bool,
    ignore_root_agents_md: bool,
    oneshot: Option<String>,
    models_url: Option<String>,
}

impl ChatOptions {
    fn with_resume(mut self, resume: bool) -> Self {
        self.resume = resume;
        self
    }

    fn with_disable_default_system_prompt(mut self, value: bool) -> Self {
        self.disable_default_system_prompt = value;
        self
    }

    fn with_ignore_root_agents_md(mut self, value: bool) -> Self {
        self.ignore_root_agents_md = value;
        self
    }

    fn with_oneshot(mut self, oneshot: Option<String>) -> Self {
        self.oneshot = oneshot;
        self
    }
}

fn compact_instructions(prompt: &str) -> Option<&str> {
    let prompt = prompt.trim();
    if prompt == "/compact" {
        return Some("");
    }
    prompt.strip_prefix("/compact ")
}

fn update_status(
    tui: &mut Tui,
    cursor: &Cursor,
    session: &mut credits::Session,
    chat: &chat::Chat,
    tool_tokens: u64,
    model_modalities: Option<&str>,
) {
    session.collect_summary_usage();
    session.active_context_tokens =
        session.estimate_context(chat.session.estimate_active_tokens() + tool_tokens);
    tui.status = format!(
        "{} | MODEL: {}{}{} | REASONING: {}",
        cursor.to_string(session).trim_end_matches("\n> "),
        chat.model(),
        chat.providers()["order"][0]
            .as_str()
            .map_or_else(String::new, |provider| format!(" [{provider}]")),
        model_modalities.map_or_else(String::new, |modalities| format!(" ({modalities})")),
        chat.reasoning(),
    );
    if let Some(service_tier) = chat.service_tier() {
        tui.status
            .push_str(&format!(" | SERVICE TIER: {service_tier}"));
    }
}

enum CompactionOutcome {
    Compacted { before: u64, after: u64 },
    Nothing,
    Cancelled,
    Failed(String),
}

#[allow(clippy::too_many_arguments)]
fn compact(
    chat: &mut chat::Chat,
    client: &reqwest::blocking::Client,
    url: &str,
    tui: &mut Tui,
    settings: &compaction::Settings,
    max_context: Option<u64>,
    output_limit: Option<u64>,
    instructions: &str,
    tool_tokens: u64,
    session: &mut credits::Session,
    mode: compaction::Mode,
    credits_gateway: Option<&str>,
) -> anyhow::Result<CompactionOutcome> {
    let result = (|| {
        let Some(preparation) =
            compaction::prepare(&chat.session, settings.keep_recent_tokens, mode)
        else {
            return Ok(CompactionOutcome::Nothing);
        };

        tui.accepting_input = false;
        tui.activity = Some("Compacting");
        tui.draw()?;

        let mut previous_summary = preparation.previous_summary.clone();
        let mut usage = None;
        let summary_requests = if settings.cache_aware {
            1
        } else {
            preparation.spans.len()
        };
        for index in 0..summary_requests {
            let mut request;
            let request_tokens;
            if settings.cache_aware {
                let summarized_messages = preparation.spans.iter().map(Vec::len).sum::<usize>()
                    + usize::from(preparation.previous_summary.is_some());
                let mut prompt = compaction::summary_prompt(None, Some(instructions), None);
                prompt.push_str(&format!(
                    "\nSummarize only the first {summarized_messages} messages after the system message \
                     in the conversation above, including any previous summary. \
                     Later messages will be retained verbatim; do not summarize them.\n"
                ));
                request_tokens = session
                    .estimate_context(chat.session.estimate_active_tokens() + tool_tokens)
                    + compaction::estimate_text_tokens(&prompt)
                    + 64;
                request = serde_json::to_value(&*chat)?;
                request["messages"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!({
                        "role": "user", "content": prompt,
                    }));
                request["stream"] = serde_json::json!(false);
            } else {
                let transcript =
                    compaction::serialize_conversation(&chat.session, &preparation.spans[index]);
                let prompt = compaction::summary_prompt(
                    previous_summary.as_deref(),
                    Some(instructions),
                    Some(&transcript),
                );
                request_tokens = compaction::estimate_text_tokens(&prompt) + 64;
                request = compaction::summary_request(
                    chat.model(),
                    chat.providers(),
                    chat.service_tier(),
                    &prompt,
                );
            }
            let max_tokens =
                match compaction::summary_budget(max_context, output_limit, request_tokens) {
                    Ok(value) => value,
                    Err(error) => return Ok(CompactionOutcome::Failed(error.to_string())),
                };
            request["max_tokens"] = serde_json::json!(max_tokens);

            let (tx, rx) = mpsc::channel();
            let client = client.clone();
            let url = url.to_owned();
            thread::spawn(move || {
                let _ = tx.send(compaction::request_summary(&client, &url, &request));
            });
            let result = match tui.recv(&rx) {
                Ok(Some(result)) => result,
                Ok(None) => return Ok(CompactionOutcome::Failed("worker disconnected".into())),
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::Interrupted) =>
                {
                    session.pending_summaries.push(rx);
                    return Ok(CompactionOutcome::Cancelled);
                }
                Err(error) => return Err(error),
            };
            if let Some(ref reported) = result.usage {
                session.save_usage(reported);
                mi::completions::Usage::accumulate(&mut usage, reported);
            }
            previous_summary = Some(match result.text {
                Ok(text) => text,
                Err(compaction::SummaryFailure::Overflow) => {
                    return Ok(CompactionOutcome::Failed(
                        "context overflow while generating the summary".into(),
                    ));
                }
                Err(compaction::SummaryFailure::Empty) => {
                    return Ok(CompactionOutcome::Failed(
                        "the model returned an empty summary".into(),
                    ));
                }
                Err(compaction::SummaryFailure::Error(message)) => {
                    return Ok(CompactionOutcome::Failed(message));
                }
            });
        }

        let Some(summary) = previous_summary else {
            return Ok(CompactionOutcome::Nothing);
        };
        let summary = compaction::append_file_lists(
            summary,
            &preparation.read_files,
            &preparation.modified_files,
        );

        let mut candidate = chat.session.clone();
        candidate.compactions.push(chat::Compaction {
            summary,
            first_kept_message_index: preparation.first_kept_message_index,
            message_count_at_compaction: chat.session.messages.len(),
            tokens_before: preparation.tokens_before,
            usage,
            read_files: preparation.read_files,
            modified_files: preparation.modified_files,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |value| value.as_secs()),
        });
        if let Err(error) = candidate.validate() {
            return Ok(CompactionOutcome::Failed(error.to_string()));
        }
        let after = candidate.estimate_active_tokens() + tool_tokens;
        let before = preparation.tokens_before + tool_tokens;
        if after >= before {
            return Ok(CompactionOutcome::Failed(
                "compaction did not reduce the context".into(),
            ));
        }

        if mode == compaction::Mode::Recovery
            && let Some(window) = max_context
            && session.estimate_context(after) > window
        {
            return Ok(CompactionOutcome::Failed(
                "retained input, system prompt, tools, or summary still exceeds the context window"
                    .into(),
            ));
        }

        let previous = std::mem::replace(&mut chat.session, candidate);
        if let Err(error) = chat.save_session() {
            chat.session = previous;
            return Ok(CompactionOutcome::Failed(format!(
                "could not persist the compaction: {error}"
            )));
        }
        Ok(CompactionOutcome::Compacted { before, after })
    })();
    session.collect_summary_usage();
    if session.pending_summaries.is_empty()
        && let Some(gateway) = credits_gateway
        && let Ok(Ok(credits)) = tui.run(|| credits::get_credits(gateway))
    {
        session.update(&credits);
    }
    result
}

fn report_compaction(tui: &mut Tui, outcome: &CompactionOutcome) -> anyhow::Result<()> {
    tui.activity = None;
    tui.finish()?;
    match outcome {
        CompactionOutcome::Compacted { before, after } => tui.write(&format!(
            "Compacted context: approximately {before} -> {after} tokens\n"
        )),
        CompactionOutcome::Nothing => tui.write("Nothing to compact\n"),
        CompactionOutcome::Cancelled => tui.write("Compaction cancelled\n"),
        CompactionOutcome::Failed(message) => tui.write(&format!("Compaction failed: {message}\n")),
    }
}

#[allow(clippy::too_many_arguments)]
fn auto_compact(
    chat: &mut chat::Chat,
    client: &reqwest::blocking::Client,
    url: &str,
    tui: &mut Tui,
    settings: &compaction::Settings,
    max_context: Option<u64>,
    output_limit: Option<u64>,
    tool_tokens: u64,
    session: &mut credits::Session,
    credits_gateway: Option<&str>,
) -> anyhow::Result<bool> {
    if !settings.enabled {
        return Ok(true);
    }
    let Some(max_context) = max_context else {
        return Ok(true);
    };
    let threshold = compaction::threshold(max_context, settings.compact_at_percent);
    let estimated = session.estimate_context(chat.session.estimate_active_tokens() + tool_tokens);
    if estimated <= threshold {
        return Ok(true);
    }
    let outcome = compact(
        chat,
        client,
        url,
        tui,
        settings,
        Some(max_context),
        output_limit,
        "",
        tool_tokens,
        session,
        compaction::Mode::Normal,
        credits_gateway,
    )?;
    report_compaction(tui, &outcome)?;
    if matches!(
        outcome,
        CompactionOutcome::Cancelled | CompactionOutcome::Failed(_)
    ) {
        return Ok(false);
    }
    let after = session.estimate_context(chat.session.estimate_active_tokens() + tool_tokens);
    if after > max_context {
        tui.write("Retained input, system prompt, tools, or summary still exceeds the context budget; reduce the input or retention settings before retrying\n")?;
        return Ok(false);
    }
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn run_chat(
    model: OpenRouterModel,
    reasoning_effort: OpenRouterReasoning,
    mut max_context: Option<u64>,
    mut output_limit: Option<u64>,
    compaction_config: Option<CompactionConfig>,
    credits_gateway: Option<&str>,
    chat_gateway: &str,
    system_prompt_presets: &[String],
    opts: ChatOptions,
) -> anyhow::Result<()> {
    let (resume, disable_default_system_prompt, ignore_root_agents_md, oneshot) = (
        opts.resume,
        opts.disable_default_system_prompt,
        opts.ignore_root_agents_md,
        opts.oneshot,
    );

    let mut session = if let Some(credits_gateway) = credits_gateway {
        let credits = credits::get_credits(credits_gateway)?;
        credits::Session::from(&credits)
    } else {
        credits::Session::default()
    };

    let mut settings = compaction::Settings::resolve(compaction_config.as_ref(), model.model())?;
    let mut cursor = Cursor::new(max_context);

    let skills = parse_skills()?;
    let system_prompt = build_system_prompt(
        system_prompt_presets,
        disable_default_system_prompt,
        ignore_root_agents_md,
        &skills,
    )?;

    let mut tool_registry = make_tools![
        tool::Read,
        tool::Tail,
        tool::WriteTool,
        tool::EditTool,
        tool::BashTool::new(300, 300),
    ];
    let tool_tokens = compaction::estimate_tools_tokens(&tool_registry.defs);

    let mut tui = Tui::new(
        cursor
            .to_string(&session)
            .trim_end_matches("\n> ")
            .to_owned(),
    )?;

    tui.commands.extend(skills.iter().map(|skill| {
        let mut description = skill
            .description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if let Some((end, _)) = description.char_indices().nth(100) {
            description.truncate(end);
            description.push_str("...");
        }
        (format!("/skill:{}", skill.name), description)
    }));

    let client = reqwest::blocking::ClientBuilder::default()
        .timeout(Duration::from_secs(600))
        .build()?;

    tui.write(&draw_banner("NEW SESSION STARTED"))?;
    let mut providers = model.provider();
    let mut model_modalities = None;
    if let Some(url) = &opts.models_url {
        let (info, selected_providers) = get_model_info(&client, url, &model, &mut tui)?;
        providers = selected_providers;
        max_context = Some(info.limits.context_length);
        output_limit = info.limits.output.max_completion_tokens;
        cursor = Cursor::new(max_context);
        model_modalities = Some(info.architecture.to_status());
    }
    if max_context.is_none() && settings.enabled {
        tui.write("Context capacity is unknown; automatic threshold compaction is disabled. Set local.context_window to enable it.\n")?;
    }

    let mut prompt_history = Vec::new();

    let mut chat = chat::Chat::new(
        model.model(),
        providers,
        model.service_tier(),
        reasoning_effort,
        tool_registry.defs.clone(),
        &system_prompt,
    );
    if resume {
        chat.resume()?;
    }
    update_status(
        &mut tui,
        &cursor,
        &mut session,
        &chat,
        tool_tokens,
        model_modalities.as_deref(),
    );

    loop {
        let mut user_prompt = if let Some(ref prompt) = oneshot {
            tui.write(&format!("> {prompt}\n"))?;
            prompt.to_owned()
        } else {
            let mut prompt = String::new();
            match handle_events(&mut tui, &mut prompt, &mut prompt_history)? {
                HandleEventsAction::None => {}
                HandleEventsAction::Exit => return Ok(()),
            }
            prompt
        };

        update_status(
            &mut tui,
            &cursor,
            &mut session,
            &chat,
            tool_tokens,
            model_modalities.as_deref(),
        );
        let prompt = user_prompt.trim();
        let (word, arg) = prompt
            .split_once(char::is_whitespace)
            .unwrap_or((prompt, ""));
        let arg = arg.trim();
        if matches!(word, "/model" | "/m") {
            match arg.parse::<OpenRouterModel>() {
                Ok(new_model) => {
                    let result = (|| -> anyhow::Result<()> {
                        let new_settings = compaction::Settings::resolve(
                            compaction_config.as_ref(),
                            new_model.model(),
                        )?;
                        let providers = if let Some(url) = &opts.models_url {
                            let (info, providers) =
                                get_model_info(&client, url, &new_model, &mut tui)?;
                            max_context = Some(info.limits.context_length);
                            output_limit = info.limits.output.max_completion_tokens;
                            cursor = Cursor::new(max_context);
                            model_modalities = Some(info.architecture.to_status());
                            providers
                        } else {
                            new_model.provider()
                        };
                        chat.set_model(&new_model, providers);
                        settings = new_settings;
                        Ok(())
                    })();
                    match result {
                        Ok(()) => {
                            update_status(
                                &mut tui,
                                &cursor,
                                &mut session,
                                &chat,
                                tool_tokens,
                                model_modalities.as_deref(),
                            );
                            tui.write(&format!(
                                "model: {} ({})\n",
                                new_model.as_str(),
                                new_model.model(),
                            ))?;
                        }
                        Err(error) => tui.write(&format!("failed to change model: {error}\n"))?,
                    }
                }
                Err(_) => tui.write(&format!("unknown model: {arg}\n"))?,
            }
            if oneshot.is_some() {
                break;
            }
            continue;
        }
        if matches!(word, "/reasoning" | "/r") {
            match arg.parse::<OpenRouterReasoning>() {
                Ok(reasoning) => {
                    chat.set_reasoning(reasoning);
                    update_status(
                        &mut tui,
                        &cursor,
                        &mut session,
                        &chat,
                        tool_tokens,
                        model_modalities.as_deref(),
                    );
                    tui.write(&format!("reasoning: {}\n", chat.reasoning()))?;
                }
                Err(_) => tui.write(&format!("unknown reasoning level: {arg}\n"))?,
            }
            if oneshot.is_some() {
                break;
            }
            continue;
        }
        if user_prompt.trim() == "/new" {
            chat.session = chat::Session::new_system(&system_prompt);
            chat.save_session()?;
            session = credits::Session {
                start_balance: session.curr_balance,
                curr_balance: session.curr_balance,
                context_ratio: session.context_ratio,
                pending_summaries: std::mem::take(&mut session.pending_summaries),
                ..Default::default()
            };
            update_status(
                &mut tui,
                &cursor,
                &mut session,
                &chat,
                tool_tokens,
                model_modalities.as_deref(),
            );
            tui.write(&draw_banner("NEW SESSION STARTED"))?;
            if oneshot.is_some() {
                break;
            }
            continue;
        }
        if user_prompt.is_empty() {
            break;
        }
        if let Some(instructions) = compact_instructions(&user_prompt) {
            tui.accepting_input = false;
            let outcome = compact(
                &mut chat,
                &client,
                chat_gateway,
                &mut tui,
                &settings,
                max_context,
                output_limit,
                instructions,
                tool_tokens,
                &mut session,
                compaction::Mode::Normal,
                credits_gateway,
            )?;
            report_compaction(&mut tui, &outcome)?;
            update_status(
                &mut tui,
                &cursor,
                &mut session,
                &chat,
                tool_tokens,
                model_modalities.as_deref(),
            );
            if oneshot.is_some() {
                break;
            }
            continue;
        }
        apply_skill(&skills, &mut user_prompt, &mut tui)?;
        chat.session
            .messages
            .push(chat::Message::User(chat::SimpleMessage::new(&user_prompt)));

        let generation = (|| -> anyhow::Result<()> {
            let mut recovery_used = false;
            loop {
                if !auto_compact(
                    &mut chat,
                    &client,
                    chat_gateway,
                    &mut tui,
                    &settings,
                    max_context,
                    output_limit,
                    tool_tokens,
                    &mut session,
                    credits_gateway,
                )? {
                    break;
                }
                update_status(
                    &mut tui,
                    &cursor,
                    &mut session,
                    &chat,
                    tool_tokens,
                    model_modalities.as_deref(),
                );
                tui.accepting_input = false;
                tui.activity = Some("Thinking");
                tui.draw()?;
                let mut raw_log_writer = log::create_sse_log()?;

                let mut a = Assembler::default();

                let request = client.post(chat_gateway).json(&chat).build()?;
                let thread_client = client.clone();
                let (tx, rx) = mpsc::channel();
                thread::spawn(move || {
                    let _ = tx.send(thread_client.execute(request));
                });
                let res = tui
                    .recv(&rx)?
                    .ok_or_else(|| anyhow::anyhow!("worker disconnected"))??;

                if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    if let Some(retry_after) = res.headers().get(reqwest::header::RETRY_AFTER) {
                        tui.write(&format!("Retry-After: {retry_after:?}\n"))?;
                    } else {
                        tui.write("no header\n")?;
                    }
                }
                if !res.status().is_success() {
                    let status = res.status();
                    let body = res.text().unwrap_or_default();
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body)
                        && let Ok(usage) =
                            serde_json::from_value::<mi::completions::Usage>(value["usage"].clone())
                    {
                        session.save_usage(&usage);
                    }
                    if !recovery_used && compaction::is_context_overflow(status, &body) {
                        let outcome = compact(
                            &mut chat,
                            &client,
                            chat_gateway,
                            &mut tui,
                            &settings,
                            max_context,
                            output_limit,
                            "",
                            tool_tokens,
                            &mut session,
                            compaction::Mode::Recovery,
                            credits_gateway,
                        )?;
                        report_compaction(&mut tui, &outcome)?;
                        if matches!(outcome, CompactionOutcome::Compacted { .. }) {
                            recovery_used = true;
                            update_status(
                                &mut tui,
                                &cursor,
                                &mut session,
                                &chat,
                                tool_tokens,
                                model_modalities.as_deref(),
                            );
                            continue;
                        }
                        return Ok(());
                    }
                    anyhow::bail!("API {status} error: {body}");
                }

                let (tx, rx) = mpsc::channel();
                thread::spawn(move || {
                    for line in BufReader::new(res).lines() {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                });

                let mut finish_reason = None;
                let mut stream_error = None;
                while let Some(line) = tui.recv(&rx)? {
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

                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Ok(usage) = serde_json::from_value::<mi::completions::Usage>(
                                value["usage"].clone(),
                            ) {
                                session.save_usage(&usage);
                                let estimated = chat.session.estimate_active_tokens() + tool_tokens;
                                if estimated > 0 && usage.prompt_tokens > 0 {
                                    session.context_ratio =
                                        (usage.prompt_tokens as f64 / estimated as f64).max(1.0);
                                }
                            }
                            if !value["error"].is_null() {
                                stream_error = Some(value["error"].to_string());
                                break;
                            }
                        }

                        match serde_json::from_str::<Answer>(data) {
                            Ok(v) => {
                                update_status(
                                    &mut tui,
                                    &cursor,
                                    &mut session,
                                    &chat,
                                    tool_tokens,
                                    model_modalities.as_deref(),
                                );
                                let Some(choice) = v.choices.first() else {
                                    tui.draw()?;
                                    continue;
                                };

                                a.push_delta(&v);

                                if let Some(ref reasoning) = choice.delta.reasoning
                                    && !reasoning.is_empty()
                                {
                                    tui.activity = Some("Thinking");
                                    tui.print(printer::Mode::Reasoning, reasoning)?;
                                }

                                if let Some(ref content) = choice.delta.content
                                    && !content.is_empty()
                                {
                                    tui.activity = Some("Generating");
                                    tui.print(printer::Mode::Content, content)?;
                                }

                                if choice
                                    .delta
                                    .tool_calls
                                    .as_ref()
                                    .is_some_and(|calls| !calls.is_empty())
                                {
                                    tui.activity = Some("Tool-calling");
                                }

                                if choice.finish_reason.is_some() {
                                    finish_reason = choice.finish_reason.clone();
                                }

                                if let Some(
                                    FinishReason::Stop
                                    | FinishReason::ToolCalls
                                    | FinishReason::Length,
                                ) = choice.finish_reason
                                {
                                    tui.finish()?;
                                } else {
                                    tui.draw()?;
                                }
                            }
                            Err(e) => {
                                tui.finish()?;
                                tui.write(&format!("Failed to parse json: {e}\n"))?;
                            }
                        }
                    }
                }

                if let Some(credits_gateway) = credits_gateway {
                    let credits = tui.run(|| credits::get_credits(credits_gateway))??;
                    session.update(&credits);
                }

                update_status(
                    &mut tui,
                    &cursor,
                    &mut session,
                    &chat,
                    tool_tokens,
                    model_modalities.as_deref(),
                );
                tui.finish()?;

                if let Some(message) = stream_error {
                    let am = a.assemble();
                    if am.content.as_deref().is_some_and(|c| !c.is_empty())
                        || !am.tool_calls.is_empty()
                    {
                        let start = chat.session.messages.len();
                        let tool_calls = am
                            .tool_calls
                            .iter()
                            .map(chat::ToolCall::from)
                            .collect::<Vec<_>>();
                        let failed = am
                            .tool_calls
                            .iter()
                            .map(|call| {
                                chat::Tool::new(
                                    call.id.clone(),
                                    "Tool call abandoned: the response failed.".into(),
                                )
                            })
                            .collect::<Vec<_>>();
                        chat.session.messages.push(chat::Message::Assistant(
                            chat::AssistantMessage::new(am.content, tool_calls),
                        ));
                        chat.session
                            .messages
                            .extend(failed.into_iter().map(chat::Message::Tool));
                        chat.session
                            .omitted
                            .push((start, chat.session.messages.len()));
                        chat.save_session()?;
                    }
                    update_status(
                        &mut tui,
                        &cursor,
                        &mut session,
                        &chat,
                        tool_tokens,
                        model_modalities.as_deref(),
                    );
                    let overflow = compaction::is_overflow_message(&message.to_lowercase());
                    if overflow && !recovery_used {
                        let outcome = compact(
                            &mut chat,
                            &client,
                            chat_gateway,
                            &mut tui,
                            &settings,
                            max_context,
                            output_limit,
                            "",
                            tool_tokens,
                            &mut session,
                            compaction::Mode::Recovery,
                            credits_gateway,
                        )?;
                        report_compaction(&mut tui, &outcome)?;
                        if matches!(outcome, CompactionOutcome::Compacted { .. }) {
                            recovery_used = true;
                            update_status(
                                &mut tui,
                                &cursor,
                                &mut session,
                                &chat,
                                tool_tokens,
                                model_modalities.as_deref(),
                            );
                            continue;
                        }
                        return Ok(());
                    }
                    anyhow::bail!("API error: {message}");
                }

                let am = a.assemble();

                let content = am.content.clone();

                let mut tools: Vec<chat::Tool> = Vec::with_capacity(am.tool_calls.len());
                let mut chat_tool_calls: Vec<chat::ToolCall> =
                    Vec::with_capacity(am.tool_calls.len());

                let truncated = matches!(finish_reason, Some(FinishReason::Length));

                for tool_call in am.tool_calls.iter() {
                    tui.activity = Some("Tool-calling");
                    tui.tool(&tool_call.function, &tool_call.arguments)?;

                    chat_tool_calls.push(chat::ToolCall::from(tool_call));

                    if truncated {
                        tools.push(chat::Tool::new(
                            tool_call.id.clone(),
                            "Tool call abandoned: the response was truncated.".into(),
                        ));
                    } else if let Some(tool_impl) = tool_registry.get_mut(&tool_call.function) {
                        let result = tui
                            .run(|| tool_impl.call(&tool_call.arguments))?
                            .unwrap_or_else(|e| format!("failed to call tool: {e}"));
                        tui.print(printer::Mode::Content, &result)?;
                        tui.finish()?;
                        tools.push(chat::Tool::new(tool_call.id.clone(), result));
                    } else {
                        tools.push(chat::Tool::new(
                            tool_call.id.clone(),
                            "Wrong tool name! Tool not found!".into(),
                        ));
                    }
                }

                let attempt_start = chat.session.messages.len();
                let assistant_message = chat::AssistantMessage::new(content, chat_tool_calls);
                chat.session
                    .messages
                    .push(chat::Message::Assistant(assistant_message));

                let has_tools = !tools.is_empty();
                for tool in tools {
                    chat.session.messages.push(chat::Message::Tool(tool));
                }
                if truncated {
                    chat.session
                        .omitted
                        .push((attempt_start, chat.session.messages.len()));
                }
                chat.save_session()?;
                update_status(
                    &mut tui,
                    &cursor,
                    &mut session,
                    &chat,
                    tool_tokens,
                    model_modalities.as_deref(),
                );

                if truncated {
                    if recovery_used {
                        tui.write(
                            "Response was truncated and compaction already made one attempt\n",
                        )?;
                        break;
                    }
                    let outcome = compact(
                        &mut chat,
                        &client,
                        chat_gateway,
                        &mut tui,
                        &settings,
                        max_context,
                        output_limit,
                        "",
                        tool_tokens,
                        &mut session,
                        compaction::Mode::Recovery,
                        credits_gateway,
                    )?;
                    report_compaction(&mut tui, &outcome)?;
                    if matches!(outcome, CompactionOutcome::Compacted { .. }) {
                        recovery_used = true;
                        update_status(
                            &mut tui,
                            &cursor,
                            &mut session,
                            &chat,
                            tool_tokens,
                            model_modalities.as_deref(),
                        );
                        continue;
                    }
                    chat.save_session()?;
                    break;
                }

                if !has_tools {
                    break;
                }
            }

            Ok(())
        })();
        update_status(
            &mut tui,
            &cursor,
            &mut session,
            &chat,
            tool_tokens,
            model_modalities.as_deref(),
        );
        if let Err(error) = generation {
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::Interrupted)
            {
                tui.activity = None;
                tui.finish()?;
                tui.write("Interrupted\n")?;
                chat.save_session()?;
            } else {
                return Err(error);
            }
        }
        tui.activity = None;
        if oneshot.is_some() {
            break;
        }
    }

    tui.write("Chat is over!\n")?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    id: String,
    architecture: ModelArchitecture,
    #[serde(flatten)]
    limits: ModelLimits,
}

#[derive(Debug, Deserialize)]
struct ModelArchitecture {
    input_modalities: Vec<String>,
    output_modalities: Vec<String>,
}

impl ModelArchitecture {
    fn to_status(&self) -> String {
        let mut first_part = Vec::new();
        let mut second_part = Vec::new();
        for value in self.input_modalities.iter() {
            match value.as_str() {
                "text" => first_part.push("T"),
                "image" => first_part.push("I"),
                _ => {}
            }
        }

        for value in self.output_modalities.iter() {
            match value.as_str() {
                "text" => second_part.push("T"),
                "image" => second_part.push("I"),
                _ => {}
            }
        }

        format!("{} -> {}", first_part.join(""), second_part.join(""))
    }
}

#[derive(Debug, Deserialize)]
struct ModelLimits {
    context_length: u64,
    #[serde(alias = "top_provider")]
    output: ModelTopProvider,
}

#[derive(Debug, Deserialize)]
struct ModelTopProvider {
    #[serde(alias = "max_output_tokens")]
    max_completion_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct EndpointsResponse {
    data: ModelEndpoints,
}

#[derive(Deserialize)]
struct ModelEndpoints {
    endpoints: Vec<Endpoint>,
}

#[derive(Deserialize)]
struct Endpoint {
    tag: Option<String>,
}

fn get_model_info(
    client: &reqwest::blocking::Client,
    url: &str,
    model: &OpenRouterModel,
    tui: &mut Tui,
) -> anyhow::Result<(ModelInfo, serde_json::Value)> {
    let model_id = model.model();
    let (info, endpoints) = tui.run(|| -> anyhow::Result<(ModelInfo, Vec<Endpoint>)> {
        let query_url = Url::parse_with_params(url, &[("q", model_id)])?;
        let response = client.get(query_url).send()?;
        let status = response.status();
        let body_text = response.text()?;
        if !status.is_success() {
            anyhow::bail!("API {status} error: {body_text}");
        }
        let body: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| anyhow::anyhow!("invalid JSON: {e} — body: {body_text}"))?;
        let models = body["data"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing data array in response"))?;
        let model_value = models
            .iter()
            .find(|value| value["id"] == model_id)
            .ok_or_else(|| anyhow::anyhow!("model '{model_id}' not found"))?;
        let info: ModelInfo = serde_json::from_value(model_value.clone())?;

        let mut endpoints_url = Url::parse(url)?;
        {
            let mut segments = endpoints_url
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("invalid models URL"))?;
            segments.pop_if_empty();
            for part in info.id.split('/') {
                segments.push(part);
            }
            segments.push("endpoints");
        }
        let response = client.get(endpoints_url).send()?;
        let status = response.status();
        let body_text = response.text()?;
        if !status.is_success() {
            anyhow::bail!("API {status} error: {body_text}");
        }
        let body: EndpointsResponse = serde_json::from_str(&body_text)
            .map_err(|e| anyhow::anyhow!("invalid endpoints response: {e} — body: {body_text}"))?;
        Ok((info, body.data.endpoints))
    })??;

    for &provider in model.provider_order() {
        if endpoints
            .iter()
            .any(|endpoint| endpoint.tag.as_deref() == Some(provider))
        {
            let mut providers = model.provider();
            providers["order"] = serde_json::json!([provider]);
            return Ok((info, providers));
        }
        tui.write(&format!(
            "Provider '{provider}' is currently unavailable.\n"
        ))?;
    }

    anyhow::bail!("no available provider for model '{model_id}'");
}

fn reset_terminal() {
    print!("\x1b[0m");
    print!("\x1b[?25h");
    print!("\x1b[<u");
    disable_raw_mode().unwrap();
    let mut stdout = std::io::stdout();
    stdout.execute(DisableBracketedPaste).unwrap();
    stdout.flush().unwrap();
}

fn main() -> anyhow::Result<()> {
    let cli = cmd::Cli::parse();
    match cli.command {
        cmd::Command::McpServer => {}
        _ => {
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
        }
    }

    match cli.command {
        cmd::Command::McpServer => start(cli),
        _ => match start(cli) {
            Ok(_) => {
                reset_terminal();
                Ok(())
            }
            Err(e) => {
                reset_terminal();
                Err(e)
            }
        },
    }
}

fn start(cli: Cli) -> anyhow::Result<()> {
    let mut config = parse_config()?;
    if let Some(cache_aware) = cli.cache_aware_compaction {
        config
            .compaction
            .get_or_insert_with(CompactionConfig::default)
            .cache_aware = Some(cache_aware);
    }
    if let Some(percent) = cli.compaction_at_percent {
        let compaction = config
            .compaction
            .get_or_insert_with(CompactionConfig::default);
        compaction.compact_at_percent = Some(percent);
        if let Some(overrides) = &mut compaction.model_overrides {
            for override_ in overrides.values_mut() {
                override_.compact_at_percent = Some(percent);
            }
        }
    }

    match cli.command {
        cmd::Command::Local {
            resume,
            oneshot,
            profile,
            system_prompt_preset,
            disable_default_system_prompt,
            ignore_root_agents_md,
        } => {
            let profile = config.load_profile(profile.as_deref(), system_prompt_preset)?;
            let base_url = config.local.base_url;
            let chat_completions_url = format!("{base_url}/chat/completions");

            let opts = ChatOptions::default()
                .with_resume(resume)
                .with_disable_default_system_prompt(disable_default_system_prompt)
                .with_ignore_root_agents_md(ignore_root_agents_md)
                .with_oneshot(oneshot);

            run_chat(
                OpenRouterModel::None,
                OpenRouterReasoning::None,
                config.local.context_window,
                None,
                config.compaction,
                None,
                &chat_completions_url,
                &profile.spp,
                opts,
            )?;
        }
        cmd::Command::OpenRouter {
            model,
            reasoning,
            resume,
            oneshot,
            profile,
            system_prompt_preset,
            disable_default_system_prompt,
            ignore_root_agents_md,
        } => {
            let profile = config.load_profile(profile.as_deref(), system_prompt_preset)?;
            let model = model.or(profile.model).unwrap_or(OpenRouterModel::DeepSeek);
            let reasoning = reasoning
                .or(profile.reasoning)
                .unwrap_or(OpenRouterReasoning::Low);
            println!(
                "OpenRouter | model: {} ({}) | reasoning: {} | service_tier: {}",
                model.as_str(),
                model.model(),
                reasoning.as_str(),
                model.service_tier().unwrap_or("default"),
            );
            let base_url = config.openrouter.base_url;
            let chat_completions_url = format!("{base_url}/chat/completions");
            let credits_url = format!("{base_url}/credits");

            if matches!(model, OpenRouterModel::None) {
                eprintln!("choose model");
                return Ok(());
            }

            let opts = ChatOptions {
                models_url: Some(format!("{base_url}/models")),
                ..ChatOptions::default()
            }
            .with_resume(resume)
            .with_disable_default_system_prompt(disable_default_system_prompt)
            .with_ignore_root_agents_md(ignore_root_agents_md)
            .with_oneshot(oneshot);

            run_chat(
                model,
                reasoning,
                None,
                None,
                config.compaction,
                Some(&credits_url),
                &chat_completions_url,
                &profile.spp,
                opts,
            )?
        }
        cmd::Command::ImageGen {
            model,
            prompt,
            resolution,
            aspect_ratio,
            output_dir,
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
                &output_dir,
                &input_reference_path,
            )?;
        }
        cmd::Command::Chan => chan::chan()?,
        cmd::Command::Jev => {
            let url = config.jev.url;
            jev::jev(&url)?;
        }
        cmd::Command::McpServer => {
            echo_mcp_server()?;
        }
        cmd::Command::McpHttpServer => {
            mcp_http_server("127.0.0.1:8090", "/mcp")?;
        }
        cmd::Command::McpClient => {
            let client = SyncMcpClient::connect("tmp/mcp-server.sh")?;

            let remote = Arc::new(SyncMcpClient::connect_http("http://localhost:8090/mcp")?);

            for client in [&client, &remote] {
                let tools = client.list_tools()?;
                println!(
                    "tools: {:?}",
                    tools.iter().map(|t| &t.name).collect::<Vec<_>>()
                );
            }
            let r = remote.call_tool("echo", serde_json::json!({ "text": "hello http" }))?;
            println!("{r:?}");
            let r = remote.call_tool("stream_countdown", serde_json::json!({ "count": 3 }))?;
            println!("{r:?}");

            let c = Arc::clone(&remote);
            let call = thread::spawn(move || {
                c.call_tool("stream_countdown", serde_json::json!({ "count": 5 }))
            });

            loop {
                while let Some(p) = remote.try_recv_progress() {
                    println!(
                        "progress {}/{}: {}",
                        p.progress,
                        p.total.unwrap_or(0.0),
                        p.message.as_deref().unwrap_or("")
                    );
                }

                if call.is_finished() {
                    // drain (?)
                    while let Some(p) = remote.try_recv_progress() {
                        println!(
                            "progress {}/{}: {}",
                            p.progress,
                            p.total.unwrap_or(0.0),
                            p.message.as_deref().unwrap_or("")
                        );
                    }
                    break;
                }

                thread::sleep(Duration::from_millis(20));
            }

            let result = call.join().expect("worker panicked")?;
            for item in &result.content {
                if let Some(text) = item.as_text() {
                    println!("final: {}", text.text);
                }
            }
        }
    }

    Ok(())
}
