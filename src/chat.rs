use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, fs};

use crate::{
    compaction,
    responses::{self, InputItem, Item, ProviderPreferences, Role, Status},
    xdg::get_session_path,
};

const SESSION_VERSION: u32 = 1;

pub struct Chat {
    model: String,
    providers: Option<ProviderPreferences>,
    service_tier: Option<&'static str>,
    reasoning_effort: &'static str,
    tools: Vec<Value>,
    pub session: Session,
}

impl Chat {
    pub fn new(
        model: &str,
        providers: Option<ProviderPreferences>,
        service_tier: Option<&'static str>,
        reasoning_effort: &'static str,
        tools: Vec<Value>,
        system: &str,
    ) -> Self {
        Self {
            model: model.into(),
            providers,
            service_tier,
            reasoning_effort,
            tools,
            session: Session::new_system(system),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn set_model(
        &mut self,
        model: &crate::cmd::OpenRouterModel,
        providers: Option<ProviderPreferences>,
    ) {
        self.model = model.model().into();
        self.providers = providers;
        self.service_tier = model.service_tier();
    }

    pub fn reasoning(&self) -> &str {
        self.reasoning_effort
    }

    pub fn set_reasoning(&mut self, effort: &'static str) {
        self.reasoning_effort = effort;
    }

    pub fn providers(&self) -> Option<&ProviderPreferences> {
        self.providers.as_ref()
    }

    pub fn service_tier(&self) -> Option<&'static str> {
        self.service_tier
    }

    pub fn input(&self, messages: &[Message]) -> Vec<InputItem> {
        let mut input = Vec::new();
        for message in messages {
            match message {
                Message::System(message) => input.push(InputItem::Message {
                    role: Role::System,
                    content: message.content.clone(),
                }),
                Message::User(message) => input.push(InputItem::Message {
                    role: Role::User,
                    content: message.content.clone(),
                }),
                Message::Assistant(assistant) => {
                    if let Some(output) = &assistant.output
                        && output.model == self.model
                        && output.provider == self.providers
                    {
                        input.extend(output.items.iter().cloned().map(InputItem::Item));
                    } else {
                        if let Some(content) = &assistant.content {
                            input.push(InputItem::Message {
                                role: Role::Assistant,
                                content: content.clone(),
                            });
                        }
                        for call in &assistant.tool_calls {
                            input.push(InputItem::Item(Item::FunctionCall(
                                responses::FunctionCall {
                                    id: None,
                                    status: None,
                                    call_id: call.id.clone(),
                                    name: call.name().to_owned(),
                                    arguments: call.arguments().to_owned(),
                                },
                            )));
                        }
                    }
                }
                Message::Tool(tool) => input.push(InputItem::Item(Item::FunctionCallOutput {
                    call_id: tool.tool_call_id.clone(),
                    output: tool.content.clone(),
                })),
            }
        }
        input
    }

    pub fn payload(
        &self,
        input: Vec<InputItem>,
        stream: bool,
        with_tools: bool,
    ) -> responses::Request<'_> {
        let tools = (with_tools && !self.tools.is_empty()).then_some(self.tools.as_slice());
        responses::Request {
            model: &self.model,
            input,
            stream,
            store: false,
            include: ["reasoning.encrypted_content"],
            reasoning: responses::ReasoningSettings {
                effort: self.reasoning_effort,
                summary: (stream && self.reasoning_effort != "none").then_some("auto"),
            },
            provider: self.providers().filter(|p| !p.order.is_empty()),
            service_tier: self.service_tier,
            cache_control: self
                .model
                .starts_with("anthropic/")
                .then_some(responses::CacheControl::Ephemeral),
            tools,
            tool_choice: tools.map(|_| {
                if stream {
                    responses::ToolChoice::Auto
                } else {
                    responses::ToolChoice::None
                }
            }),
            max_output_tokens: None,
        }
    }

    pub fn assistant_message(&self, items: Vec<Item>) -> anyhow::Result<AssistantMessage> {
        let mut ids = BTreeSet::new();
        let mut calls = Vec::new();
        for item in &items {
            let Item::FunctionCall(call) = item else {
                continue;
            };
            let id = call.call_id.as_str();
            anyhow::ensure!(!id.is_empty(), "missing function call ID");
            anyhow::ensure!(!call.name.is_empty(), "missing function name");
            anyhow::ensure!(ids.insert(id), "duplicate function call ID {id}");
            anyhow::ensure!(
                call.status.is_none_or(|status| status == Status::Completed),
                "unfinished function call {id}"
            );
            calls.push(ToolCall {
                id: id.into(),
                function: ToolCallFunction {
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                },
            });
        }
        let mut message = AssistantMessage::new(responses::output_text(&items, true), calls);
        message.output = Some(ResponseOutput {
            model: self.model.clone(),
            provider: self.providers.clone(),
            items,
        });
        Ok(message)
    }

