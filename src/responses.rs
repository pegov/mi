use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize)]
pub struct Request<'a> {
    pub model: &'a str,
    pub input: Vec<InputItem>,
    pub stream: bool,
    pub store: bool,
    pub include: [&'static str; 1],
    pub reasoning: ReasoningSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<&'a ProviderPreferences>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<&'a [Value]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ProviderPreferences {
    pub order: Vec<String>,
    pub allow_fallbacks: bool,
}

#[derive(Serialize)]
pub struct ReasoningSettings {
    pub effort: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CacheControl {
    Ephemeral,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    None,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum InputItem {
    Message { role: Role, content: String },
    Item(Item),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    Developer,
    User,
    Assistant,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    Message {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        role: Role,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<Status>,
        #[serde(skip_serializing_if = "Option::is_none")]
        phase: Option<String>,
        content: Vec<MessagePart>,
    },
    FunctionCall(FunctionCall),
    FunctionCallOutput {
        call_id: String,
        output: FunctionOutput,
    },
    Reasoning {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<Status>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<Vec<ReasoningPart>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<Vec<ReasoningPart>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        encrypted_content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FunctionOutput {
    Text(String),
    Parts(Vec<FunctionOutputPart>),
}

impl FunctionOutput {
    pub fn display_text(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Parts(_) => "[Image attached]",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FunctionOutputPart {
    InputImage { image_url: String, detail: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningPart {
    ReasoningText { text: String },
    SummaryText { text: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessagePart {
    InputText {
        text: String,
    },
    InputImage {
        image_url: String,
        detail: String,
    },
    OutputText {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Vec<Annotation>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<Logprob>>,
    },
    Refusal {
        refusal: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Annotation {
    UrlCitation {
        url: String,
        title: String,
        start_index: usize,
        end_index: usize,
    },
    FileCitation {
        file_id: String,
        filename: String,
        index: usize,
    },
    ContainerFileCitation {
        container_id: String,
        file_id: String,
        filename: String,
        start_index: usize,
        end_index: usize,
    },
    FilePath {
        file_id: String,
        index: usize,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Logprob {
    token: String,
    logprob: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_logprobs: Option<Vec<Logprob>>,
}

pub fn output_text(items: &[Item], include_refusals: bool) -> Option<String> {
    let text: String = items
        .iter()
        .filter_map(|item| match item {
            Item::Message { content, .. } => Some(content),
            _ => None,
        })
        .flatten()
        .filter_map(|part| match part {
            MessagePart::OutputText { text, .. } => Some(text.as_str()),
            MessagePart::Refusal { refusal } if include_refusals => Some(refusal.as_str()),
            _ => None,
        })
        .collect();
    (!text.is_empty()).then_some(text)
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    InProgress,
    Completed,
    Incomplete,
    Failed,
    Cancelled,
}

#[derive(Deserialize)]
pub struct Response {
    pub status: Option<Status>,
    pub output: Option<Vec<Item>>,
    pub error: Option<ApiError>,
    pub usage: Option<Usage>,
    pub incomplete_details: Option<IncompleteDetails>,
}

#[derive(Debug, Deserialize)]
pub struct IncompleteDetails {
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ApiError {
    Text(String),
    Details(ErrorDetails),
}

#[derive(Debug, Deserialize)]
pub struct ErrorDetails {
    code: Option<ErrorCode>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ErrorCode {
    Name(String),
    Number(i64),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => f.write_str(text),
            Self::Details(details) => details.fmt(f),
        }
    }
}

impl fmt::Display for ErrorDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.code {
            Some(ErrorCode::Name(code)) => write!(f, "{code}: ")?,
            Some(ErrorCode::Number(code)) => write!(f, "{code}: ")?,
            None => {}
        }
        f.write_str(self.message.as_deref().unwrap_or("unspecified API error"))
    }
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    #[serde(
        rename = "response.output_text.delta",
        alias = "response.refusal.delta"
    )]
    TextDelta { delta: String },
    #[serde(
        rename = "response.reasoning_text.delta",
        alias = "response.reasoning_summary_text.delta"
    )]
    ReasoningDelta { delta: String },
    #[serde(rename = "response.output_item.added")]
    ItemAdded { item: AddedItem },
    #[serde(
        rename = "response.completed",
        alias = "response.incomplete",
        alias = "response.failed",
        alias = "response.cancelled"
    )]
    Finished { response: Response },
    #[serde(rename = "error")]
    Error {
        error: Option<ApiError>,
        #[serde(flatten)]
        details: ErrorDetails,
        usage: Option<Usage>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
pub struct AddedItem {
    #[serde(rename = "type")]
    pub kind: ItemKind,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    FunctionCall,
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Usage {
    #[serde(alias = "prompt_tokens")]
    pub input_tokens: u64,
    #[serde(alias = "completion_tokens")]
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost: Option<f64>,
    #[serde(default, alias = "prompt_tokens_details")]
    pub input_tokens_details: InputTokensDetails,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct InputTokensDetails {
    #[serde(default)]
    pub cached_tokens: u64,
}

impl Usage {
    pub fn accumulate(total: &mut Option<Self>, usage: &Self) {
        if let Some(total) = total {
            total.input_tokens += usage.input_tokens;
            total.output_tokens += usage.output_tokens;
            total.total_tokens += usage.total_tokens;
            total.input_tokens_details.cached_tokens += usage.input_tokens_details.cached_tokens;
            if let Some(cost) = usage.cost {
                *total.cost.get_or_insert(0.0) += cost;
            }
        } else {
            *total = Some(usage.clone());
        }
    }
}
