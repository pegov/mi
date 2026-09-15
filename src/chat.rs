use serde::Serialize;

use crate::assembler;

const TOOL_CHOICE_AUTO: &str = "auto";

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Chat {
    model: String,
    providers: Option<serde_json::Value>,
    reasoning_effort: Option<String>,
    tools: Option<Vec<serde_json::Value>>,
    tool_choice: &'static str,
    stream: bool,
    pub messages: Vec<Message>,
}

impl Chat {
    pub fn new(
        model: &str,
        providers: Option<serde_json::Value>,
        reasoning_effort: Option<String>,
        tools: Option<Vec<serde_json::Value>>,
        system: &str,
        user: &str,
    ) -> Self {
        let system_message = SimpleMessage::new(system);
        let user_message = SimpleMessage::new(user);

        Self {
            model: model.into(),
            providers,
            reasoning_effort,
            tools,
            tool_choice: TOOL_CHOICE_AUTO,
            stream: true,
            messages: vec![Message::System(system_message), Message::User(user_message)],
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
    System(SimpleMessage),
    User(SimpleMessage),
    Assistant(AssistantMessage),
    Tool(Tool),
}

#[derive(Debug, Serialize)]
pub struct SimpleMessage {
    content: String,
}

impl SimpleMessage {
    pub fn new(content: &str) -> Self {
        Self {
            content: content.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AssistantMessage {
    content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ToolCall>,
}

impl AssistantMessage {
    pub fn new(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            content,
            tool_calls,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    type_: String,
    index: usize,
    function: ToolCallFunction,
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

#[derive(Debug, Serialize)]
pub struct ToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
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
}
