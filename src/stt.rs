use std::{process::Stdio, time::Duration};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, header},
    routing::post,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use crate::cmd::SttModel;

const MAX_AUDIO_BYTES: usize = 25_000_000;
const MAX_BODY_BYTES: usize = MAX_AUDIO_BYTES.div_ceil(3) * 4 + 8192;
const TRANSCRIPTION_PATH: &str = "/audio/transcriptions";

type RequestError = (StatusCode, &'static str);

#[derive(Clone)]
struct Gateway {
    client: reqwest::Client,
    url: reqwest::Url,
    model: &'static str,
    provider: &'static str,
}

#[derive(Serialize)]
struct TranscriptionRequest {
    model: &'static str,
    input_audio: Audio,
    provider: Provider,
}

#[derive(Deserialize)]
struct AudioUpload {
    input_audio: Audio,
}

#[derive(Deserialize, Serialize)]
struct Audio {
    data: String,
    format: String,
}

#[derive(Serialize)]
struct Provider {
    only: [&'static str; 1],
}

#[derive(Deserialize, Serialize)]
struct Transcript {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<Usage>,
}

#[derive(Deserialize, Serialize)]
struct Usage {
    #[serde(skip_serializing_if = "Option::is_none")]
    seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_tokens: Option<u64>,
}

pub fn serve(host: &str, port: u16, model: SttModel, base_url: &str) -> anyhow::Result<()> {
    let (model, provider) = match model {
        SttModel::Mai => ("microsoft/mai-transcribe-2", "azure"),
    };

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let gateway = Gateway {
                client: reqwest::Client::builder()
                    .timeout(Duration::from_secs(75))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()?,
                url: format!("{}{TRANSCRIPTION_PATH}", base_url.trim_end_matches('/')).parse()?,
                model,
                provider,
            };
            let router = Router::new()
                .route(TRANSCRIPTION_PATH, post(transcribe))
                .layer(DefaultBodyLimit::max(MAX_BODY_BYTES));
            let listener = tokio::net::TcpListener::bind((host, port)).await?;
            eprintln!(
                "STT server listening on http://{}{TRANSCRIPTION_PATH} | model: {model} | provider: {provider}",
                listener.local_addr()?
            );

            axum::serve(listener, router.with_state(gateway))
                .with_graceful_shutdown(async {
                    tokio::signal::ctrl_c().await.ok();
                })
                .await?;
            Ok(())
        })
}

async fn transcribe(
    State(gateway): State<Gateway>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Transcript>, RequestError> {
    if headers
        .get("sec-fetch-site")
        .is_some_and(|site| site == "cross-site")
    {
        return Err((StatusCode::FORBIDDEN, "cross-site uploads are not allowed"));
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let media_type = content_type.split(';').next().unwrap_or_default().trim();

    let decoded;
    let (audio, mut format) = if media_type.eq_ignore_ascii_case("application/json") {
        let AudioUpload { input_audio } = serde_json::from_slice(&body).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "expected input_audio with data and format",
            )
        })?;
        let format = input_audio.format.to_ascii_lowercase();
        if !matches!(format.as_str(), "wav" | "mp3" | "m4a" | "flac" | "webm") {
            return Err((
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "expected WAV, MP3, M4A, FLAC, or WebM audio",
            ));
        }
        decoded = STANDARD.decode(input_audio.data).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "input_audio.data must be base64 audio",
            )
        })?;
        (decoded.as_slice(), format)
    } else if media_type.eq_ignore_ascii_case("multipart/form-data") {
        let (audio, format) = audio_upload(content_type, &body)?;
        (audio, format.to_owned())
    } else {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "expected application/json or multipart/form-data",
        ));
    };
    if audio.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "audio is empty"));
    }
    if audio.len() > MAX_AUDIO_BYTES {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, "audio exceeds 25 MB"));
    }

    let converted;
    let audio = if matches!(format.as_str(), "m4a" | "webm") {
        converted = convert_audio(audio, &format).await?;
        format = "flac".to_owned();
        converted.as_slice()
    } else {
        audio
    };

    let payload = TranscriptionRequest {
        model: gateway.model,
        input_audio: Audio {
            data: STANDARD.encode(audio),
            format,
        },
        provider: Provider {
            only: [gateway.provider],
        },
    };
    let response = gateway
        .client
        .post(gateway.url)
        .json(&payload)
        .send()
        .await
        .map_err(|err| {
            eprintln!("STT gateway request failed: {}", err.without_url());
            (
                StatusCode::BAD_GATEWAY,
                "transcription gateway request failed",
            )
        })?;
    let status = response.status();
    if !status.is_success() {
        let status = if status.is_client_error() || status.is_server_error() {
            status
        } else {
            StatusCode::BAD_GATEWAY
        };
        return Err((status, "transcription gateway rejected the request"));
    }

    response.json().await.map(Json).map_err(|_| {
        (
            StatusCode::BAD_GATEWAY,
            "transcription gateway returned an invalid response",
        )
    })
}

