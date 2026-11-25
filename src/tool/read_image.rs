use std::{fs, io::Read, path::PathBuf};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;

use crate::responses::{FunctionOutput, FunctionOutputPart};

use super::Tool;

#[derive(Deserialize)]
struct ReadImageArgs {
    path: PathBuf,
}

pub struct ReadImage;

impl Tool for ReadImage {
    fn name(&self) -> &str {
        "read_image"
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "strict": false,
            "name": self.name(),
            "description": "Read a local PNG, JPEG, WebP, or GIF file and return it as an image attachment (up to 20 MiB).",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative or absolute image file path" }
                },
                "required": ["path"]
            }
        })
    }

    fn call(&mut self, args: &str) -> anyhow::Result<FunctionOutput> {
        const MAX_BYTES: u64 = 20 * 1024 * 1024;
        let args: ReadImageArgs = serde_json::from_str(args)?;
        let metadata = fs::metadata(&args.path)?;
        anyhow::ensure!(metadata.is_file(), "path must point to a regular file");
        anyhow::ensure!(metadata.len() <= MAX_BYTES, "image exceeds 20 MiB");
        let mut bytes = Vec::new();
        fs::File::open(args.path)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(bytes.len() as u64 <= MAX_BYTES, "image exceeds 20 MiB");
        let mime = match bytes.as_slice() {
            [137, b'P', b'N', b'G', 13, 10, 26, 10, ..] => "image/png",
            [255, 216, 255, ..] => "image/jpeg",
            [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => "image/gif",
            [
                b'R',
                b'I',
                b'F',
                b'F',
                _,
                _,
                _,
                _,
                b'W',
                b'E',
                b'B',
                b'P',
                ..,
            ] => "image/webp",
            _ => anyhow::bail!("unsupported image format; expected PNG, JPEG, WebP, or GIF"),
        };
        Ok(FunctionOutput::Parts(vec![
            FunctionOutputPart::InputImage {
                image_url: format!("data:{mime};base64,{}", STANDARD.encode(&bytes)),
                detail: "auto".into(),
            },
        ]))
    }
}
