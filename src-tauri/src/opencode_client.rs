use crate::models::{OpenCodeGoSnapshot, OpenCodeGoWindow};
use regex::Regex;

const BASE_URL: &str = "https://opencode.ai";
const WORKSPACES_SERVER_ID: &str = "def39973159c7f0483d8793a822b8dbb10d067e12c65455fcb4608459ba0234f";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

#[derive(Debug)]
pub enum OpenCodeError {
    NotConfigured,
    SignedOut,
    ParseFailed(String),
    Network(String),
}

impl std::fmt::Display for OpenCodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "OpenCode Go: no session found. Log in via the OpenCode CLI or paste a cookie in Settings."),
            Self::SignedOut => write!(f, "OpenCode Go: session expired. Log in again or update your cookie."),
            Self::ParseFailed(detail) => write!(f, "OpenCode Go: could not parse usage data. {}", detail),
            Self::Network(msg) => write!(f, "OpenCode Go: network error — {}", msg),
        }
    }
}

fn resolve_cookie(manual_cookie: &Option<String>) -> Option<String> {
    if let Some(cookie) = manual_cookie {
        let trimmed = cookie.trim();
        if !trimmed.is_empty() {
            if trimmed.contains('=') {
                return Some(trimmed.to_string());
            }
            return Some(format!("auth={}", trimmed));
        }
    }
    None
}

fn looks_signed_out(body: &str) -> bool {
    let lower = body.to_lowercase();
    lower.contains("sign in")
        || lower.contains("login")
        || lower.contains("auth/authorize")
        || lower.contains("openauth")
        || lower.contains("not associated with an account")
}

fn build_client() -> Result<reqwest::Client, OpenCodeError> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| OpenCodeError::Network(e.to_string()))
}

async fn fetch_workspace_id(client: &reqwest::Client, cookie: &str, override_id: &Option<String>) -> Result<String, OpenCodeError> {
    if let Some(id) = override_id {
        let trimmed = id.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let url = format!("{}/_server?id={}", BASE_URL, WORKSPACES_SERVER_ID);

    let resp = client
        .get(&url)
        .header("Cookie", cookie)
        .header("Origin", BASE_URL)
        .header("Referer", format!("{}/", BASE_URL))
        .header("X-Server-Id", WORKSPACES_SERVER_ID)
        .header("Accept", "text/javascript, application/json;q=0.9, */*;q=0.8")
        .send()
        .await
        .map_err(|e| OpenCodeError::Network(e.to_string()))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(OpenCodeError::SignedOut);
    }

    let body = resp.text().await.map_err(|e| OpenCodeError::Network(e.to_string()))?;

    if looks_signed_out(&body) {
        return Err(OpenCodeError::SignedOut);
    }

    if let Some(id) = extract_workspace_id(&body) {
        return Ok(id);
    }

    // POST fallback
    let instance_id = uuid::Uuid::new_v4().to_string();
    let resp = client
        .post(format!("{}/_server", BASE_URL))
        .header("Cookie", cookie)
        .header("Origin", BASE_URL)
        .header("Referer", format!("{}/", BASE_URL))
        .header("X-Server-Id", WORKSPACES_SERVER_ID)
        .header("X-Server-Instance", format!("server-fn:{}", instance_id))
        .header("Content-Type", "application/json")
        .header("Accept", "text/javascript, application/json;q=0.9, */*;q=0.8")
        .body("[]")
        .send()
        .await
        .map_err(|e| OpenCodeError::Network(e.to_string()))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(OpenCodeError::SignedOut);
    }

    let body = resp.text().await.map_err(|e| OpenCodeError::Network(e.to_string()))?;

    extract_workspace_id(&body).ok_or_else(|| {
        let snippet: String = body.chars().take(200).collect();
        OpenCodeError::ParseFailed(format!("No workspace ID found in response. Body starts with: {:?}", snippet))
    })
}

