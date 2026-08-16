use crate::models::*;
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::{timeout, Duration};

#[derive(Debug, PartialEq, Eq)]
pub enum CodexError {
    CliNotFound,
    InvalidResponse,
    MainLimitMissing,
    TimedOut,
    InvalidId,
    NoCredit,
    NothingToReset,
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
            Self::InvalidId => write!(f, "The reset credit or idempotency key is invalid."),
            Self::NoCredit => write!(f, "Codex could not find that reset credit."),
            Self::NothingToReset => write!(f, "Codex has nothing to reset for that credit."),
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
    id: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConsumeOutcome {
    Reset,
    AlreadyRedeemed,
}

fn initialize_request(version: &str) -> serde_json::Value {
    serde_json::json!({
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "codex-dial",
                "title": "Codex Dial",
                "version": version
            },
            "capabilities": {
                "experimentalApi": true
            }
        }
    })
}

fn consume_request(credit_id: &str, idempotency_key: &str) -> serde_json::Value {
    serde_json::json!({
        "id": 2,
        "method": "account/rateLimitResetCredit/consume",
        "params": {
            "idempotencyKey": idempotency_key,
            "creditId": credit_id
        }
    })
}

fn initialized_notification() -> serde_json::Value {
    serde_json::json!({ "method": "initialized" })
}

fn decode_consume_response(response: &serde_json::Value) -> Result<ConsumeOutcome, CodexError> {
    if response.get("id").and_then(|id| id.as_i64()) != Some(2) {
        return Err(CodexError::InvalidResponse);
    }
    if response
        .get("error")
        .is_some_and(|error| !error.is_null())
    {
        return Err(CodexError::InvalidResponse);
    }

    let rpc: RpcResponse<ConsumeResult> =
        serde_json::from_value(response.clone()).map_err(|_| CodexError::InvalidResponse)?;
    let outcome = rpc
        .result
        .and_then(|result| result.outcome)
        .ok_or(CodexError::InvalidResponse)?;

    match outcome.as_str() {
        "reset" => Ok(ConsumeOutcome::Reset),
        "alreadyRedeemed" => Ok(ConsumeOutcome::AlreadyRedeemed),
        "nothingToReset" => Err(CodexError::NothingToReset),
        "noCredit" => Err(CodexError::NoCredit),
        _ => Err(CodexError::InvalidResponse),
    }
}

#[derive(Deserialize)]
struct ConsumeResult {
    outcome: Option<String>,
}

fn validate_consume_ids(credit_id: &str, idempotency_key: &str) -> Result<(), CodexError> {
    if credit_id.trim().is_empty() || uuid::Uuid::parse_str(idempotency_key).is_err() {
        return Err(CodexError::InvalidId);
    }
    Ok(())
}

