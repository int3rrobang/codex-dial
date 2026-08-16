use crate::models::{OpenCodeGoSnapshot, OpenCodeGoWindow};
use chrono::DateTime;
use serde::Deserialize;

const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

#[derive(Debug)]
pub enum OpenCodeError {
    NotConfigured,
    Unauthorized,
    Forbidden,
    RateLimited,
    HttpStatus(u16),
    Network,
    ParseFailed,
}

impl std::fmt::Display for OpenCodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(
                f,
                "OpenCode Go: API key is not configured. Add an API key in Settings."
            ),
            Self::Unauthorized => write!(
                f,
                "OpenCode Go: API key was rejected. Check the key in Settings."
            ),
            Self::Forbidden => write!(
                f,
                "OpenCode Go: this API key cannot access OpenCode Go usage."
            ),
            Self::RateLimited => write!(
                f,
                "OpenCode Go: usage requests are temporarily rate limited. Try again later."
            ),
            Self::HttpStatus(status) => write!(
                f,
                "OpenCode Go: usage service returned HTTP status {status}. Try again later."
            ),
            Self::Network => write!(
                f,
                "OpenCode Go: could not reach the usage service. Check your connection and try again."
            ),
            Self::ParseFailed => write!(
                f,
                "OpenCode Go: usage service returned an unexpected response. Try again later."
            ),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageResponse {
    usage: UsageData,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageData {
    rolling: UsageWindowData,
    weekly: UsageWindowData,
    monthly: UsageWindowData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum UsageStatus {
    Ok,
    RateLimited,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageWindowData {
    #[serde(rename = "status")]
    _status: UsageStatus,
    percent: f64,
    #[serde(rename = "resetsAt")]
    resets_at: String,
}

fn build_client() -> Result<reqwest::Client, OpenCodeError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| OpenCodeError::Network)
}

fn status_error(status: reqwest::StatusCode) -> OpenCodeError {
    match status {
        reqwest::StatusCode::UNAUTHORIZED => OpenCodeError::Unauthorized,
        reqwest::StatusCode::FORBIDDEN => OpenCodeError::Forbidden,
        reqwest::StatusCode::TOO_MANY_REQUESTS => OpenCodeError::RateLimited,
        status => OpenCodeError::HttpStatus(status.as_u16()),
    }
}

fn parse_usage(body: &str, fetched_at: i64) -> Result<OpenCodeGoSnapshot, OpenCodeError> {
    let response: UsageResponse =
        serde_json::from_str(body).map_err(|_| OpenCodeError::ParseFailed)?;

    let window_defs = [
        ("5h", 300i64, &response.usage.rolling),
        ("Weekly", 10080i64, &response.usage.weekly),
        ("Monthly", 43200i64, &response.usage.monthly),
    ];
    let mut windows = Vec::with_capacity(window_defs.len());

    for (name, duration_minutes, usage) in window_defs {
        if !usage.percent.is_finite() || !(0.0..=100.0).contains(&usage.percent) {
            return Err(OpenCodeError::ParseFailed);
        }
        let resets_at = DateTime::parse_from_rfc3339(&usage.resets_at)
            .map_err(|_| OpenCodeError::ParseFailed)?
            .timestamp();
        windows.push(OpenCodeGoWindow {
            name: name.to_string(),
            remaining_percent: 100.0 - usage.percent,
            resets_at,
            duration_minutes,
        });
    }

    Ok(OpenCodeGoSnapshot { windows, fetched_at })
}

async fn fetch_from_endpoint(
    api_key: &Option<String>,
    endpoint_url: &str,
) -> Result<OpenCodeGoSnapshot, OpenCodeError> {
    let api_key = api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or(OpenCodeError::NotConfigured)?;
    let client = build_client()?;
    let response = client
        .get(endpoint_url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|_| OpenCodeError::Network)?;

    if !response.status().is_success() {
        return Err(status_error(response.status()));
    }

    let body = response.text().await.map_err(|_| OpenCodeError::Network)?;
    parse_usage(&body, chrono::Utc::now().timestamp())
}

pub async fn fetch(api_key: &Option<String>) -> Result<OpenCodeGoSnapshot, OpenCodeError> {
    fetch_from_endpoint(api_key, USAGE_URL).await
}

#[cfg(test)]
mod tests {
    use super::{fetch_from_endpoint, parse_usage, status_error, OpenCodeError};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const SUCCESS_FIXTURE: &str = r#"{
        "usage": {
            "rolling": {"status": "ok", "percent": 0, "resetsAt": "2030-01-01T00:00:00Z"},
            "weekly": {"status": "ok", "percent": 50, "resetsAt": "2030-01-08T01:02:03+01:00"},
            "monthly": {"status": "rate-limited", "percent": 100, "resetsAt": "2030-02-01T00:00:00.000Z"}
        }
    }"#;

    #[test]
    fn parses_exact_api_casing_and_maps_boundaries() {
        let snapshot = parse_usage(SUCCESS_FIXTURE, 42).expect("fixture should parse");

        assert_eq!(snapshot.fetched_at, 42);
        assert_eq!(snapshot.windows.len(), 3);
        assert_eq!(snapshot.windows[0].name, "5h");
        assert_eq!(snapshot.windows[0].remaining_percent, 100.0);
        assert_eq!(snapshot.windows[0].resets_at, 1_893_456_000);
        assert_eq!(snapshot.windows[0].duration_minutes, 300);
        assert_eq!(snapshot.windows[1].remaining_percent, 50.0);
        assert_eq!(snapshot.windows[1].duration_minutes, 10080);
        assert_eq!(snapshot.windows[2].remaining_percent, 0.0);
        assert_eq!(snapshot.windows[2].duration_minutes, 43200);
    }

    #[test]
    fn rejects_unknown_status() {
        let fixture = SUCCESS_FIXTURE.replace("\"status\": \"ok\"", "\"status\": \"warning\"");
        assert!(matches!(
            parse_usage(&fixture, 42),
            Err(OpenCodeError::ParseFailed)
        ));
    }

    #[test]
    fn rejects_wrong_key_casing() {
        let fixture = SUCCESS_FIXTURE.replace("\"resetsAt\"", "\"resets_at\"");
        assert!(matches!(
            parse_usage(&fixture, 42),
            Err(OpenCodeError::ParseFailed)
        ));
    }

    #[test]
    fn rejects_malformed_missing_wrong_type_and_invalid_timestamp() {
        let fixtures = [
            r#"not json"#,
            r#"{"usage":{"rolling":{},"weekly":{},"monthly":{}}}"#,
            r#"{"usage":{"rolling":{"status":"ok","percent":"50","resetsAt":"2030-01-01T00:00:00Z"},"weekly":{"status":"ok","percent":50,"resetsAt":"2030-01-01T00:00:00Z"},"monthly":{"status":"ok","percent":50,"resetsAt":"2030-01-01T00:00:00Z"}}}"#,
            r#"{"usage":{"rolling":{"status":"ok","percent":50,"resetsAt":"not-a-timestamp"},"weekly":{"status":"ok","percent":50,"resetsAt":"2030-01-01T00:00:00Z"},"monthly":{"status":"ok","percent":50,"resetsAt":"2030-01-01T00:00:00Z"}}}"#,
            r#"{"usage":{"rolling":{"status":"ok","percent":-0.1,"resetsAt":"2030-01-01T00:00:00Z"},"weekly":{"status":"ok","percent":50,"resetsAt":"2030-01-01T00:00:00Z"},"monthly":{"status":"ok","percent":50,"resetsAt":"2030-01-01T00:00:00Z"}}}"#,
            r#"{"usage":{"rolling":{"status":"ok","percent":100.1,"resetsAt":"2030-01-01T00:00:00Z"},"weekly":{"status":"ok","percent":50,"resetsAt":"2030-01-01T00:00:00Z"},"monthly":{"status":"ok","percent":50,"resetsAt":"2030-01-01T00:00:00Z"}}}"#,
        ];

        for fixture in fixtures {
            assert!(matches!(
                parse_usage(fixture, 42),
                Err(OpenCodeError::ParseFailed)
            ));
        }
    }

    #[test]
    fn status_errors_are_actionable_without_response_data() {
        for (status, expected) in [
            (401, "API key was rejected"),
            (403, "cannot access"),
            (429, "rate limited"),
            (500, "HTTP status 500"),
        ] {
            let status = reqwest::StatusCode::from_u16(status).unwrap();
            let message = status_error(status).to_string();
            assert!(message.contains(expected));
            assert!(!message.contains("secret"));
            assert!(!message.contains("response body"));
        }
    }
    #[tokio::test]
    async fn fetches_usage_from_endpoint_and_maps_response() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let endpoint_url = format!(
            "http://{}/zen/go/v1/usage",
            listener.local_addr().expect("listener should have an address")
        );

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("request should arrive");
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            loop {
                let bytes_read = socket
                    .read(&mut buffer)
                    .await
                    .expect("request should be readable");
                if bytes_read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..bytes_read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }

            let request = String::from_utf8(request).expect("request should be valid HTTP");
            let mut request_parts = request.lines().next().unwrap_or_default().split_whitespace();
            assert_eq!(request_parts.next(), Some("GET"));
            assert_eq!(request_parts.next(), Some("/zen/go/v1/usage"));
            assert_eq!(request_parts.next(), Some("HTTP/1.1"));
            assert!(request.lines().any(|line| {
                let Some((name, value)) = line.split_once(':') else {
                    return false;
                };
                name.eq_ignore_ascii_case("authorization") && value.trim() == "Bearer fixture-key"
            }));

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                SUCCESS_FIXTURE.len(),
                SUCCESS_FIXTURE
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("response should be writable");
        });

        let before_fetch = chrono::Utc::now().timestamp();
        let result = fetch_from_endpoint(
            &Some(String::from("fixture-key")),
            &endpoint_url,
        )
        .await;
        server.await.expect("server should complete successfully");
        let snapshot = result.expect("usage response should parse");
        let after_fetch = chrono::Utc::now().timestamp();

        assert!((before_fetch..=after_fetch).contains(&snapshot.fetched_at));
        assert_eq!(snapshot.windows.len(), 3);
        assert_eq!(snapshot.windows[0].name, "5h");
        assert_eq!(snapshot.windows[0].remaining_percent, 100.0);
        assert_eq!(snapshot.windows[0].resets_at, 1_893_456_000);
        assert_eq!(snapshot.windows[0].duration_minutes, 300);
        assert_eq!(snapshot.windows[1].name, "Weekly");
        assert_eq!(snapshot.windows[1].remaining_percent, 50.0);
        assert_eq!(snapshot.windows[1].resets_at, 1_894_060_923);
        assert_eq!(snapshot.windows[1].duration_minutes, 10080);
        assert_eq!(snapshot.windows[2].name, "Monthly");
        assert_eq!(snapshot.windows[2].remaining_percent, 0.0);
        assert_eq!(snapshot.windows[2].resets_at, 1_896_134_400);
        assert_eq!(snapshot.windows[2].duration_minutes, 43200);
    }

    #[tokio::test]
    async fn maps_unauthorized_response_to_safe_error() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let endpoint_url = format!(
            "http://{}/zen/go/v1/usage",
            listener.local_addr().expect("listener should have an address")
        );

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("request should arrive");
            let mut request = [0u8; 1024];
            socket
                .read(&mut request)
                .await
                .expect("request should be readable");
            socket
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .expect("response should be writable");
        });

        let result = fetch_from_endpoint(
            &Some(String::from("fixture-key")),
            &endpoint_url,
        )
        .await;
        server.await.expect("server should complete successfully");
        let error = result.expect_err("unauthorized response should fail");

        assert!(matches!(error, OpenCodeError::Unauthorized));
        let message = error.to_string();
        assert!(message.contains("API key was rejected"));
        assert!(!message.contains("fixture-key"));
    }
}
