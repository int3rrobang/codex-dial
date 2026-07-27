use crate::models::*;
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

#[derive(Debug)]
pub enum CodexError {
    CliNotFound,
    InvalidResponse,
    MainLimitMissing,
    TimedOut,
}

impl std::fmt::Display for CodexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CliNotFound => write!(
                f,
                "Codex CLI was not found. Install it, sign in, and try again."
            ),
            Self::InvalidResponse => write!(
                f,
                "Codex returned data this app could not read. Update Codex CLI and try again."
            ),
            Self::MainLimitMissing => write!(
                f,
                "Codex did not return a usable limit. Make sure Codex CLI is signed in."
            ),
            Self::TimedOut => write!(f, "Codex took too long to respond. Try refreshing again."),
        }
    }
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    #[allow(dead_code)]
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RateLimitsResult {
    #[serde(rename = "rateLimits")]
    rate_limits: RateLimitSnapshot,
    #[serde(rename = "rateLimitsByLimitId")]
    rate_limits_by_limit_id: Option<std::collections::HashMap<String, RateLimitSnapshot>>,
    #[serde(rename = "rateLimitResetCredits")]
    rate_limit_reset_credits: Option<ResetCredits>,
}

#[derive(Deserialize)]
struct ResetCredits {
    #[serde(rename = "availableCount")]
    available_count: i32,
    credits: Option<Vec<ResetCredit>>,
}

#[derive(Deserialize)]
struct ResetCredit {
    status: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
    title: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct RateLimitSnapshot {
    #[serde(rename = "limitId")]
    #[allow(dead_code)]
    limit_id: Option<String>,
    #[serde(rename = "limitName")]
    limit_name: Option<String>,
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
}

#[derive(Deserialize)]
struct RateLimitWindow {
    #[serde(rename = "usedPercent")]
    used_percent: f64,
    #[serde(rename = "windowDurationMins")]
    window_duration_mins: Option<i64>,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
}

#[derive(Deserialize)]
struct UsageResult {
    #[serde(rename = "dailyUsageBuckets")]
    daily_usage_buckets: Option<Vec<TokenBucket>>,
}

#[derive(Deserialize)]
struct TokenBucket {
    #[serde(rename = "startDate")]
    start_date: String,
    tokens: i64,
}

fn find_codex_executable() -> Option<String> {
    let names = ["codex.cmd", "codex.exe"];
    let dirs: Vec<Option<std::path::PathBuf>> = vec![
        dirs::home_dir().map(|h| h.join(".codex").join("bin")),
        dirs::data_local_dir().map(|d| d.join("Programs").join("codex")),
        Some(std::path::PathBuf::from("C:\\Program Files\\codex")),
        dirs::config_dir().map(|c| c.join("npm")),
    ];

    for dir in dirs.iter().flatten() {
        for name in &names {
            let candidate = dir.join(name);
            if candidate.exists() {
                return candidate.to_str().map(|s| s.to_string());
            }
        }
    }

    // Try PATH via where.exe, prefer .cmd/.exe over extensionless shims
    if let Ok(output) = std::process::Command::new("where.exe")
        .arg("codex")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = path
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect();
            // Prefer .cmd or .exe entries
            if let Some(found) = lines
                .iter()
                .find(|l| l.ends_with(".cmd") || l.ends_with(".exe"))
            {
                return Some(found.to_string());
            }
            if let Some(first) = lines.first() {
                return Some(first.to_string());
            }
        }
    }

    None
}