fn extract_workspace_id(body: &str) -> Option<String> {
    let re = Regex::new(r#"id\s*:\s*"(wrk_[^"]+)""#).unwrap();
    if let Some(caps) = re.captures(body) {
        return Some(caps[1].to_string());
    }
    let re2 = Regex::new(r#"(wrk_[A-Za-z0-9]+)"#).unwrap();
    re2.captures(body).map(|caps| caps[1].to_string())
}

async fn fetch_usage_page(client: &reqwest::Client, cookie: &str, workspace_id: &str) -> Result<String, OpenCodeError> {
    let url = format!("{}/workspace/{}/go", BASE_URL, workspace_id);

    let resp = client
        .get(&url)
        .header("Cookie", cookie)
        .header("Origin", BASE_URL)
        .header("Referer", format!("{}/workspace/{}/go", BASE_URL, workspace_id))
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Sec-Fetch-Dest", "document")
        .header("Sec-Fetch-Mode", "navigate")
        .header("Sec-Fetch-Site", "same-origin")
        .send()
        .await
        .map_err(|e| OpenCodeError::Network(e.to_string()))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(OpenCodeError::SignedOut);
    }
    if status.is_redirection() {
        let location = resp.headers().get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown");
        return Err(OpenCodeError::ParseFailed(format!(
            "Redirected to {} (status {}). Cookie may be invalid or expired.", location, status
        )));
    }

    let body = resp.text().await.map_err(|e| OpenCodeError::Network(e.to_string()))?;

    if looks_signed_out(&body) {
        return Err(OpenCodeError::SignedOut);
    }

    Ok(body)
}

struct WindowParse {
    percent: f64,
    reset_in_sec: i64,
}

const PERCENT_KEYS: &[&str] = &["usagePercent", "usedPercent", "percent", "utilization", "used_percent"];
const RESET_KEYS: &[&str] = &["resetInSec", "resetSeconds", "resetIn", "reset_in_sec", "reset_seconds"];
const NEST_KEYS: &[&str] = &["data", "result", "usage", "billing", "payload"];

fn extract_percent(obj: &serde_json::Value) -> Option<f64> {
    for key in PERCENT_KEYS {
        if let Some(val) = obj.get(key).and_then(|v| v.as_f64()) {
            return Some(val);
        }
    }
    let used = obj.get("used").and_then(|v| v.as_f64());
    let limit = obj.get("limit").or(obj.get("quota")).and_then(|v| v.as_f64());
    if let (Some(u), Some(l)) = (used, limit) {
        if l > 0.0 {
            return Some(u / l * 100.0);
        }
    }
    None
}

fn extract_reset(obj: &serde_json::Value) -> Option<i64> {
    for key in RESET_KEYS {
        if let Some(val) = obj.get(key).and_then(|v| v.as_i64()) {
            return Some(val);
        }
    }
    for key in &["resetAt", "nextReset", "reset_at"] {
        if let Some(val) = obj.get(key) {
            if let Some(ts) = val.as_i64() {
                let now = chrono::Utc::now().timestamp();
                if ts > 1_000_000_000_000 {
                    return Some(ts / 1000 - now);
                } else if ts > 1_000_000_000 {
                    return Some(ts - now);
                }
            }
            if let Some(s) = val.as_str() {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                    return Some(dt.timestamp() - chrono::Utc::now().timestamp());
                }
            }
        }
    }
    None
}

fn search_json_for_window(value: &serde_json::Value, key: &str, depth: u8) -> Option<WindowParse> {
    if depth > 6 {
        return None;
    }

    if let Some(obj) = value.as_object() {
        if let Some(window_val) = obj.get(key) {
            if window_val.is_object() {
                let percent = extract_percent(window_val);
                let reset = extract_reset(window_val);
                if let (Some(p), Some(r)) = (percent, reset) {
                    return Some(WindowParse { percent: p, reset_in_sec: r });
                }
            }
        }

        for nest_key in NEST_KEYS {
            if let Some(nested) = obj.get(*nest_key) {
                if let Some(result) = search_json_for_window(nested, key, depth + 1) {
                    return Some(result);
                }
            }
        }

        for (_, v) in obj {
            if v.is_object() || v.is_array() {
                if let Some(result) = search_json_for_window(v, key, depth + 1) {
                    return Some(result);
                }
            }
        }
    }

    if let Some(arr) = value.as_array() {
        for item in arr {
            if let Some(result) = search_json_for_window(item, key, depth + 1) {
                return Some(result);
            }
        }
    }

    None
}

