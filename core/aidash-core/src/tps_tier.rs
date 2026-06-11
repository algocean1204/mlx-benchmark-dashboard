//! TPS 등급 판정 — 모든 표면이 공유하는 단일 구현.
//! 디퓨전 모델은 블록 burst로 decode TPS가 왜곡되므로 total TPS로 등급·표시한다.

use crate::profile::is_diffusion_kind;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TpsTier {
    Unusable,
    Sluggish,
    Ideal,
    Fast,
    Realtime,
}

#[derive(Debug, Clone, Serialize)]
pub struct TierInfo {
    pub badge: &'static str,
    pub label: &'static str,
    pub key: &'static str,
}

pub fn tps_tier(decode_tps: f64) -> TpsTier {
    if decode_tps < 10.0 {
        TpsTier::Unusable
    } else if decode_tps < 40.0 {
        TpsTier::Sluggish
    } else if decode_tps < 60.0 {
        TpsTier::Ideal
    } else if decode_tps < 100.0 {
        TpsTier::Fast
    } else {
        TpsTier::Realtime
    }
}

impl TpsTier {
    pub fn info(self) -> TierInfo {
        match self {
            TpsTier::Unusable => TierInfo {
                badge: "🔴",
                label: "사용 불가",
                key: "unusable",
            },
            TpsTier::Sluggish => TierInfo {
                badge: "🟠",
                label: "답답함",
                key: "sluggish",
            },
            TpsTier::Ideal => TierInfo {
                badge: "🟢",
                label: "이상적",
                key: "ideal",
            },
            TpsTier::Fast => TierInfo {
                badge: "🔵",
                label: "빠름",
                key: "fast",
            },
            TpsTier::Realtime => TierInfo {
                badge: "🟣",
                label: "실시간급",
                key: "realtime",
            },
        }
    }

    pub fn display(self) -> String {
        let info = self.info();
        format!("{} {}", info.badge, info.label)
    }

    pub fn json_value(self) -> serde_json::Value {
        let info = self.info();
        serde_json::json!({
            "badge": info.badge,
            "label": info.label,
            "key": info.key,
        })
    }
}

pub fn format_decode_tps(decode_tps: f64) -> String {
    format!("{:.1} {}", decode_tps, tps_tier(decode_tps).display())
}

pub fn format_decode_tps_opt(decode_tps: Option<f64>) -> String {
    decode_tps
        .map(format_decode_tps)
        .unwrap_or_else(|| "-".into())
}

/// 디퓨전 모델은 total_tps, 그 외는 decode_tps를 표시·등급에 사용한다.
pub fn effective_display_tps(
    generation_kind: &str,
    decode_tps: Option<f64>,
    total_tps: Option<f64>,
) -> Option<f64> {
    if is_diffusion_kind(generation_kind) {
        total_tps.filter(|t| t.is_finite() && *t > 0.0)
    } else {
        decode_tps
    }
}

pub fn tier_for_run(
    generation_kind: &str,
    decode_tps: Option<f64>,
    total_tps: Option<f64>,
) -> Option<TpsTier> {
    effective_display_tps(generation_kind, decode_tps, total_tps).map(tps_tier)
}

pub fn tier_display_suffix(generation_kind: &str) -> &'static str {
    if is_diffusion_kind(generation_kind) {
        " (디퓨전: 전체 처리율 기준)"
    } else {
        ""
    }
}

pub fn format_display_tps_opt(
    generation_kind: &str,
    decode_tps: Option<f64>,
    total_tps: Option<f64>,
) -> String {
    match effective_display_tps(generation_kind, decode_tps, total_tps) {
        Some(tps) => {
            let suffix = tier_display_suffix(generation_kind);
            format!(
                "{:.1} {}{}",
                tps,
                tps_tier(tps).display(),
                suffix
            )
        }
        None => "-".into(),
    }
}

pub fn format_processing_time_ms(ttft_ms: Option<f64>) -> String {
    ttft_ms
        .map(|ms| format!("처리시간 {:.0} ms", ms))
        .unwrap_or_else(|| "-".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_boundaries() {
        assert_eq!(tps_tier(9.99), TpsTier::Unusable);
        assert_eq!(tps_tier(10.0), TpsTier::Sluggish);
        assert_eq!(tps_tier(39.99), TpsTier::Sluggish);
        assert_eq!(tps_tier(40.0), TpsTier::Ideal);
        assert_eq!(tps_tier(59.99), TpsTier::Ideal);
        assert_eq!(tps_tier(60.0), TpsTier::Fast);
        assert_eq!(tps_tier(99.99), TpsTier::Fast);
        assert_eq!(tps_tier(100.0), TpsTier::Realtime);
    }

    #[test]
    fn diffusion_uses_total_tps_for_tier_not_decode() {
        use crate::profile::{GENERATION_KIND_AUTOREGRESSIVE, GENERATION_KIND_DIFFUSION};

        assert_eq!(
            tier_for_run(GENERATION_KIND_DIFFUSION, Some(11_325.49), Some(4.72)),
            Some(TpsTier::Unusable)
        );
        assert_eq!(
            effective_display_tps(GENERATION_KIND_DIFFUSION, Some(11_325.49), Some(4.72)),
            Some(4.72)
        );
        assert_eq!(
            tier_for_run(GENERATION_KIND_AUTOREGRESSIVE, Some(52.0), Some(40.0)),
            Some(TpsTier::Ideal)
        );
        assert_eq!(
            effective_display_tps(GENERATION_KIND_AUTOREGRESSIVE, Some(52.0), Some(40.0)),
            Some(52.0)
        );
    }

    #[test]
    fn diffusion_tier_suffix() {
        use crate::profile::{GENERATION_KIND_AUTOREGRESSIVE, GENERATION_KIND_DIFFUSION};

        assert!(tier_display_suffix(GENERATION_KIND_DIFFUSION).contains("디퓨전"));
        assert_eq!(tier_display_suffix(GENERATION_KIND_AUTOREGRESSIVE), "");
    }
}