pub async fn fetch() -> Result<UsageSnapshot, CodexError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let executable = find_codex_executable().ok_or(CodexError::CliNotFound)?;

    let mut child = Command::new(&executable)
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| CodexError::CliNotFound)?;

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let version = env!("CARGO_PKG_VERSION");
    let init_msg = format!(
        r#"{{"id":1,"method":"initialize","params":{{"clientInfo":{{"name":"codex-dial","title":"Codex Dial","version":"{}"}},"capabilities":{{"experimentalApi":true}}}}}}"#,
        version
    );

    let fetched_at = chrono::Utc::now().timestamp();

    let result = timeout(Duration::from_secs(15), async {
        stdin
            .write_all(format!("{}\n", init_msg).as_bytes())
            .await
            .map_err(|_| CodexError::InvalidResponse)?;
        stdin
            .flush()
            .await
            .map_err(|_| CodexError::InvalidResponse)?;

        let mut rate_limits_response: Option<serde_json::Value> = None;
        let mut usage_response: Option<serde_json::Value> = None;
        let mut line = String::new();

        loop {
            line.clear();
            let n = reader
                .read_line(&mut line)
                .await
                .map_err(|_| CodexError::InvalidResponse)?;
            if n == 0 {
                return Err(CodexError::InvalidResponse);
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let obj: serde_json::Value =
                serde_json::from_str(trimmed).map_err(|_| CodexError::InvalidResponse)?;
            let id = obj.get("id").and_then(|v| v.as_i64());

            if obj.get("error").is_some() {
                return Err(CodexError::InvalidResponse);
            }

            match id {
                Some(1) => {
                    stdin
                        .write_all(b"{\"method\":\"initialized\"}\n")
                        .await
                        .map_err(|_| CodexError::InvalidResponse)?;
                    stdin
                        .write_all(b"{\"id\":2,\"method\":\"account/rateLimits/read\"}\n")
                        .await
                        .map_err(|_| CodexError::InvalidResponse)?;
                    stdin
                        .write_all(b"{\"id\":3,\"method\":\"account/usage/read\"}\n")
                        .await
                        .map_err(|_| CodexError::InvalidResponse)?;
                    stdin
                        .flush()
                        .await
                        .map_err(|_| CodexError::InvalidResponse)?;
                }
                Some(2) => rate_limits_response = Some(obj),
                Some(3) => usage_response = Some(obj),
                _ => continue,
            }

            if let (Some(rl), Some(ur)) = (&rate_limits_response, &usage_response) {
                return decode(rl, ur, fetched_at);
            }
        }
    })
    .await;

    let _ = child.kill().await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(CodexError::TimedOut),
    }
}

fn parse_reset_credits(reset_credits: Option<ResetCredits>) -> (i32, Vec<BankedResetCredit>) {
    let Some(reset_credits) = reset_credits else {
        return (0, Vec::new());
    };

    let mut details: Vec<BankedResetCredit> = reset_credits
        .credits
        .unwrap_or_default()
        .into_iter()
        .filter(|credit| credit.status.as_deref() == Some("available"))
        .filter_map(|credit| {
            Some(BankedResetCredit {
                title: credit.title.unwrap_or_else(|| "Banked reset".to_string()),
                description: credit.description,
                expires_at: credit.expires_at?,
            })
        })
        .collect();
    details.sort_by_key(|credit| credit.expires_at);

    (reset_credits.available_count, details)
}

#[cfg(test)]
mod tests {
    use super::{parse_reset_credits, ResetCredits};

    #[test]
    fn parses_available_reset_details_in_expiry_order() {
        let reset_credits: ResetCredits = serde_json::from_value(serde_json::json!({
            "availableCount": 3,
            "credits": [
                {
                    "status": "available",
                    "expiresAt": 200,
                    "title": "Later reset",
                    "description": "Ready to redeem"
                },
                {
                    "status": "available",
                    "expiresAt": 100,
                    "title": "Sooner reset"
                },
                {
                    "status": "consumed",
                    "expiresAt": 50,
                    "title": "Consumed reset"
                }
            ]
        }))
        .unwrap();

        let (count, details) = parse_reset_credits(Some(reset_credits));

        assert_eq!(count, 3);
        assert_eq!(details.len(), 2);
        assert_eq!(details[0].title, "Sooner reset");
        assert_eq!(details[0].expires_at, 100);
        assert_eq!(details[1].title, "Later reset");
        assert_eq!(details[1].description.as_deref(), Some("Ready to redeem"));
    }

