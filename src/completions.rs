use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Message {
    pub role: Option<Role>,
    pub content: Option<String>,
    #[serde(alias = "reasoning_content")]
    pub reasoning: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolCall {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub index: usize,
    pub function: ToolCallFunction,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolCallFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

impl Message {
    pub fn create(role: Role, content: String) -> Self {
        return Self {
            role: Some(role),
            content: Some(content),
            reasoning: None,
            tool_calls: None,
        };
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Chat {
    pub model: String,
    pub provider: serde_json::Value,
    pub reasoning_effort: ReasoningEffortOpenAI,
    pub messages: Vec<Message>,
    pub stream: bool,
    pub tools: Option<Vec<serde_json::Value>>,
    pub tool_choice: ToolChoice,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffortOpenAI {
    None,
    // Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    None,
    Auto,
}

// #[derive(Deserialize, Serialize)]
// #[serde(rename_all = "snake_case")]
// pub struct Tool {
//     #[serde(rename = "type")]
//     type_: String,
//     function: Function,
// }
//
// #[derive(Deserialize, Serialize)]
// #[serde(rename_all = "snake_case")]
// pub struct Function {
//     name: String,
//     description: String,
//     parameters:
// }

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Function {}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Answer {
    pub choices: Vec<Choice>,
    created: u64,
    id: String,
    model: String,
    object: String,
    pub usage: Option<Usage>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Choice {
    pub finish_reason: Option<FinishReason>,
    pub index: usize,
    pub message: Option<Message>,
    pub delta: Message,
    pub logprobs: Option<()>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    ToolCalls,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Usage {
    pub completion_tokens: u64,
    pub prompt_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub cost: Option<f64>,
    #[serde(default)]
    pub prompt_tokens_details: PromptTokensDetails,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PromptTokensDetails {
    #[serde(default)]
    pub cached_tokens: u64,
}
