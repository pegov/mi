use std::{collections::HashMap, env, fs, path::PathBuf, process::Command};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub local: LocalConfig,
    pub openrouter: OpenRouterConfig,
    pub jev: JevConfig,
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
}

#[derive(Debug, Deserialize)]
pub struct Profile {
    #[serde(alias = "system_prompt_presets")]
    pub spp: Vec<String>,
}

impl Config {
    pub fn system_prompt_presets(
        &self,
        profile: Option<&str>,
        cli_spp: Vec<String>,
    ) -> Result<Vec<String>> {
        let mut spp = match profile {
            Some(name) => {
                let mut available = self.profiles.keys().map(String::as_str).collect::<Vec<_>>();
                available.sort();
                let profile = self.profiles.get(name).ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown profile {name:?}, available: {}",
                        available.join(", ")
                    )
                })?;
                println!("Loaded profile {name:?} | spps: {}", profile.spp.join(", "));
                profile.spp.clone()
            }
            None => Vec::new(),
        };
        spp.extend(cli_spp);
        Ok(spp)
    }
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

pub fn load_system_prompt_preset(name: &str) -> Result<String> {
    let dir = must_config_dir().join("sp-presets");

    let mut names = fs::read_dir(&dir)
        .context(format!("reading: {}", dir.display()))?
        .map(|entry| {
            let entry = entry?;
            let path = entry.path();

            if !entry.file_type()?.is_file() || path.extension().is_none_or(|ext| ext != "md") {
                return Ok(None);
            }

            Ok(path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned))
        })
        .collect::<Result<Vec<Option<String>>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    names.sort();
    if !names.iter().any(|preset| preset == name) {
        anyhow::bail!(
            "unknown system prompt preset {name:?}, available: {}",
            names.join(", ")
        );
    }

    let path = dir.join(format!("{name}.md"));
    fs::read_to_string(&path).context(format!("reading {}", path.display()))
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
