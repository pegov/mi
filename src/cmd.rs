use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        num_args = 0..=1,
        default_missing_value = "true",
        require_equals = true,
        help = "Summarize using the conversation prefix for prompt caching (default: true; config: compaction.cache_aware)"
    )]
    pub cache_aware_compaction: Option<bool>,
    #[arg(
        long,
        global = true,
        value_name = "PERCENT",
        help = "Compact when this percentage of the context window is used (default: 45; config: compaction.compact_at_percent)"
    )]
    pub compaction_at_percent: Option<u64>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, clap::Args)]
pub struct ChatArgs {
    #[arg(short = 's', long, help = "Enable web search and fetch tools")]
    pub with_search: bool,
    #[arg(
        long,
        value_enum,
        default_value = "deepseek",
        help = "Provider for web search and fetch"
    )]
    pub search_provider: SearchProvider,
    #[arg(long, value_name = "PROMPT")]
    pub oneshot: Option<String>,
    #[arg(long)]
    pub resume: bool,
    #[arg(short, long)]
    pub profile: Option<String>,
    #[arg(long, visible_alias = "spp")]
    pub system_prompt_preset: Vec<String>,
    #[arg(long, visible_alias = "ddsp")]
    pub disable_default_system_prompt: bool,
    #[arg(long, visible_alias = "iram")]
    pub ignore_root_agents_md: bool,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum SearchProvider {
    Deepseek,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Local {
        #[command(flatten)]
        chat: ChatArgs,
    },
    #[command(name = "deepseek", alias = "ds")]
    DeepSeek {
        #[arg(short, long, value_enum)]
        reasoning: Option<OpenRouterReasoning>,
        #[command(flatten)]
        chat: ChatArgs,
    },
    #[command(name = "openrouter", alias = "or")]
    OpenRouter {
        #[arg(short, long, value_enum)]
        model: Option<OpenRouterModel>,
        #[arg(short, long, value_enum)]
        reasoning: Option<OpenRouterReasoning>,
        #[command(flatten)]
        chat: ChatArgs,
    },
    #[command(name = "image-gen")]
    ImageGen {
        #[arg(long, default_value = "bytedance-seed/seedream-4.5")]
        model: String,

        #[arg(long)]
        prompt: String,

        #[arg(long, default_value = "2K")]
        resolution: String,

        #[arg(long, default_value = "16:9")]
        aspect_ratio: String,

        #[arg(long)]
        output_dir: PathBuf,

        #[arg(long, num_args = 1..)]
        input_reference_path: Vec<String>,
    },
    Chan,
    Jev,
    #[command(name = "mcp-http-server")]
    McpHttpServer,
    #[command(name = "mcp-server")]
    McpServer,
    #[command(name = "mcp-client")]
    McpClient,
}

#[derive(Clone, Debug, Default, Deserialize, ValueEnum)]
pub enum OpenRouterModel {
    #[default]
    #[serde(rename = "none")]
    #[value(name = "none")]
    None,
    #[serde(rename = "deepseek")]
    #[value(name = "deepseek")]
    DeepSeek,
    #[serde(rename = "glm-flash")]
    #[value(name = "glm-flash")]
    GlmFlash,
    #[serde(rename = "luna/flex")]
    #[value(name = "luna/flex")]
    OpenAILunaFlex,
    #[serde(rename = "terra/flex")]
    #[value(name = "terra/flex")]
    OpenAITerraFlex,
    #[serde(rename = "sol/flex")]
    #[value(name = "sol/flex")]
    OpenAISolFlex,
    #[serde(rename = "luna")]
    #[value(name = "luna")]
    OpenAILuna,
    #[serde(rename = "terra")]
    #[value(name = "terra")]
    OpenAITerra,
    #[serde(rename = "sol")]
    #[value(name = "sol")]
    OpenAISol,
    #[serde(rename = "kimi")]
    #[value(name = "kimi")]
    Kimi,
    #[serde(rename = "muse-ember")]
    #[value(name = "muse-ember")]
    MuseEmber,
}

impl OpenRouterModel {
    pub fn as_str(&self) -> &str {
        match *self {
            Self::None => "none",
            Self::DeepSeek => "deepseek",
            Self::GlmFlash => "glm-flash",
            Self::OpenAILunaFlex => "luna/flex",
            Self::OpenAITerraFlex => "terra/flex",
            Self::OpenAISolFlex => "sol/flex",
            Self::OpenAILuna => "luna",
            Self::OpenAITerra => "terra",
            Self::OpenAISol => "sol",
            Self::Kimi => "kimi",
            Self::MuseEmber => "muse-ember",
        }
    }

    pub fn model(&self) -> &str {
        match *self {
            Self::None => "none",
            Self::DeepSeek => "deepseek/deepseek-v4.1-flash-1017",
            Self::GlmFlash => "z-ai/glm-5.3-flash",
            Self::OpenAILunaFlex | Self::OpenAILuna => "openai/gpt-6-luna",
            Self::OpenAITerraFlex | Self::OpenAITerra => "openai/gpt-6.1-terra",
            Self::OpenAISolFlex | Self::OpenAISol => "openai/gpt-6.1-sol",
            Self::Kimi => "moonshotai/kimi-k3.1-pro",
            Self::MuseEmber => "meta/muse-ember",
        }
    }

    pub fn provider_order(&self) -> &'static [&'static str] {
        match self {
            Self::None => &[],
            Self::DeepSeek => &["deepseek"],
            Self::GlmFlash => &["z-ai/fp8"],
            Self::OpenAILunaFlex | Self::OpenAITerraFlex | Self::OpenAISolFlex => &["openai/flex"],
            Self::OpenAILuna | Self::OpenAITerra | Self::OpenAISol => &["openai"],
            Self::Kimi => &["moonshotai/mxfp4"],
            Self::MuseEmber => &["meta"],
        }
    }

    pub fn provider(&self) -> serde_json::Value {
        serde_json::json!({
            "order": self.provider_order(),
            "allow_fallbacks": false,
        })
    }

    pub fn service_tier(&self) -> Option<&'static str> {
        match *self {
            Self::OpenAILunaFlex | Self::OpenAITerraFlex | Self::OpenAISolFlex => Some("flex"),
            Self::None
            | Self::DeepSeek
            | Self::GlmFlash
            | Self::OpenAILuna
            | Self::OpenAITerra
            | Self::OpenAISol
            | Self::Kimi
            | Self::MuseEmber => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ValueEnum)]
pub enum OpenRouterReasoning {
    #[serde(rename = "none")]
    #[value(name = "none")]
    None,
    #[default]
    #[serde(rename = "low")]
    #[value(name = "low")]
    Low,
    #[serde(rename = "medium")]
    #[value(name = "medium")]
    Medium,
    #[serde(rename = "high")]
    #[value(name = "high")]
    High,
}

impl std::str::FromStr for OpenRouterModel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        clap::ValueEnum::from_str(s, true)
    }
}

impl std::str::FromStr for OpenRouterReasoning {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        clap::ValueEnum::from_str(s, true)
    }
}

impl OpenRouterReasoning {
    pub fn as_str(&self) -> &str {
        match *self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}
