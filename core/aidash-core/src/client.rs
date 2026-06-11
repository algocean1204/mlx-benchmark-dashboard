//! Python 서버 HTTP 클라이언트 (reqwest): OpenAI 호환 호출 + 스트리밍 토큰 타임스탬프 기록,
//! /health, /metrics
//!
//! IN: 요청(채팅/이미지/오디오), 서버 포트
//! OUT: 스트리밍 토큰 이벤트, 완료 응답, 토큰별 타임스탬프

use std::path::Path;
use std::time::Instant;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::events::CoreEvent;

#[derive(Debug, Clone, Serialize)]
pub struct StreamStats {
    pub ttft_ms: f64,
    pub prefill_tps: f64,
    /// `None` when decode duration cannot be measured (tokens_out < 2 or first==last token time).
    pub decode_tps: Option<f64>,
    pub total_tps: f64,
    pub tokens_in: u32,
    pub tokens_out: u32,
}

#[derive(Debug, Clone)]
pub struct TokenTimestamp {
    pub token_index: u32,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MlxMetrics {
    pub mlx_active_bytes: u64,
    pub mlx_peak_bytes: u64,
    pub mlx_cache_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
    usage: Option<StreamUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
}

pub async fn fetch_metrics(client: &Client, port: u16) -> Result<MlxMetrics, String> {
    let url = format!("http://127.0.0.1:{port}/metrics");
    client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<MlxMetrics>()
        .await
        .map_err(|e| e.to_string())
}

fn image_data_url(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/png",
    };
    Ok(format!("data:{mime};base64,{}", BASE64.encode(bytes)))
}

fn chat_messages_json(prompt: &str, image_path: Option<&Path>) -> Result<serde_json::Value, String> {
    if let Some(path) = image_path {
        let data_url = image_data_url(path)?;
        Ok(serde_json::json!({
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {"type": "image_url", "image_url": {"url": data_url}},
            ],
        }))
    } else {
        Ok(serde_json::json!({
            "role": "user",
            "content": prompt,
        }))
    }
}

pub async fn stream_chat_completion(
    client: &Client,
    port: u16,
    model: &str,
    prompt: &str,
    max_tokens: u32,
    event_tx: Option<broadcast::Sender<CoreEvent>>,
) -> Result<(String, StreamStats), String> {
    stream_chat_completion_with_image(client, port, model, prompt, None, max_tokens, event_tx).await
}

pub async fn stream_chat_completion_with_image(
    client: &Client,
    port: u16,
    model: &str,
    prompt: &str,
    image_path: Option<&Path>,
    max_tokens: u32,
    event_tx: Option<broadcast::Sender<CoreEvent>>,
) -> Result<(String, StreamStats), String> {
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let message = chat_messages_json(prompt, image_path)?;
    let body = serde_json::json!({
        "model": model,
        "messages": [message],
        "stream": true,
        "max_tokens": max_tokens,
    });

    let request_sent = Instant::now();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("chat request failed ({status}): {text}"));
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut full_text = String::new();
    let mut token_index: u32 = 0;
    let mut first_token_at: Option<Instant> = None;
    let mut last_token_at: Option<Instant> = None;
    let mut usage_prompt: Option<u32> = None;
    let mut usage_completion: Option<u32> = None;
    let mut done = false;

    while !done {
        let chunk = match stream.next().await {
            Some(Ok(bytes)) => bytes,
            Some(Err(e)) => return Err(e.to_string()),
            None => break,
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim_end_matches('\r').to_string();
            buffer.drain(..=pos);

            if line.is_empty() {
                continue;
            }
            if !line.starts_with("data: ") {
                continue;
            }
            let data = line["data: ".len()..].trim();
            if data == "[DONE]" {
                done = true;
                break;
            }

            let parsed: StreamChunk = serde_json::from_str(data).map_err(|e| {
                format!("invalid stream chunk: {e} (data={data})")
            })?;

            if let Some(usage) = parsed.usage {
                usage_prompt = usage.prompt_tokens.or(usage_prompt);
                usage_completion = usage.completion_tokens.or(usage_completion);
            }

            for choice in parsed.choices {
                if let Some(content) = choice.delta.content {
                    if !content.is_empty() {
                        let now = Instant::now();
                        if first_token_at.is_none() {
                            first_token_at = Some(now);
                        }
                        last_token_at = Some(now);

                        if let Some(tx) = &event_tx {
                            let _ = tx.send(CoreEvent::Token {
                                index: token_index,
                                text: content.clone(),
                            });
                        }
                        full_text.push_str(&content);
                        token_index = token_index.saturating_add(1);
                    }
                }
            }
        }
    }

    let first = first_token_at.ok_or_else(|| "no tokens received".to_string())?;
    let last = last_token_at.unwrap_or(first);
    let total_done = Instant::now();

    let tokens_out = usage_completion.unwrap_or(token_index);
    let tokens_in = usage_prompt.unwrap_or(1).max(1);

    let ttft_ms = first.duration_since(request_sent).as_secs_f64() * 1000.0;
    let prefill_secs = first.duration_since(request_sent).as_secs_f64();
    let prefill_tps = if prefill_secs > 0.0 {
        tokens_in as f64 / prefill_secs
    } else {
        0.0
    };

    let decode_secs = last.duration_since(first).as_secs_f64();
    let decode_tps = if tokens_out >= 2 && decode_secs > 0.0 {
        Some((tokens_out - 1) as f64 / decode_secs)
    } else {
        None
    };

    let total_secs = total_done.duration_since(request_sent).as_secs_f64();
    let total_tps = if total_secs > 0.0 && tokens_out > 0 {
        tokens_out as f64 / total_secs
    } else {
        0.0
    };

    Ok((
        full_text,
        StreamStats {
            ttft_ms,
            prefill_tps,
            decode_tps,
            total_tps,
            tokens_in,
            tokens_out,
        },
    ))
}

