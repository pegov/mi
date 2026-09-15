use crate::completions;

pub struct AssistantMessage {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

pub struct ToolCall {
    pub id: String,
    pub index: usize,
    pub type_: String,
    pub function: String,
    pub arguments: String,
}

impl ToolCall {
    pub fn new(id: &str, index: usize, type_: &str, function: &str) -> Self {
        Self {
            id: id.into(),
            index,
            type_: type_.into(),
            function: function.into(),
            arguments: String::new(),
        }
    }

    pub fn push_arguments(&mut self, arguments: &str) {
        self.arguments.push_str(arguments);
    }
}

#[derive(Default)]
pub struct Assembler {
    content: Option<String>,
    tool_calls: Vec<ToolCall>,
}

impl Assembler {
    pub fn push_delta(&mut self, answer: &completions::Answer) {
        let choice = &answer.choices[0];
        let delta = &choice.delta;

        if let Some(ref content) = delta.content {
            if let Some(ref mut a_content) = self.content {
                a_content.push_str(content);
                return;
            } else {
                self.content = Some(String::from(content));
            }
        }

        if let Some(ref tool_calls) = delta.tool_calls {
            for tool_call in tool_calls {
                if let Some(ref id) = tool_call.id
                    && let Some(ref name) = tool_call.function.name
                    && let Some(ref type_) = tool_call.type_
                {
                    let mut a_tool_call = ToolCall::new(id, tool_call.index, type_, name);
                    if let Some(ref arguments) = tool_call.function.arguments {
                        a_tool_call.push_arguments(arguments);
                    }
                    self.tool_calls.push(a_tool_call);
                } else {
                    if tool_call.index + 1 > self.tool_calls.len() {
                        eprintln!(
                            "tool call index out of range: {}, len = {}",
                            tool_call.index,
                            self.tool_calls.len()
                        );
                        return;
                    }

                    let a_tool_call = &mut self.tool_calls[tool_call.index];
                    if let Some(ref arguments) = tool_call.function.arguments {
                        a_tool_call.push_arguments(arguments);
                        return;
                    }
                }
            }
        }
    }

    pub fn assemble(self) -> AssistantMessage {
        AssistantMessage {
            content: self.content,
            tool_calls: self.tool_calls,
        }
    }
}
