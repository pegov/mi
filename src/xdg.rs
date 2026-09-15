use std::{env, fs, path::PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub local: LocalConfig,
    pub openrouter: OpenRouterConfig,
}

#[derive(Debug, Deserialize)]
pub struct LocalConfig {
    pub base_url: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenRouterConfig {
    pub base_url: String,
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

pub fn must_parse_config() -> Config {
    let config_path = must_config_dir().join("config.json");
    let config_str = fs::read_to_string(&config_path).unwrap();
    serde_json::from_str(&config_str).unwrap()
}
