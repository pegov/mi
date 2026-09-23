use std::{env, fs, path::PathBuf, process::Command};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub local: LocalConfig,
    pub openrouter: OpenRouterConfig,
    pub jev: JevConfig,
}

#[derive(Debug, Deserialize)]
pub struct LocalConfig {
    pub base_url: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenRouterConfig {
    pub base_url: String,
}

#[derive(Debug, Deserialize)]
pub struct JevConfig {
    pub url: String,
}

fn must_config_dir() -> PathBuf {
    PathBuf::from(
        env::var_os("XDG_CONFIG_HOME")
            .unwrap_or_else(|| env::home_dir().unwrap().join(".config").into()),
    )
    .join("mi")
}

pub fn must_skills_dir() -> PathBuf {
    env::home_dir()
        .unwrap()
        .join(".agents")
        .join("skills")
        .into()
}

pub fn must_session_path() -> PathBuf {
    PathBuf::from(
        env::var_os("XDG_STATE_HOME")
            .unwrap_or_else(|| env::home_dir().unwrap().join(".local/state").into()),
    )
    .join("mi")
    .join("session.json")
}

pub fn must_parse_config() -> Config {
    let config_path = must_config_dir().join("config.json");
    let config_str = fs::read_to_string(&config_path).unwrap();
    serde_json::from_str(&config_str).unwrap()
}

pub fn open_editor(user_prompt: &str) -> anyhow::Result<String> {
    let temp_file = tempfile::NamedTempFile::new()?;
    let temp_file_path = temp_file.path();

    fs::write(temp_file_path, user_prompt)?;

    let status = Command::new("nvim")
        .args(&["-c", "normal! G$", &temp_file_path.to_string_lossy()])
        .status()?;

    if !status.success() {
        anyhow::bail!("failed to write file");
    }

    Ok(fs::read_to_string(temp_file_path)?.trim().to_string())
}