fn audio_upload<'a>(
    content_type: &str,
    body: &'a [u8],
) -> Result<(&'a [u8], &'static str), RequestError> {
    let invalid = (
        StatusCode::BAD_REQUEST,
        "expected one multipart file field named 'file'",
    );
    let boundary = content_type
        .split(';')
        .skip(1)
        .filter_map(|param| param.split_once('='))
        .find_map(|(key, value)| {
            key.trim()
                .eq_ignore_ascii_case("boundary")
                .then(|| value.trim().trim_matches('"'))
        })
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 70
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"'()+_,-./:=? ".contains(&byte))
        })
        .ok_or(invalid)?;
    let delimiter = format!("\r\n--{boundary}");
    let body = body
        .strip_prefix(&delimiter.as_bytes()[2..])
        .and_then(|body| body.strip_prefix(b"\r\n"))
        .ok_or(invalid)?;
    let end = format!("{delimiter}--");
    let body = body
        .strip_suffix(b"\r\n")
        .unwrap_or(body)
        .strip_suffix(end.as_bytes())
        .ok_or(invalid)?;
    let split = body[..body.len().min(8192)]
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(invalid)?;
    let (part_headers, audio) = (&body[..split], &body[split + 4..]);
    if audio
        .windows(delimiter.len())
        .any(|window| window == delimiter.as_bytes())
    {
        return Err(invalid);
    }

    let mut filename = None;
    let mut mime = "";
    for (key, value) in std::str::from_utf8(part_headers)
        .map_err(|_| invalid)?
        .lines()
        .filter_map(|line| line.split_once(':'))
    {
        if key.eq_ignore_ascii_case("content-disposition") {
            let mut params = value.split(';');
            if !params
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case("form-data")
            {
                return Err(invalid);
            }
            let mut name = None;
            for (key, value) in params.filter_map(|param| param.split_once('=')) {
                let value = value.trim().trim_matches('"');
                match key.trim() {
                    "name" => name = Some(value),
                    "filename" => filename = Some(value),
                    _ => (),
                }
            }
            if name != Some("file") {
                return Err(invalid);
            }
        } else if key.eq_ignore_ascii_case("content-type") {
            mime = value.split(';').next().unwrap_or_default().trim();
        }
    }
    let filename = filename.filter(|value| !value.is_empty()).ok_or(invalid)?;
    let extension = filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase());
    let format = match (extension.as_deref(), mime.to_ascii_lowercase().as_str()) {
        (Some("wav"), _) => "wav",
        (Some("mp3"), _) => "mp3",
        (Some("m4a"), _) => "m4a",
        (Some("flac"), _) => "flac",
        (Some("webm"), _) => "webm",
        (_, "audio/wav" | "audio/x-wav") => "wav",
        (_, "audio/mpeg" | "audio/mp3") => "mp3",
        (_, "audio/mp4" | "audio/x-m4a") => "m4a",
        (_, "audio/flac" | "audio/x-flac") => "flac",
        (_, "audio/webm" | "video/webm") => "webm",
        _ => {
            return Err((
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "expected WAV, MP3, M4A, FLAC, or WebM audio",
            ));
        }
    };
    Ok((audio, format))
}

async fn convert_audio(audio: &[u8], format: &str) -> Result<Vec<u8>, RequestError> {
    let temp_error = |_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not stage audio for conversion",
        )
    };
    let input = tempfile::NamedTempFile::new().map_err(temp_error)?;
    tokio::fs::write(input.path(), audio)
        .await
        .map_err(temp_error)?;

    let mut command = tokio::process::Command::new("ffmpeg");
    command.args([
        "-loglevel",
        "error",
        "-protocol_whitelist",
        "file,pipe",
        "-f",
    ]);
    if format == "m4a" {
        command.args(["mov", "-enable_drefs", "0"]);
    } else {
        command.arg("matroska");
    }

    let mut child = command
        .arg("-i")
        .arg(input.path())
        .args([
            "-map", "0:a:0", "-ac", "1", "-ar", "16000", "-f", "flac", "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| {
            eprintln!("could not start ffmpeg: {err}");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "M4A and WebM uploads require ffmpeg",
            )
        })?;
    let mut stdout = child
        .stdout
        .take()
        .unwrap()
        .take((MAX_AUDIO_BYTES + 1) as u64);
    let mut flac = Vec::new();

    let status = tokio::time::timeout(Duration::from_secs(60), async {
        stdout.read_to_end(&mut flac).await?;
        drop(stdout);
        child.wait().await
    })
    .await
    .map_err(|_| (StatusCode::REQUEST_TIMEOUT, "audio conversion timed out"))?;
    if flac.len() > MAX_AUDIO_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "converted audio exceeds 25 MB",
        ));
    }
    if !status.is_ok_and(|status| status.success()) || flac.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "could not convert audio to FLAC",
        ));
    }
    Ok(flac)
}