fn try_json_parse(body: &str, key: &str) -> Option<WindowParse> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        return search_json_for_window(&json, key, 0);
    }

    let json_re = Regex::new(r"\{[\s\S]*\}").ok()?;
    for caps in json_re.captures_iter(body) {
        let candidate = &caps[0];
        if !candidate.contains(key) {
            continue;
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(candidate) {
            if let Some(result) = search_json_for_window(&json, key, 0) {
                return Some(result);
            }
        }
    }

    None
}

fn parse_window_regex(body: &str, key: &str) -> Option<WindowParse> {
    let percent_patterns = [
        format!(r#"{}[^}}]*?usagePercent\s*:\s*([0-9.]+)"#, key),
        format!(r#"{}[^}}]*?usedPercent\s*:\s*([0-9.]+)"#, key),
        format!(r#"{}[^}}]*?percent\s*:\s*([0-9.]+)"#, key),
        format!(r#"{}[^}}]*?utilization\s*:\s*([0-9.]+)"#, key),
    ];

    let reset_patterns = [
        format!(r#"{}[^}}]*?resetInSec\s*:\s*([0-9]+)"#, key),
        format!(r#"{}[^}}]*?resetSeconds\s*:\s*([0-9]+)"#, key),
        format!(r#"{}[^}}]*?resetIn\s*:\s*([0-9]+)"#, key),
    ];

    let mut percent: Option<f64> = None;
    for pattern in &percent_patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(body) {
                if let Ok(val) = caps[1].parse::<f64>() {
                    percent = Some(val);
                    break;
                }
            }
        }
    }

    let mut reset_in_sec: Option<i64> = None;
    for pattern in &reset_patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(body) {
                if let Ok(val) = caps[1].parse::<i64>() {
                    reset_in_sec = Some(val);
                    break;
                }
            }
        }
    }

    match (percent, reset_in_sec) {
        (Some(p), Some(r)) => Some(WindowParse { percent: p, reset_in_sec: r }),
        _ => None,
    }
}

fn parse_window(body: &str, key: &str) -> Option<WindowParse> {
    let parsed = try_json_parse(body, key).or_else(|| parse_window_regex(body, key))?;
    let normalized = if parsed.percent <= 1.0 && parsed.percent > 0.0 {
        parsed.percent * 100.0
    } else {
        parsed.percent
    };
    Some(WindowParse {
        percent: normalized.clamp(0.0, 100.0),
        reset_in_sec: parsed.reset_in_sec,
    })
}

pub async fn fetch(manual_cookie: &Option<String>, workspace_id: &Option<String>) -> Result<OpenCodeGoSnapshot, OpenCodeError> {
    let cookie = resolve_cookie(manual_cookie).ok_or(OpenCodeError::NotConfigured)?;
    let client = build_client()?;
    let workspace_id = fetch_workspace_id(&client, &cookie, workspace_id).await?;
    let body = fetch_usage_page(&client, &cookie, &workspace_id).await?;

    let fetched_at = chrono::Utc::now().timestamp();

    let window_defs = [
        ("rollingUsage", "5h", 300i64),
        ("weeklyUsage", "Weekly", 10080i64),
        ("monthlyUsage", "Monthly", 43200i64),
    ];

    let mut windows = Vec::new();
    for (key, name, duration) in &window_defs {
        if let Some(parsed) = parse_window(&body, key) {
            windows.push(OpenCodeGoWindow {
                name: name.to_string(),
                remaining_percent: (100.0 - parsed.percent).clamp(0.0, 100.0),
                resets_at: fetched_at + parsed.reset_in_sec,
                duration_minutes: *duration,
            });
        }
    }

    if windows.is_empty() {
        let keys_found: Vec<&str> = window_defs.iter()
            .filter(|(key, _, _)| body.contains(key))
            .map(|(key, _, _)| *key)
            .collect();
        let snippet: String = body.chars().take(300).collect();
        let detail = format!(
            "Searched for [rollingUsage, weeklyUsage, monthlyUsage]. Keys present in response: {:?}. Response length: {} chars. Starts with: {:?}",
            keys_found, body.len(), snippet
        );
        return Err(OpenCodeError::ParseFailed(detail));
    }

    Ok(OpenCodeGoSnapshot { windows, fetched_at })
}
