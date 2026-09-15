pub trait Tool {
    fn name(&self) -> &str;
    fn json(&self) -> serde_json::Value;
    fn validate_args(&self, args: &str) -> anyhow::Result<()>;
    fn note(&self, args: &str) -> anyhow::Result<String>;
    fn call(&mut self, args: &str) -> anyhow::Result<String>;
}
