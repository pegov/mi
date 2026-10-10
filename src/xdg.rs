use std::{collections::HashMap, env, fs, path::PathBuf, process::Command};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::cmd::{OpenRouterModel, OpenRouterReasoning};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub local: LocalConfig,
    pub openrouter: OpenRouterConfig,
    pub jev: JevConfig,
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Profile {
    #[serde(default, alias = "system_prompt_presets")]
    pub spp: Vec<String>,
    pub model: Option<OpenRouterModel>,
    pub reasoning: Option<OpenRouterReasoning>,
}

impl Config {
    pub fn load_profile(&self, profile: Option<&str>, cli_spp: Vec<String>) -> Result<Profile> {
        let mut profile = match profile {
            Some(name) => {
                let mut available = self.profiles.keys().map(String::as_str).collect::<Vec<_>>();
                available.sort();
                let profile = self.profiles.get(name).ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown profile {name:?}, available: {}",
                        available.join(", ")
                    )
                })?;
                println!(
                    "Loaded profile {name:?} | spps: {} | model: {} | reasoning: {}",
                    profile.spp.join(", "),
                    profile
                        .model
                        .as_ref()
                        .map_or("default", OpenRouterModel::as_str),
                    profile
                        .reasoning
                        .as_ref()
                        .map_or("default", OpenRouterReasoning::as_str),
                );
                profile.clone()
            }
            None => Profile::default(),
        };
        profile.spp.extend(cli_spp);
        Ok(profile)
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

fn get_config_dir() -> Result<PathBuf> {
    let dir = match env::var_os("XDG_CONFIG_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => env::home_dir()
            .ok_or(anyhow::anyhow!("could not find home directory for config"))?
            .join(".config"),
    };
    Ok(dir.join("mi"))
}

pub fn load_system_prompt_preset(name: &str) -> Result<String> {
    let dir = get_config_dir()?.join("sp-presets");

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

pub fn get_skills_dir() -> Result<PathBuf> {
    Ok(env::home_dir()
        .ok_or(anyhow::anyhow!("could not find home directory for skills"))?
        .join(".agents")
        .join("skills"))
}

pub fn get_session_path() -> Result<PathBuf> {
    let dir = match env::var_os("XDG_STATE_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => env::home_dir()
            .ok_or(anyhow::anyhow!("could not find home directory for session"))?
            .join(".local/state"),
    };
    Ok(dir.join("mi").join("session.json"))
}

pub fn get_parse_config() -> Result<Config> {
    let config_path = get_config_dir()?.join("config.json");
    let config_str = fs::read_to_string(&config_path)?;
    Ok(serde_json::from_str(&config_str)?)
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