    #[test]
    fn preserves_count_when_details_are_unavailable() {
        let reset_credits: ResetCredits = serde_json::from_value(serde_json::json!({
            "availableCount": 3,
            "credits": null
        }))
        .unwrap();

        let (count, details) = parse_reset_credits(Some(reset_credits));

        assert_eq!(count, 3);
        assert!(details.is_empty());
        assert_eq!(parse_reset_credits(None), (0, Vec::new()));
    }
}

fn decode(
    rate_limits_response: &serde_json::Value,
    usage_response: &serde_json::Value,
    fetched_at: i64,
) -> Result<UsageSnapshot, CodexError> {
    let rl: RpcResponse<RateLimitsResult> = serde_json::from_value(rate_limits_response.clone())
        .map_err(|_| CodexError::InvalidResponse)?;
    let ur: RpcResponse<UsageResult> =
        serde_json::from_value(usage_response.clone()).map_err(|_| CodexError::InvalidResponse)?;

    let rate_result = rl.result.ok_or(CodexError::InvalidResponse)?;
    let usage_result = ur.result.ok_or(CodexError::InvalidResponse)?;

    let snapshots: std::collections::HashMap<String, RateLimitSnapshot> =
        rate_result.rate_limits_by_limit_id.unwrap_or_else(|| {
            let mut m = std::collections::HashMap::new();
            m.insert("codex".to_string(), rate_result.rate_limits);
            m
        });

    let main_snapshot = snapshots
        .get("codex")
        .unwrap_or_else(|| snapshots.values().next().unwrap());

    let main_windows = windows_from_snapshot(main_snapshot);
    let main_window = main_windows
        .iter()
        .min_by(|a, b| {
            a.remaining_percent
                .partial_cmp(&b.remaining_percent)
                .unwrap()
        })
        .ok_or(CodexError::MainLimitMissing)?
        .clone();

    let extra_main: Vec<LimitReading> = main_windows
        .iter()
        .filter(|w| *w != &main_window)
        .map(|w| LimitReading {
            limit_id: "codex".to_string(),
            name: window_name(w.duration_minutes),
            window: w.clone(),
        })
        .collect();

    let other_limits: Vec<LimitReading> = snapshots
        .iter()
        .filter(|(k, _)| k.as_str() != "codex")
        .filter_map(|(id, snapshot)| {
            let wins = windows_from_snapshot(snapshot);
            let window = wins.iter().min_by(|a, b| {
                a.remaining_percent
                    .partial_cmp(&b.remaining_percent)
                    .unwrap()
            })?;
            Some(LimitReading {
                limit_id: id.clone(),
                name: snapshot.limit_name.clone().unwrap_or_else(|| id.clone()),
                window: window.clone(),
            })
        })
        .collect();

    let mut others: Vec<LimitReading> = extra_main.into_iter().chain(other_limits).collect();
    others.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let token_history: Vec<TokenDay> = usage_result
        .daily_usage_buckets
        .unwrap_or_default()
        .into_iter()
        .map(|b| TokenDay {
            date: b.start_date,
            tokens: b.tokens,
        })
        .collect();

    let (banked_reset_count, banked_reset_credits) =
        parse_reset_credits(rate_result.rate_limit_reset_credits);

    Ok(UsageSnapshot {
        main_limit: LimitReading {
            limit_id: "codex".to_string(),
            name: "Codex".to_string(),
            window: main_window,
        },
        other_limits: others,
        token_history,
        banked_reset_count,
        banked_reset_credits,
        fetched_at,
    })
}

fn windows_from_snapshot(snapshot: &RateLimitSnapshot) -> Vec<UsageWindow> {
    [&snapshot.primary, &snapshot.secondary]
        .iter()
        .filter_map(|w| {
            let w = w.as_ref()?;
            let resets_at = w.resets_at?;
            let duration = w.window_duration_mins?;
            Some(UsageWindow {
                remaining_percent: (100.0 - w.used_percent).clamp(0.0, 100.0),
                resets_at,
                duration_minutes: duration,
            })
        })
        .collect()
}

fn window_name(minutes: i64) -> String {
    if minutes == 10_080 {
        "Weekly window".to_string()
    } else if minutes % 60 == 0 {
        format!("{}-hour window", minutes / 60)
    } else {
        "Additional window".to_string()
    }
}
