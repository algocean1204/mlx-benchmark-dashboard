use aidash_core::pyproc::LogStream;

fn parse(stream: LogStream, line: &str) -> Option<(String, String)> {
    if line.contains("%|") || line.contains('\r') {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
        if let Some(level) = value.get("level").and_then(|v| v.as_str()) {
            let message = value
                .get("message")
                .and_then(|v| v.as_str())
                .or_else(|| value.get("event").and_then(|v| v.as_str()))
                .unwrap_or(line)
                .to_string();
            return Some((level.to_string(), message));
        }
    }
    let level = match stream {
        LogStream::Stdout => "info",
        LogStream::Stderr => "info",
    };
    Some((level.into(), line.to_string()))
}

#[test]
fn stderr_plain_text_defaults_to_info() {
    let parsed = parse(LogStream::Stderr, "loading weights").expect("parsed");
    assert_eq!(parsed.0, "info");
}

#[test]
fn json_line_uses_embedded_level() {
    let line = r#"{"event":"log","level":"warning","message":"retrying"}"#;
    let parsed = parse(LogStream::Stderr, line).expect("parsed");
    assert_eq!(parsed.0, "warning");
    assert_eq!(parsed.1, "retrying");
}

#[test]
fn tqdm_lines_are_filtered() {
    assert!(parse(LogStream::Stderr, " 12%|██| 3/25").is_none());
}