    pub fn request(
        &self,
        client: &reqwest::blocking::Client,
        url: &str,
    ) -> reqwest::blocking::RequestBuilder {
        let request = client.post(url);
        if self.providers.is_none()
            && let Some(id) = &self.session.opencode_session
        {
            request.header("X-Opencode-Session", id)
        } else {
            request
        }
    }

    pub fn save_session(&self) -> anyhow::Result<()> {
        let path = get_session_path()?;
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir)?;
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer(file.as_file_mut(), &self.session)?;
        file.as_file().sync_all()?;
        file.persist(&path)?;
        Ok(())
    }

    pub fn resume(&mut self) -> anyhow::Result<()> {
        let bytes = fs::read(get_session_path()?)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        let session = if value.is_array() {
            Session {
                version: SESSION_VERSION,
                messages: serde_json::from_value(value)?,
                compactions: vec![],
                omitted: vec![],
                opencode_session: None,
            }
        } else {
            serde_json::from_value(value)?
        };
        session.validate()?;
        self.session = session;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    version: u32,
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compactions: Vec<Compaction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted: Vec<(usize, usize)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode_session: Option<String>,
}

impl Session {
    pub fn new_system(system: &str) -> Self {
        Self {
            version: SESSION_VERSION,
            messages: vec![Message::System(SimpleMessage::new(system))],
            compactions: vec![],
            omitted: vec![],
            opencode_session: None,
        }
    }

    pub fn first_kept(&self) -> usize {
        self.compactions
            .last()
            .map_or(1, |compaction| compaction.first_kept_message_index)
    }

    pub fn is_omitted(&self, index: usize) -> bool {
        self.omitted
            .iter()
            .any(|&(start, end)| index >= start && index < end)
    }

    pub fn active_messages(&self) -> Vec<Message> {
        let Some(system) = self.messages.first() else {
            return vec![];
        };
        let mut messages = vec![system.clone()];
        let first_kept = self.first_kept();
        if let Some(compaction) = self.compactions.last() {
            messages.push(Message::User(SimpleMessage::new(&summary_wrapper(
                &compaction.summary,
            ))));
        }
        for (index, message) in self.messages.iter().enumerate().skip(first_kept) {
            if self.is_omitted(index) {
                continue;
            }
            messages.push(message.clone());
        }
        messages
    }

    pub fn estimate_active_tokens(&self) -> u64 {
        self.active_messages()
            .iter()
            .map(Message::estimate_tokens)
            .sum()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version != SESSION_VERSION {
            anyhow::bail!(
                "unsupported session version {}; supported version is {SESSION_VERSION}",
                self.version
            );
        }
        if self.messages.is_empty() {
            anyhow::bail!("session has no messages");
        }
        if !matches!(self.messages[0], Message::System(_)) {
            anyhow::bail!("session must start with a system message");
        }
        let mut previous_boundary = 0;
        let mut previous_extent = 0;
        for compaction in &self.compactions {
            if compaction.summary.trim().is_empty() {
                anyhow::bail!("compaction has an empty summary");
            }
            if compaction.first_kept_message_index == 0
                || compaction.first_kept_message_index > self.messages.len()
            {
                anyhow::bail!(
                    "compaction boundary {} is outside the session transcript ({} messages)",
                    compaction.first_kept_message_index,
                    self.messages.len()
                );
            }
            if compaction.message_count_at_compaction > self.messages.len() {
                anyhow::bail!(
                    "compaction transcript extent {} exceeds the saved transcript ({} messages)",
                    compaction.message_count_at_compaction,
                    self.messages.len()
                );
            }
            if compaction.first_kept_message_index > compaction.message_count_at_compaction {
                anyhow::bail!("compaction boundary exceeds its recorded transcript extent");
            }
            if compaction.first_kept_message_index <= previous_boundary
                || compaction.message_count_at_compaction < previous_extent
            {
                anyhow::bail!(
                    "compaction boundaries and transcript extents must advance chronologically"
                );
            }
            previous_boundary = compaction.first_kept_message_index;
            previous_extent = compaction.message_count_at_compaction;
        }
        for &(start, end) in &self.omitted {
            if start >= end || end > self.messages.len() {
                anyhow::bail!("invalid omitted message range {start}..{end}");
            }
        }
        for boundary in std::iter::once(1).chain(
            self.compactions
                .iter()
                .map(|compaction| compaction.first_kept_message_index),
        ) {
            let mut pending = std::collections::BTreeSet::new();
            for (index, message) in self.messages.iter().enumerate().skip(boundary) {
                if self.is_omitted(index) {
                    continue;
                }
                match message {
                    Message::Tool(tool) => {
                        if !pending.remove(tool.tool_call_id.as_str()) {
                            anyhow::bail!("orphaned or duplicate tool result at message {index}");
                        }
                    }
                    Message::Assistant(assistant) => {
                        if !pending.is_empty() {
                            anyhow::bail!("missing tool results before message {index}");
                        }
                        for call in assistant.tool_calls() {
                            if !pending.insert(call.id()) {
                                anyhow::bail!("duplicate tool call ID at message {index}");
                            }
                        }
                    }
                    _ => {
                        if !pending.is_empty() {
                            anyhow::bail!("missing tool results before message {index}");
                        }
                    }
                }
            }
            if !pending.is_empty() {
                anyhow::bail!("session ends with missing tool results");
            }
        }
        Ok(())
    }
}

