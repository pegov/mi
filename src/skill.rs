use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use walkdir::WalkDir;

use crate::xdg::must_skills_dir;

#[derive(Clone, Debug)]
pub struct Skill {
    pub base_path: PathBuf,
    pub full: String,
    pub name: String,
    pub description: String,
    pub disable_model_invocation: bool,
    pub body: String,
}

#[derive(Deserialize)]
struct SkillYaml {
    name: String,
    description: String,
    #[serde(rename = "disable-model-invocation", default)]
    disable_model_invocation: bool,
}

struct SkillParsed {
    full: String,
    yaml: SkillYaml,
    body: String,
}

impl SkillParsed {
    fn new(full: String, yaml: SkillYaml, body: String) -> Self {
        Self { full, yaml, body }
    }
}

const SKILL_MD: &str = "SKILL.md";

impl Skill {
    fn new(skill_parsed: SkillParsed, base_path: PathBuf) -> Self {
        Self {
            base_path,
            full: skill_parsed.full,
            name: skill_parsed.yaml.name,
            description: skill_parsed.yaml.description,
            disable_model_invocation: skill_parsed.yaml.disable_model_invocation,
            body: skill_parsed.body,
        }
    }

    pub fn from_dir(dir_path: &Path) -> anyhow::Result<Skill> {
        let skill_path = dir_path.join(SKILL_MD);

        let content = fs::read_to_string(skill_path)?;
        let skill_parsed = parse_skill_str(&content)?;

        Ok(Skill::new(skill_parsed, dir_path.to_owned()))
    }

    fn format(&self) -> String {
        format!(
            "- [{}] {}: {}",
            self.base_path.to_string_lossy(),
            self.name,
            self.description,
        )
    }
}

fn parse_skill_str(skill_str: &str) -> anyhow::Result<SkillParsed> {
    let start = skill_str
        .strip_prefix("---")
        .ok_or(anyhow::anyhow!("wrong md format: first!"))?;

    let end_pos = start
        .find("\n---")
        .ok_or(anyhow::anyhow!("wrong md format: second!"))?;

    let yaml_str = &start[..end_pos];
    let yaml: SkillYaml = serde_yaml::from_str(yaml_str)?;
    let body = &start[end_pos + 4..].trim();

    Ok(SkillParsed::new(
        skill_str.to_owned(),
        yaml,
        body.to_string(),
    ))
}

pub fn parse_skills() -> anyhow::Result<Vec<Skill>> {
    WalkDir::new(must_skills_dir())
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .filter(|e| e.path().join(SKILL_MD).is_file())
        .map(|e| e.into_path())
        .map(|dir| Skill::from_dir(&dir))
        .collect()
}

pub fn format_skills(skills: &[Skill]) -> String {
    assert!(!skills.is_empty());

    let skills_format = skills
        .iter()
        .map(|s| s.format())
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Available skills:\n\n{skills_format}\n\nIf a user request matches any skill descriptions, load it first using `<base_path>/SKILL.md`."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_str() {
        let skill_str = "---
name: name
description: desc
---
FULL BODY";

        let skill_yaml = parse_skill_str(skill_str).unwrap();
        assert_eq!(skill_yaml.yaml.name, "name");
        assert_eq!(skill_yaml.yaml.description, "desc");
        assert_eq!(skill_yaml.body, "FULL BODY");
    }
}

pub fn manually_invoke_skill(skills: &[Skill], name: &str, args: &str) -> Option<String> {
    for skill in skills {
        if skill.name != name {
            continue;
        }

        if args != "" {
            return Some(format!(
                "[Manual skill invocation]\nAuto-inserting SKILL.md text:\n{}\nARGUMENTS: {}",
                skill.full.trim(),
                args.trim()
            ));
        } else {
            return Some(skill.full.clone());
        }
    }

    None
}
