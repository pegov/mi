use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use base64::{Engine, engine::general_purpose};

pub fn generate_image(
    gateway: &str,
    model: String,
    prompt: String,
    resolution: String,
    aspect_ratio: String,
    reference_paths: &[String],
) -> anyhow::Result<()> {
    let out_dir = PathBuf::from("./output");
    std::fs::create_dir_all(&out_dir)?;

    let client = reqwest::blocking::ClientBuilder::default()
        .timeout(Duration::from_secs(600))
        .build()?;

    let mut references = Vec::with_capacity(reference_paths.len());
    for path_str in reference_paths {
        let path = Path::new(path_str);
        let bytes = fs::read(path_str)?;

        let mime = match path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .as_deref()
        {
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("webp") => "image/webp",
            Some("gif") => "image/gif",
            Some("svg") => "image/svg+xml",
            _ => "application/octet-stream",
        };

        let b64 = general_purpose::STANDARD.encode(&bytes);
        let data_url = format!("data:{};base64,{}", mime, b64);

        references.push(serde_json::json!(
        {
            "type": "image_url",
            "image_url": { "url": data_url }
        }
        ));
    }

    let mut payload = serde_json::json!(
        {
            "model": model,
            "prompt": prompt,
            "resolution": resolution,
            "aspect_ratio": aspect_ratio,
        }
    );

    if !references.is_empty() {
        payload["input_references"] = serde_json::Value::Array(references);
    }

    let response = client.post(gateway).json(&payload).send()?;
    let status = response.status();
    let body_text = response.text()?;

    if !status.is_success() {
        anyhow::bail!("API {} error: {}", status, body_text);
    }

    let body: serde_json::Value = serde_json::from_str(&body_text)
        .map_err(|e| anyhow::anyhow!("invalid JSON: {} — body: {}", e, body_text))?;
    let images = body["data"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing data array in response"))?;

    if images.is_empty() {
        anyhow::bail!("no images returned by the API");
    }

    for (i, item) in images.iter().enumerate() {
        let b64 = item["b64_json"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing b64_json in data[{}]", i))?;

        let media_type = item["media_type"].as_str().unwrap_or("image/jpeg");
        let ext = match media_type {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            "image/gif" => "gif",
            _ => "bin",
        };

        let stem = if images.len() == 1 {
            "generated".to_string()
        } else {
            format!("generated_{:02}", i + 1)
        };

        let out_path = unique_path(&out_dir, &stem, ext);

        let bytes = general_purpose::STANDARD.decode(b64)?;

        fs::write(&out_path, &bytes)?;

        println!(
            "OK [{}] {} ({} bytes, {})",
            i + 1,
            out_path.display(),
            bytes.len(),
            media_type,
        );
    }

    if let Some(cost) = body["usage"]["cost"].as_f64() {
        println!("   cost: ${:.4}", cost);
    }

    Ok(())
}

fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let candidate = dir.join(format!("{}.{}", stem, ext));
    if !candidate.exists() {
        return candidate;
    }
    for n in 1u32.. {
        let candidate = dir.join(format!("{}_{}.{}", stem, n, ext));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}
