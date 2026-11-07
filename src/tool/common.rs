pub fn format_lines(lines: &[&str], first_line: usize, numbered: bool) -> String {
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
    fn call(&mut self, args: &str) -> anyhow::Result<String>;
    fn call_cancellable(
        &mut self,
        args: &str,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> anyhow::Result<String> {
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
        }
        self.call(args)
    }
}

pub enum WebToolKind {
    Search,
    Fetch,
}

impl WebToolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Fetch => "fetch",
        }
    }
}

pub fn web_request(
    client: &reqwest::blocking::Client,
    base_url: &str,
    tool: WebToolKind,
    args: &impl serde::Serialize,
) -> anyhow::Result<String> {
    let url = format!("{}/tools/{}", base_url.trim_end_matches('/'), tool.as_str());
    let response = client.post(url).json(args).send()?;
    let status = response.status();
    let body = response.text()?;
    if !status.is_success() {
        anyhow::bail!("API {status} error: {body}");
    }
    Ok(body)
}
