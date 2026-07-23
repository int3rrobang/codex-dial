use crate::models::*;
use std::collections::HashMap;

const DAY_SECS: f64 = 86_400.0;

pub fn evaluate(
    window: &UsageWindow,
    samples: &[UsageSample],
    token_history: &[TokenDay],
    safety_buffer: f64,
    now: i64,
    previous_status: Option<PaceStatus>,
) -> Forecast {
    let now_f = now as f64;
    let resets_at_f = window.resets_at as f64;
    let starts_at_f = window.starts_at() as f64;

    let days_left = ((resets_at_f - now_f) / DAY_SECS).max(0.0);
    let elapsed_days = ((now_f - starts_at_f) / DAY_SECS).max(1.0 / 24.0);

    let current_samples: Vec<&UsageSample> = {
        let mut s: Vec<_> = samples
            .iter()
            .filter(|s| s.resets_at == window.resets_at && (s.observed_at as i64) <= now)
            .collect();
        s.sort_by_key(|s| s.observed_at);
        s
    };

    let window_rate = ((100.0 - window.remaining_percent) / elapsed_days).max(0.0);

    let recent_rate = if let (Some(first), Some(last)) = (current_samples.first(), current_samples.last()) {
        if last.observed_at > first.observed_at {
            let days = (last.observed_at - first.observed_at) as f64 / DAY_SECS;
            ((first.remaining_percent as f64 - last.remaining_percent as f64) / days).max(0.0)
        } else {
            window_rate
        }
    } else {
        window_rate
    };

    let current_rate = if current_samples.len() > 1 {
        0.7 * recent_rate + 0.3 * window_rate
    } else {
        window_rate
    };

    let historical_rates: Vec<f64> = {
        let grouped: HashMap<i64, Vec<&UsageSample>> = samples
            .iter()
            .filter(|s| s.resets_at != window.resets_at)
            .fold(HashMap::new(), |mut map, s| {
                map.entry(s.resets_at).or_default().push(s);
                map
            });

        grouped
            .values()
            .filter_map(|window_samples| {
                let mut ordered: Vec<_> = window_samples.clone();
                ordered.sort_by_key(|s| s.observed_at);
                let first = ordered.first()?;
                let last = ordered.last()?;
                if last.observed_at <= first.observed_at {
                    return None;
                }
                let days = (last.observed_at - first.observed_at) as f64 / DAY_SECS;
                Some(((first.remaining_percent as f64 - last.remaining_percent as f64) / days).max(0.0))
            })
            .collect()
    };

    let historical_rate = if historical_rates.is_empty() {
        token_bootstrap_rate(window, window_rate, token_history, now).unwrap_or(current_rate)
    } else {
        historical_rates.iter().sum::<f64>() / historical_rates.len() as f64
    };

    let expected_rate = 0.75 * current_rate + 0.25 * historical_rate;
    let safety_rate = current_rate.max(historical_rate) * 1.2;

    let expected = (window.remaining_percent - expected_rate * days_left).max(0.0);
    let safety = (window.remaining_percent - safety_rate * days_left).max(0.0);
    let historical = (window.remaining_percent - historical_rate * days_left).max(0.0);

    let recommended = if days_left > 0.0 {
        (window.remaining_percent - safety_buffer).max(0.0) / days_left
    } else {
        0.0
    };

    let status = if safety < safety_buffer
        || (previous_status == Some(PaceStatus::SlowDown) && safety < safety_buffer + 1.0)
    {
        PaceStatus::SlowDown
    } else if expected > 8.0 || (previous_status == Some(PaceStatus::RoomToUseMore) && expected > 7.0) {
        PaceStatus::RoomToUseMore
    } else {
        PaceStatus::OnTrack
    };

    Forecast {
        status,
        expected_remaining_at_reset: expected,
        safety_remaining_at_reset: safety,
        historical_remaining_at_reset: historical,
        recommended_percent_per_day: recommended,
        current_percent_per_day: expected_rate,
        historical_percent_per_day: historical_rate,
        safety_percent_per_day: safety_rate,
    }
}

fn token_bootstrap_rate(
    window: &UsageWindow,
    window_rate: f64,
    token_history: &[TokenDay],
    now: i64,
) -> Option<f64> {
    let day_number = |ts: i64| -> i64 { (ts as f64 / DAY_SECS).floor() as i64 };

    let start = day_number(window.starts_at());
    let today = day_number(now);

    let mut buckets: HashMap<i64, i64> = HashMap::new();
    for td in token_history {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(&td.date, "%Y-%m-%d") {
            let ts = date.and_hms_opt(0, 0, 0)?.and_utc().timestamp();
            let day = day_number(ts);
            *buckets.entry(day).or_default() += td.tokens;
        }
    }

    let first = *buckets.keys().min()?;
    let latest = buckets.keys().filter(|&&d| d < today).max().copied()?;
    if latest < start {
        return None;
    }

    let current_count = latest - start + 1;
    let current_tokens: i64 = (start..=latest).map(|d| buckets.get(&d).copied().unwrap_or(0)).sum();

    let history_end = start - 1;
    let history_start = first.max(history_end - 27);
    if history_start > history_end || current_tokens <= 0 {
        return None;
    }

    let history_count = history_end - history_start + 1;
    let history_tokens: i64 = (history_start..=history_end).map(|d| buckets.get(&d).copied().unwrap_or(0)).sum();

    let current_average = current_tokens as f64 / current_count as f64;
    let historical_average = history_tokens as f64 / history_count as f64;

    if current_average <= 0.0 || historical_average <= 0.0 {
        return None;
    }

    let relative_pace = (historical_average / current_average).clamp(0.25, 4.0);
    Some(window_rate * relative_pace)
}
