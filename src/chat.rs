use serde::{Deserialize, Serialize};
use std::fs;

use crate::{assembler, compaction, xdg::get_session_path};

const TOOL_CHOICE_AUTO: &str = "auto";
const SESSION_VERSION: u32 = 1;

#[derive(Serialize)]
pub struct Chat {
    model: String,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    providers: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_tier: Option<&'static str>,
    #[serde(flatten)]
    reasoning: Reasoning,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<serde_json::Value>,
    tool_choice: &'static str,
    stream: bool,
    #[serde(rename = "messages", serialize_with = "serialize_active_messages")]
    pub session: Session,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

impl Chat {
    pub fn new(
        model: &str,
        providers: serde_json::Value,
        service_tier: Option<&'static str>,
        reasoning_effort: &'static str,
        tools: Vec<serde_json::Value>,
        system: &str,
    ) -> Self {
        let reasoning = if providers.is_null() {
            Reasoning::OpenCode { reasoning_effort }
        } else {
            Reasoning::Other {
                reasoning: Effort {
                    effort: reasoning_effort,
                },
            }
        };
        Self {
            model: model.into(),
            providers,
            service_tier,
            reasoning,
            tools,
            tool_choice: TOOL_CHOICE_AUTO,
            stream: true,
            session: Session::new_system(system),
            cache_control: cache_control(model),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn set_model(&mut self, model: &crate::cmd::OpenRouterModel, providers: serde_json::Value) {
        self.model = model.model().into();
        self.providers = providers;
        self.service_tier = model.service_tier();
        self.cache_control = cache_control(&self.model);
    }

    pub fn reasoning(&self) -> &str {
        match &self.reasoning {
            Reasoning::OpenCode { reasoning_effort } => reasoning_effort,
            Reasoning::Other { reasoning } => reasoning.effort,
        }
    }

    pub fn set_reasoning(&mut self, effort: &'static str) {
        match &mut self.reasoning {
            Reasoning::OpenCode { reasoning_effort } => *reasoning_effort = effort,
            Reasoning::Other { reasoning } => reasoning.effort = effort,
        }
    }

    pub fn providers(&self) -> &serde_json::Value {
        &self.providers
    }

    pub fn service_tier(&self) -> Option<&'static str> {
        self.service_tier
    }

    pub fn request(
        &self,
        client: &reqwest::blocking::Client,
        url: &str,
    ) -> reqwest::blocking::RequestBuilder {
        let request = client.post(url);
        if self.providers.is_null()
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

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    type_: &'static str,
}

fn cache_control(model: &str) -> Option<CacheControl> {
    model
        .starts_with("anthropic/")
        .then_some(CacheControl { type_: "ephemeral" })
}

fn serialize_active_messages<S>(session: &Session, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    session.active_messages().serialize(serializer)
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
    pub usage: Option<crate::completions::Usage>,
    #[serde(default)]
    pub read_files: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub modified_files: std::collections::BTreeSet<String>,
    pub timestamp: u64,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Reasoning {
    OpenCode { reasoning_effort: &'static str },
    Other { reasoning: Effort },
}

#[derive(Serialize)]
struct Effort {
    effort: &'static str,
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
}

impl AssistantMessage {
    pub fn new(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            content,
            tool_calls,
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
    #[serde(rename = "type")]
    type_: String,
    index: usize,
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

impl From<&assembler::ToolCall> for ToolCall {
    fn from(value: &assembler::ToolCall) -> Self {
        Self {
            id: value.id.clone(),
            type_: value.type_.clone(),
            index: value.index,
            function: ToolCallFunction {
                name: value.function.clone(),
                arguments: value.arguments.clone(),
            },
        }
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