#[derive(Debug, Clone, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

pub async fn transcribe_audio(
    client: &Client,
    port: u16,
    audio_path: &Path,
) -> Result<(String, f64), String> {
    let url = format!("http://127.0.0.1:{port}/v1/audio/transcriptions");
    let bytes = tokio::fs::read(audio_path)
        .await
        .map_err(|e| e.to_string())?;
    let file_name = audio_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio.wav")
        .to_string();

    let request_sent = Instant::now();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(bytes)
            .file_name(file_name)
            .mime_str("audio/wav")
            .map_err(|e| e.to_string())?,
    );

    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("transcription request failed ({status}): {text}"));
    }

    let parsed: TranscriptionResponse = response.json().await.map_err(|e| e.to_string())?;
    let elapsed_ms = request_sent.elapsed().as_secs_f64() * 1000.0;
    Ok((parsed.text, elapsed_ms))
}

pub async fn speech_audio(
    client: &Client,
    port: u16,
    input: &str,
    voice: Option<&str>,
) -> Result<f64, String> {
    let url = format!("http://127.0.0.1:{port}/v1/audio/speech");
    let mut body = serde_json::json!({"input": input});
    if let Some(v) = voice {
        body["voice"] = serde_json::Value::String(v.into());
    }

    let request_sent = Instant::now();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("speech request failed ({status}): {text}"));
    }

    let _audio = response.bytes().await.map_err(|e| e.to_string())?;
    Ok(request_sent.elapsed().as_secs_f64() * 1000.0)
}

#[derive(Debug, Clone, Deserialize)]
struct ImageGenerationResponse {
    data: Vec<ImageGenerationData>,
}

#[derive(Debug, Clone, Deserialize)]
struct ImageGenerationData {
    b64_json: String,
}

pub async fn generate_image(
    client: &Client,
    port: u16,
    prompt: &str,
) -> Result<f64, String> {
    let url = format!("http://127.0.0.1:{port}/v1/images/generations");
    let body = serde_json::json!({
        "prompt": prompt,
        "n": 1,
        "size": "512x512",
    });

    let request_sent = Instant::now();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("image generation request failed ({status}): {text}"));
    }

    let parsed: ImageGenerationResponse = response.json().await.map_err(|e| e.to_string())?;
    if parsed.data.is_empty() {
        return Err("image generation returned no data".into());
    }
    Ok(request_sent.elapsed().as_secs_f64() * 1000.0)
}

pub fn timing_only_stats(ttft_ms: f64) -> StreamStats {
    StreamStats {
        ttft_ms,
        prefill_tps: 0.0,
        decode_tps: None,
        total_tps: 0.0,
        tokens_in: 0,
        tokens_out: 0,
    }
}

pub fn is_token_benchmark(stats: &StreamStats) -> bool {
    stats.tokens_out > 0 || stats.prefill_tps > 0.0 || stats.decode_tps.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_tps_none_for_single_token() {
        let stats = StreamStats {
            ttft_ms: 100.0,
            prefill_tps: 50.0,
            decode_tps: None,
            total_tps: 10.0,
            tokens_in: 10,
            tokens_out: 1,
        };
        assert!(stats.decode_tps.is_none());
        assert!(is_token_benchmark(&stats));
    }
}

/// Non-streaming chat completion (temperature 0) for eval.
pub async fn chat_completion(
    client: &Client,
    port: u16,
    model: &str,
    prompt: &str,
    max_tokens: u32,
    temperature: f64,
) -> Result<String, String> {
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": false,
        "max_tokens": max_tokens,
        "temperature": temperature,
    });

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("chat request failed ({status}): {text}"));
    }

    #[derive(Deserialize)]
    struct ChatResponse {
        choices: Vec<ChatChoice>,
    }
    #[derive(Deserialize)]
    struct ChatChoice {
        message: ChatMessage,
    }
    #[derive(Deserialize)]
    struct ChatMessage {
        content: String,
    }

    let parsed: ChatResponse = response.json().await.map_err(|e| e.to_string())?;
    parsed
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| "empty chat response".into())
}