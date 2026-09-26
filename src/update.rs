//! 可选的更新检查（默认关闭）：开启后每 24 小时至多查询一次 GitHub Releases API。
//!
//! 隐私约定：只读 `releases/latest` 一个端点、带固定 User-Agent、10 秒超时、
//! 不下载任何文件、不上报任何本机信息。结果只显示在设置窗里。
//!
//! **网络栈随 cargo feature `update-check` 一起编译（v0.6 起默认关）**：
//! `ureq` 的默认 features 会把 `rustls` + `ring`（大块汇编加密代码）无条件拖进
//! 二进制，而本功能默认关闭。实测省下 1,168,896 B（10,224,640 → 9,055,744，-11.4%）。
//! 需要在线检查的构建：`cargo build --release --features update-check`。

use std::time::Duration;

#[cfg(feature = "update-check")]
use serde_json::Value;

use crate::config::Config;

pub const RELEASES_URL: &str = "https://github.com/lexingtonhibiki/eyeflow/releases";
#[cfg(feature = "update-check")]
const API_URL: &str = "https://api.github.com/repos/lexingtonhibiki/EyeFlow/releases/latest";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateInfo {
    pub latest: String,
    pub url: String,
    pub is_newer: bool,
}

/// 距上次检查是否已超过 24 小时（从未检查过视为已超过）。
pub fn is_due(cfg: &Config, now_unix: u64) -> bool {
    cfg.update_check_enabled
        && cfg
            .update_last_checked
            .is_none_or(|t| now_unix.saturating_sub(t) >= CHECK_INTERVAL.as_secs())
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 查询最新版本。网络与解析错误统一为中文 Err。
#[cfg(feature = "update-check")]
pub fn check() -> Result<UpdateInfo, String> {
    let text = ureq::get(API_URL)
        .header("User-Agent", concat!("eyeflow/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .config()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(404) => crate::tr::tr("update.not_released").to_string(),
            other => crate::tr::tr_fill("update.net_error", "{err}", other),
        })?
        .body_mut()
        .read_to_string()
        .map_err(|e| crate::tr::tr_fill("update.read_error", "{err}", e))?;
    let json: Value = serde_json::from_str(&text)
        .map_err(|e| crate::tr::tr_fill("update.parse_error", "{err}", e))?;
    if let Some(msg) = json.get("message").and_then(|m| m.as_str()) {
        return Err(crate::tr::tr_fill("update.api_error", "{msg}", msg));
    }
    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::tr::tr("update.no_tag").to_string())?;
    let latest = tag.trim_start_matches(['v', 'V']).to_string();
    let url = json
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or(RELEASES_URL)
        .to_string();
    let is_newer = version_gt(&latest, current_version());
    Ok(UpdateInfo {
        latest,
        url,
        is_newer,
    })
}

/// 未编译进 `update-check` feature 时的占位：调用点无需到处写 `cfg`，
/// 用户点“立即检查”会看到一句明确的中文原因而不是崩溃。
#[cfg(not(feature = "update-check"))]
pub fn check() -> Result<UpdateInfo, String> {
    Err(crate::tr::tr("update.not_included").to_string())
}

/// “x.y.z” 数字版本号的字典序比较（忽略前导 v、非数字段按 0 处理）。
/// 纯逻辑，两种构建下都保留——它的单测不依赖网络栈。
#[cfg_attr(not(feature = "update-check"), allow(dead_code))]
pub fn version_gt(a: &str, b: &str) -> bool {
    fn parse(v: &str) -> Vec<u64> {
        v.trim()
            .trim_start_matches(['v', 'V'])
            .split('.')
            .map(|p| p.trim().parse::<u64>().unwrap_or(0))
            .collect()
    }
    let (a, b) = (parse(a), parse(b));
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(version_gt("0.4.1", "0.4.0"));
        assert!(version_gt("1.0.0", "0.9.9"));
        assert!(version_gt("v0.5.0", "0.4.10")); // 数字比较，10 > 0
        assert!(!version_gt("0.4.0", "0.4.0"));
        assert!(!version_gt("0.3.9", "0.4.0"));
        assert!(!version_gt("0.4", "0.4.0")); // 补零后相等
    }

    #[test]
    fn is_due_requires_enabled_and_interval() {
        let mut cfg = Config::default();
        assert!(!is_due(&cfg, 1000), "默认关闭");
        cfg.update_check_enabled = true;
        assert!(is_due(&cfg, 1000), "从未检查过");
        cfg.update_last_checked = Some(1000);
        assert!(!is_due(&cfg, 1000 + 3600));
        assert!(is_due(&cfg, 1000 + 24 * 3600));
        cfg.update_check_enabled = false;
        assert!(!is_due(&cfg, 1000 + 48 * 3600));
    }
}