fn configure_app_server_command(command: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn app_server_command(executable: &str) -> Command {
    let mut command = Command::new(executable);
    command
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configure_app_server_command(&mut command);
    command
}

async fn spawn_app_server(executable: &str) -> Result<Child, CodexError> {
    app_server_command(executable)
        .spawn()
        .map_err(|_| CodexError::CliNotFound)
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
    let executable = find_codex_executable().ok_or(CodexError::CliNotFound)?;
    let mut child = spawn_app_server(&executable).await?;

    let mut stdin = child.stdin.take().ok_or(CodexError::InvalidResponse)?;
    let stdout = child.stdout.take().ok_or(CodexError::InvalidResponse)?;
    let mut reader = BufReader::new(stdout);

    let version = env!("CARGO_PKG_VERSION");
    let init_msg = serde_json::to_string(&initialize_request(version))
        .map_err(|_| CodexError::InvalidResponse)?;

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


            if obj
                .get("error")
                .is_some_and(|error| !error.is_null())
            {
                return Err(CodexError::InvalidResponse);
            }

            match id {
                Some(1) => {
                    let initialized = serde_json::to_string(&initialized_notification())
                        .map_err(|_| CodexError::InvalidResponse)?;
                    stdin
                        .write_all(format!("{}\n", initialized).as_bytes())
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

async fn consume_reset_credit_session<R, W>(
    reader: &mut R,
    writer: &mut W,
    init_msg: &str,
    consume_msg: &str,
) -> Result<ConsumeOutcome, CodexError>
where
    R: tokio::io::AsyncBufRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    writer
        .write_all(format!("{}\n", init_msg).as_bytes())
        .await
        .map_err(|_| CodexError::InvalidResponse)?;
    writer
        .flush()
        .await
        .map_err(|_| CodexError::InvalidResponse)?;

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
        let response: serde_json::Value =
            serde_json::from_str(trimmed).map_err(|_| CodexError::InvalidResponse)?;
        match response.get("id").and_then(|id| id.as_i64()) {
            Some(1) => {
                let initialized = serde_json::to_string(&initialized_notification())
                    .map_err(|_| CodexError::InvalidResponse)?;
                writer
                    .write_all(format!("{}\n", initialized).as_bytes())
                    .await
                    .map_err(|_| CodexError::InvalidResponse)?;
                writer
                    .write_all(format!("{}\n", consume_msg).as_bytes())
                    .await
                    .map_err(|_| CodexError::InvalidResponse)?;
                writer
                    .flush()
                    .await
                    .map_err(|_| CodexError::InvalidResponse)?;
            }
            Some(2) => return decode_consume_response(&response),
            _ => continue,
        }
    }
}

pub(crate) async fn consume_reset_credit(
    credit_id: &str,
    idempotency_key: &str,
) -> Result<ConsumeOutcome, CodexError> {
    validate_consume_ids(credit_id, idempotency_key)?;

    let executable = find_codex_executable().ok_or(CodexError::CliNotFound)?;
    let mut child = spawn_app_server(&executable).await?;
    let mut stdin = child.stdin.take().ok_or(CodexError::InvalidResponse)?;
    let stdout = child.stdout.take().ok_or(CodexError::InvalidResponse)?;
    let mut reader = BufReader::new(stdout);
    let version = env!("CARGO_PKG_VERSION");
    let init_msg = serde_json::to_string(&initialize_request(version))
        .map_err(|_| CodexError::InvalidResponse)?;
    let consume_msg = serde_json::to_string(&consume_request(credit_id, idempotency_key))
        .map_err(|_| CodexError::InvalidResponse)?;

    let result = timeout(
        Duration::from_secs(15),
        consume_reset_credit_session(&mut reader, &mut stdin, &init_msg, &consume_msg),
    )
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
            let id = credit.id.filter(|id| !id.trim().is_empty())?;
            Some(BankedResetCredit {
                id,
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
                    "id": "credit-later",
                    "expiresAt": 200,
                    "title": "Later reset",
                    "description": "Ready to redeem"
                },
                {
                    "status": "available",
                    "id": "credit-sooner",
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

        assert_eq!(details[0].id, "credit-sooner");
        assert_eq!(details[1].id, "credit-later");
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
    #[test]
    fn builds_initialize_request_with_experimental_api() {
        assert_eq!(
            super::initialize_request("1.2.3"),
            serde_json::json!({
                "id": 1,
                "method": "initialize",
                "params": {
                    "clientInfo": {
                        "name": "codex-dial",
                        "title": "Codex Dial",
                        "version": "1.2.3"
                    },
                    "capabilities": {"experimentalApi": true}
                }
            })
        );
    }
    #[test]

    fn skips_available_credits_without_non_empty_ids_but_preserves_count() {
        let reset_credits: ResetCredits = serde_json::from_value(serde_json::json!({
            "availableCount": 2,
            "credits": [
                {"status": "available", "id": "", "expiresAt": 100},
                {"status": "available", "expiresAt": 200}
            ]
        }))
        .unwrap();

        let (count, details) = parse_reset_credits(Some(reset_credits));

        assert_eq!(count, 2);
        assert!(details.is_empty());
    }

    #[test]
    fn builds_exact_consume_request() {
        assert_eq!(
            super::consume_request("opaque-credit", "550e8400-e29b-41d4-a716-446655440000"),
            serde_json::json!({
                "id": 2,
                "method": "account/rateLimitResetCredit/consume",
                "params": {
                    "idempotencyKey": "550e8400-e29b-41d4-a716-446655440000",
                    "creditId": "opaque-credit"
                }
            })
        );
    }

    #[test]
    fn decodes_all_consume_outcomes_without_exposing_response_body() {
        assert_eq!(
            super::decode_consume_response(&serde_json::json!({
                "id": 2, "result": {"outcome": "reset"}
            })),
            Ok(super::ConsumeOutcome::Reset)
        );
        assert_eq!(
            super::decode_consume_response(&serde_json::json!({
                "id": 2, "result": {"outcome": "alreadyRedeemed"}
            })),
            Ok(super::ConsumeOutcome::AlreadyRedeemed)
        );
        assert!(matches!(
            super::decode_consume_response(&serde_json::json!({
                "id": 2, "result": {"outcome": "nothingToReset"}
            })),
            Err(super::CodexError::NothingToReset)
        ));
        assert!(matches!(
            super::decode_consume_response(&serde_json::json!({
                "id": 2, "result": {"outcome": "noCredit"}
            })),
            Err(super::CodexError::NoCredit)
        ));
        assert!(matches!(
            super::decode_consume_response(&serde_json::json!({
                "id": 2, "error": {"message": "secret body"}
            })),
            Err(super::CodexError::InvalidResponse)
        ));
    }

    #[test]
    fn validates_uuid_and_non_empty_credit_id_before_launch() {
        assert!(super::validate_consume_ids(
            "opaque-credit",
            "550e8400-e29b-41d4-a716-446655440000"
        )
        .is_ok());
        assert!(matches!(
            super::validate_consume_ids("", "550e8400-e29b-41d4-a716-446655440000"),
            Err(super::CodexError::InvalidId)
        ));
        assert!(matches!(
            super::validate_consume_ids("opaque-credit", "not-a-uuid"),
            Err(super::CodexError::InvalidId)
        ));
    }
    #[tokio::test]
    async fn consume_session_uses_injected_stream_and_decodes_reset() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::time::{timeout, Duration};

        let credit_id = "opaque-credit";
        let idempotency_key = "550e8400-e29b-41d4-a716-446655440000";
        let (client, server) = tokio::io::duplex(4096);
        let (client_reader, mut client_writer) = tokio::io::split(client);
        let (server_reader, mut server_writer) = tokio::io::split(server);
        let mut client_reader = BufReader::new(client_reader);

        let fake_server = tokio::spawn(async move {
            let mut server_reader = BufReader::new(server_reader);
            let mut line = String::new();

            server_reader.read_line(&mut line).await.unwrap();
            let initialize: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
            assert_eq!(
                initialize,
                super::initialize_request(env!("CARGO_PKG_VERSION"))
            );

            server_writer
                .write_all(b"{\"id\":1,\"result\":{}}\n")
                .await
                .unwrap();
            server_writer.flush().await.unwrap();

            line.clear();
            server_reader.read_line(&mut line).await.unwrap();
            let initialized: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
            assert_eq!(initialized, super::initialized_notification());

            line.clear();
            server_reader.read_line(&mut line).await.unwrap();
            let consume: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
            assert_eq!(consume, super::consume_request(credit_id, idempotency_key));
            assert_eq!(consume["method"], "account/rateLimitResetCredit/consume");
            assert_eq!(consume["params"]["creditId"], credit_id);
            assert_eq!(consume["params"]["idempotencyKey"], idempotency_key);

            server_writer
                .write_all(b"{\"id\":2,\"result\":{\"outcome\":\"reset\"}}\n")
                .await
                .unwrap();
            server_writer.flush().await.unwrap();
        });

        let init_msg =
            serde_json::to_string(&super::initialize_request(env!("CARGO_PKG_VERSION"))).unwrap();
        let consume_msg = serde_json::to_string(&super::consume_request(credit_id, idempotency_key))
            .unwrap();
        let result = timeout(
            Duration::from_secs(1),
            super::consume_reset_credit_session(
                &mut client_reader,
                &mut client_writer,
                &init_msg,
                &consume_msg,
            ),
        )
        .await
        .expect("in-process consume session timed out");

        assert_eq!(result, Ok(super::ConsumeOutcome::Reset));
        fake_server.await.unwrap();
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