fn summary_wrapper(summary: &str) -> String {
    format!(
        "This is a summary of the earlier conversation. Use it as context; the messages \
         that follow continue the conversation.\n\n<context_summary>\n{summary}\n</context_summary>"
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Compaction {
    pub summary: String,
    pub first_kept_message_index: usize,
    pub message_count_at_compaction: usize,
    pub tokens_before: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<responses::Usage>,
    #[serde(default)]
    pub read_files: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub modified_files: std::collections::BTreeSet<String>,
    pub timestamp: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
    System(SimpleMessage),
    User(SimpleMessage),
    Assistant(AssistantMessage),
    Tool(Tool),
}

impl Message {
    pub fn estimate_tokens(&self) -> u64 {
        const OVERHEAD: u64 = 4;
        match self {
            Self::System(message) | Self::User(message) => {
                OVERHEAD + compaction::estimate_text_tokens(&message.content)
            }
            Self::Assistant(message) => {
                let mut tokens = OVERHEAD;
                if let Some(content) = &message.content {
                    tokens += compaction::estimate_text_tokens(content);
                }
                if let Some(output) = &message.output {
                    tokens += output
                        .items
                        .iter()
                        .filter_map(|item| match item {
                            Item::Reasoning { content, .. } => content.as_ref(),
                            _ => None,
                        })
                        .flatten()
                        .map(|part| match part {
                            responses::ReasoningPart::ReasoningText { text }
                            | responses::ReasoningPart::SummaryText { text } => {
                                compaction::estimate_text_tokens(text)
                            }
                        })
                        .sum::<u64>();
                }
                for call in &message.tool_calls {
                    tokens += 8
                        + compaction::estimate_text_tokens(&call.function.name)
                        + compaction::estimate_text_tokens(&call.function.arguments);
                }
                tokens
            }
            Self::Tool(tool) => OVERHEAD + 8 + compaction::estimate_text_tokens(&tool.content),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimpleMessage {
    content: String,
}

impl SimpleMessage {
    pub fn new(content: &str) -> Self {
        Self {
            content: content.into(),
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssistantMessage {
    content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output: Option<ResponseOutput>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ResponseOutput {
    model: String,
    provider: Option<ProviderPreferences>,
    items: Vec<Item>,
}

impl AssistantMessage {
    pub fn new(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            content,
            tool_calls,
            output: None,
        }
    }

    pub fn content(&self) -> Option<&str> {
        self.content.as_deref()
    }

    pub fn tool_calls(&self) -> &[ToolCall] {
        &self.tool_calls
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    id: String,
    function: ToolCallFunction,
}

impl ToolCall {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.function.name
    }

    pub fn arguments(&self) -> &str {
        &self.function.arguments
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tool {
    tool_call_id: String,
    content: String,
}

impl Tool {
    pub fn new(tool_call_id: String, content: String) -> Self {
        Self {
            tool_call_id,
            content,
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}
