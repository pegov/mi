pub(super) fn format_lines(lines: &[&str], first_line: usize, numbered: bool) -> String {
    let mut output = String::new();
    for (offset, line) in lines.iter().enumerate() {
        if offset != 0 {
            output.push('\n');
        }
        if numbered {
            output.push_str(&(first_line + offset).to_string());
            output.push_str(": ");
        }
        output.push_str(line);
    }
    output
}

pub trait Tool {
    fn name(&self) -> &str;
    fn json(&self) -> serde_json::Value;
    fn validate_args(&self, args: &str) -> anyhow::Result<()>;
    fn note(&self, args: &str) -> anyhow::Result<String>;
    fn call(&mut self, args: &str) -> anyhow::Result<String>;
}
