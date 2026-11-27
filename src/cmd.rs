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
    #[arg(long, visible_alias = "rwt", help = "Enable read/write/edit tools")]
    pub rw_tools: bool,
    #[arg(long, visible_alias = "imgt", help = "Enable the read_image tool")]
    pub image_tool: bool,
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
    #[arg(long)]
    pub show_reasoning: bool,
    #[arg(long, visible_alias = "sto")]
    pub show_tool_output: bool,
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

#[derive(Clone, Debug, ValueEnum)]
pub enum OpenCodeModel {
    #[value(name = "deepseek-v4.1-flash", alias = "deepseek")]
    DeepSeekFlash,
    #[value(name = "glm-5.3", alias = "glm")]
    Glm,
    #[value(name = "kimi-k3.1-pro", alias = "kimi")]
    Kimi,
    #[value(name = "gpt-6-luna", alias = "luna")]
    Luna,
    #[value(name = "qwen4-max", alias = "qwen")]
    Qwen4,
}

#[derive(Clone, Debug, Default, ValueEnum)]
pub enum OpenCodeReasoning {
    None,
    #[default]
    Low,
    Medium,
    High,
    Max,
}

impl OpenCodeReasoning {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }
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
    #[command(name = "opencode", visible_alias = "oc")]
    OpenCode {
        #[arg(short, long, value_enum, default_value = "deepseek-v4.1-flash")]
        model: OpenCodeModel,
        #[arg(short, long, visible_alias = "r", value_enum)]
        reasoning: Option<OpenCodeReasoning>,
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
    #[command(
        about = "Transcribe audio at POST /audio/transcriptions",
        long_about = "Transcribe audio at POST /audio/transcriptions. Send JSON with input_audio.data (base64) and input_audio.format, or one multipart field named 'file'. Supported formats: WAV, MP3, FLAC, M4A, and WebM. M4A and WebM require ffmpeg. The server has no authentication; bind only on trusted networks."
    )]
    SttServer {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 4300)]
        port: u16,
        #[arg(short, long, value_enum, default_value = "mai")]
        model: SttModel,
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

#[derive(Clone, Debug, ValueEnum)]
pub enum SttModel {
    Mai,
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
    #[serde(rename = "hy4-flash")]
    #[value(name = "hy4-flash")]
    Hy4Flash,
    #[serde(rename = "glm-flash")]
    #[value(name = "glm-flash")]
    GlmFlash,
    #[serde(rename = "luna/flex")]
    #[value(name = "luna/flex")]
    OpenAILunaFlex,
    #[serde(rename = "terra/flex")]
    #[value(name = "terra/flex")]
    OpenAITerraFlex,
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
    #[serde(rename = "neon")]
    #[value(name = "neon")]
    Gemini4Neon,
    #[serde(rename = "opus")]
    #[value(name = "opus")]
    AnthropicOpus,
}

impl OpenRouterModel {
    pub fn as_str(&self) -> &str {
        match *self {
            Self::None => "none",
            Self::DeepSeek => "deepseek",
            Self::Hy4Flash => "hy4-flash",
            Self::GlmFlash => "glm-flash",
            Self::OpenAILunaFlex => "luna/flex",
            Self::OpenAITerraFlex => "terra/flex",
            Self::OpenAILuna => "luna",
            Self::OpenAITerra => "terra",
            Self::OpenAISol => "sol",
            Self::Kimi => "kimi",
            Self::MuseEmber => "muse-ember",
            Self::Gemini4Neon => "neon",
            Self::AnthropicOpus => "opus",
        }
    }

    pub fn model(&self) -> &str {
        match *self {
            Self::None => "none",
            Self::DeepSeek => "deepseek/deepseek-v4.1-flash-1017",
            Self::Hy4Flash => "tencent/hy4-flash",
            Self::GlmFlash => "z-ai/glm-5.3-flash",
            Self::OpenAILunaFlex | Self::OpenAILuna => "openai/gpt-6-luna",
            Self::OpenAITerraFlex | Self::OpenAITerra => "openai/gpt-6.1-terra",
            Self::OpenAISol => "openai/gpt-6.2-sol",
            Self::Kimi => "moonshotai/kimi-k3.1-pro",
            Self::MuseEmber => "meta/muse-ember",
            Self::Gemini4Neon => "google/gemini-4-neon",
            Self::AnthropicOpus => "anthropic/claude-opus-5.5",
        }
    }

    pub fn provider_order(&self) -> &'static [&'static str] {
        match self {
            Self::None => &[],
            Self::DeepSeek => &["deepseek"],
            Self::Hy4Flash => &["tencent/fp8"],
            Self::GlmFlash => &["z-ai/fp8"],
            Self::OpenAILunaFlex | Self::OpenAITerraFlex => &["openai/flex"],
            Self::OpenAILuna | Self::OpenAITerra | Self::OpenAISol => &["openai"],
            Self::Kimi => &["moonshotai/mxfp4"],
            Self::MuseEmber => &["meta"],
            Self::Gemini4Neon => &["google-vertex/global"],
            Self::AnthropicOpus => &["anthropic"],
        }
    }

    pub fn provider(&self) -> crate::responses::ProviderPreferences {
        crate::responses::ProviderPreferences {
            order: self
                .provider_order()
                .iter()
                .map(|&name| name.to_owned())
                .collect(),
            allow_fallbacks: false,
        }
    }

    pub fn service_tier(&self) -> Option<&'static str> {
        match *self {
            Self::OpenAILunaFlex | Self::OpenAITerraFlex => Some("flex"),
            Self::None
            | Self::DeepSeek
            | Self::Hy4Flash
            | Self::GlmFlash
            | Self::OpenAILuna
            | Self::OpenAITerra
            | Self::OpenAISol
            | Self::Kimi
            | Self::MuseEmber
            | Self::Gemini4Neon
            | Self::AnthropicOpus => None,
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
    pub fn as_str(&self) -> &'static str {
        match *self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}
