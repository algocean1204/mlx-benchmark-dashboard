//! 채팅 세션 압축 트리거·히스토리 구성

/// 최근 N턴(사용자+어시스턴트 쌍)은 압축에서 제외한다.
pub const COMPRESS_KEEP_RECENT_TURNS: usize = 4;

/// 컨텍스트 사용량이 이 비율을 넘으면 압축을 트리거한다.
pub const COMPRESS_THRESHOLD_RATIO: f64 = 0.7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

/// prompt_tokens가 컨텍스트 한도의 70%를 넘으면 압축이 필요하다.
pub fn should_compress(prompt_tokens: u32, context_size: u32) -> bool {
    if context_size == 0 {
        return false;
    }
    prompt_tokens as f64 > context_size as f64 * COMPRESS_THRESHOLD_RATIO
}

/// 압축 대상(오래된) 메시지와 유지할 최근 메시지를 분리한다.
pub fn split_for_compression(
    messages: &[ChatTurn],
    keep_recent_turns: usize,
) -> (Vec<ChatTurn>, Vec<ChatTurn>) {
    let keep_messages = keep_recent_turns.saturating_mul(2);
    if messages.len() <= keep_messages {
        return (Vec::new(), messages.to_vec());
    }
    let split_at = messages.len() - keep_messages;
    (
        messages[..split_at].to_vec(),
        messages[split_at..].to_vec(),
    )
}

/// 요약 프롬프트 본문을 생성한다.
pub fn build_summary_prompt(old_messages: &[ChatTurn]) -> String {
    let mut body = String::from(
        "다음 대화를 핵심 사실·결정·맥락 중심으로 한국어로 요약하세요. \
         불필요한 인사말은 생략하고, 이후 대화에서 참고할 수 있게 간결하게 정리하세요.\n\n",
    );
    for msg in old_messages {
        let role = match msg.role.as_str() {
            "user" => "사용자",
            "assistant" => "어시스턴트",
            "system" => "시스템",
            other => other,
        };
        body.push_str(&format!("{role}: {}\n", msg.content));
    }
    body
}

/// 압축 후 전송용 히스토리: [요약 assistant 블록] + 최근 턴.
pub fn compressed_history(summary: &str, recent: &[ChatTurn]) -> Vec<ChatTurn> {
    let mut out = vec![ChatTurn {
        role: "assistant".into(),
        content: format!("[이전 대화 요약]\n{summary}"),
    }];
    out.extend(recent.iter().cloned());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatTurn {
        ChatTurn {
            role: role.into(),
            content: content.into(),
        }
    }

    #[test]
    fn should_compress_at_seventy_percent() {
        assert!(!should_compress(2800, 4096));
        assert!(should_compress(2868, 4096));
        assert!(!should_compress(0, 4096));
        assert!(!should_compress(100, 0));
    }

    #[test]
    fn split_keeps_recent_turns() {
        let messages: Vec<_> = (0..10)
            .map(|i| msg(if i % 2 == 0 { "user" } else { "assistant" }, &format!("m{i}")))
            .collect();
        let (old, recent) = split_for_compression(&messages, 2);
        assert_eq!(old.len(), 6);
        assert_eq!(recent.len(), 4);
        assert_eq!(recent[0].content, "m6");
    }

    #[test]
    fn compressed_history_prepends_summary() {
        let recent = vec![msg("user", "hi"), msg("assistant", "hello")];
        let hist = compressed_history("요약본", &recent);
        assert_eq!(hist.len(), 3);
        assert_eq!(hist[0].role, "assistant");
        assert!(hist[0].content.contains("요약본"));
        assert_eq!(hist[1].content, "hi");
    }

    #[test]
    fn build_summary_prompt_includes_roles() {
        let prompt = build_summary_prompt(&[
            msg("user", "내 이름은 테스터"),
            msg("assistant", "안녕하세요 테스터님"),
        ]);
        assert!(prompt.contains("사용자: 내 이름은 테스터"));
        assert!(prompt.contains("어시스턴트:"));
    }
}