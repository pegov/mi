use std::{fs, io::Read, path::PathBuf};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;

use crate::{
    image::{MAX_IMAGE_BYTES, MAX_IMAGE_MIB, mime_type},
    responses::{FunctionOutput, FunctionOutputPart},
};

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
            "description": format!("Read a local PNG, JPEG, WebP, or GIF file and return it as an image attachment (up to {MAX_IMAGE_MIB} MiB)."),
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
        let args: ReadImageArgs = serde_json::from_str(args)?;
        let metadata = fs::metadata(&args.path)?;
        anyhow::ensure!(metadata.is_file(), "path must point to a regular file");
        anyhow::ensure!(
            metadata.len() <= MAX_IMAGE_BYTES as u64,
            "image exceeds {MAX_IMAGE_MIB} MiB"
        );
        let mut bytes = Vec::new();
        fs::File::open(args.path)?
            .take(MAX_IMAGE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() <= MAX_IMAGE_BYTES,
            "image exceeds {MAX_IMAGE_MIB} MiB"
        );
        let mime = mime_type(&bytes)?;
        Ok(FunctionOutput::Parts(vec![
            FunctionOutputPart::InputImage {
                image_url: format!("data:{mime};base64,{}", STANDARD.encode(&bytes)),
                detail: "auto".into(),
            },
        ]))
    }
}
