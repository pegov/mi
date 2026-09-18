use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Local,
    #[command(name = "openrouter")]
    OpenRouter {
        #[arg(short, long, value_enum, default_value_t = OpenRouterPreset::DeepSeek)]
        preset: OpenRouterPreset,
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

        #[arg(long, num_args = 1..)]
        input_reference_path: Vec<String>,
    },
    Chan,
    Jev,
}

#[derive(Clone, Debug, Default, ValueEnum)]
pub enum OpenRouterPreset {
    #[default]
    None,
    #[value(name = "deepseek")]
    DeepSeek,
    #[value(name = "glm-flash")]
    GlmFlash,
    #[value(name = "luna")]
    OpenAILuna,
}

impl OpenRouterPreset {
    pub fn model(&self) -> &str {
        match *self {
            Self::None => "none",
            Self::DeepSeek => "deepseek/deepseek-v4.1-flash",
            Self::GlmFlash => "z-ai/glm-5.3-flash",
            Self::OpenAILuna => "openai/gpt-5.6-luna",
        }
    }

    pub fn provider(&self) -> Option<serde_json::Value> {
        match *self {
            Self::None => None,
            Self::DeepSeek => Some(serde_json::json!(
                {
                    "order": ["deepseek"],
                    "allow_fallbacks": false
                }
            )),
            Self::GlmFlash => Some(serde_json::json!(
            {
                "order": ["z-ai/fp8"],
                "allow_fallbacks": false
            }
            )),
            Self::OpenAILuna => Some(serde_json::json!(
            {
                "order": ["openai/flex"],
                "allow_fallbacks": false
            }
            )),
        }
    }

    pub fn service_tier(&self) -> Option<String> {
        match *self {
            Self::OpenAILuna => Some("flex".to_string()),
            _ => None,
        }
    }
}
