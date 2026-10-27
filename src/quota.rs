//! Account quota from an InGauge JSON snapshot.
//!
//! A missing measurement stays disengaged. It is never read as zero capacity.
//! Token windows are the applicable limit. Request counts stay on their own scale.

use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaReading {
    pub engaged: bool,
    pub remaining: Option<u64>,
    pub limit: Option<u64>,
    pub provider: Option<String>,
    pub samples: u32,
}

#[derive(Clone, Debug)]
struct Sample {
    provider: Option<String>,
    remaining: u64,
    limit: Option<u64>,
}

pub fn read_ingauge_slice(bytes: &[u8]) -> Result<QuotaReading, String> {
    let value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    Ok(read_ingauge(&value))
}

pub fn read_ingauge(value: &Value) -> QuotaReading {
    let mut samples = Vec::new();
    collect(value, &mut samples);
    let Some(tightest) = samples.iter().min_by_key(|sample| sample.remaining) else {
        return QuotaReading {
            engaged: false,
            remaining: None,
            limit: None,
            provider: None,
            samples: 0,
        };
    };
    QuotaReading {
        engaged: true,
        remaining: Some(tightest.remaining),
        limit: tightest.limit,
        provider: tightest.provider.clone(),
        samples: u32::try_from(samples.len()).unwrap_or(u32::MAX),
    }
}

fn collect(value: &Value, samples: &mut Vec<Sample>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect(item, samples);
            }
        }
        Value::Object(map) => {
            if let Some(data) = map.get("data") {
                collect(data, samples);
            }
            if let Some(nested) = map.get("ingauge") {
                collect(nested, samples);
            }
            if let Some(snapshots) = map.get("snapshots").and_then(Value::as_array) {
                for snapshot in snapshots {
                    collect_snapshot(snapshot, samples);
                }
            } else {
                collect_record(map, samples);
            }
        }
        _ => {}
    }
}

fn collect_snapshot(value: &Value, samples: &mut Vec<Sample>) {
    let Some(map) = value.as_object() else {
        return;
    };
    let provider = text(map, "provider");
    let Some(quotas) = map.get("remaining").and_then(Value::as_array) else {
        return;
    };
    for quota in quotas {
        let Some(record) = quota.as_object() else {
            continue;
        };
        push_sample(provider.clone(), record, samples);
    }
}

fn collect_record(map: &Map<String, Value>, samples: &mut Vec<Sample>) {
    if let Some(quotas) = map.get("remaining").and_then(Value::as_array) {
        let provider = text(map, "provider");
        for quota in quotas {
            let Some(record) = quota.as_object() else {
                continue;
            };
            push_sample(provider.clone(), record, samples);
        }
        return;
    }
    push_sample(text(map, "provider"), map, samples);
}

fn push_sample(provider: Option<String>, map: &Map<String, Value>, samples: &mut Vec<Sample>) {
    if !token_metric(map.get("metric").and_then(Value::as_str)) {
        return;
    }
    let Some(remaining) = measured_remaining(map) else {
        return;
    };
    samples.push(Sample {
        provider,
        remaining,
        limit: map.get("limit").and_then(Value::as_u64),
    });
}

fn token_metric(metric: Option<&str>) -> bool {
    matches!(metric, None | Some("tokens") | Some("remaining_tokens"))
}

fn measured_remaining(map: &Map<String, Value>) -> Option<u64> {
    if let Some(value) = map.get("remaining") {
        if let Some(remaining) = value.as_u64() {
            return Some(remaining);
        }
        if !value.is_null() {
            return None;
        }
    }
    let used = map.get("used").and_then(Value::as_u64)?;
    let limit = map.get("limit").and_then(Value::as_u64)?;
    Some(limit.saturating_sub(used))
}

fn text(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tightest_token_window_engages() {
        let value = serde_json::json!({
            "schema_version": 1,
            "command": "probe",
            "data": {
                "snapshots": [{
                    "provider": "groq",
                    "model": "ignored",
                    "remaining": [
                        {"metric": "tokens", "limit": 10000, "remaining": 4500},
                        {"metric": "requests", "remaining": 3}
                    ]
                }]
            }
        });
        let reading = read_ingauge(&value);
        assert!(reading.engaged);
        assert_eq!(reading.remaining, Some(4500));
        assert_eq!(reading.limit, Some(10000));
        assert_eq!(reading.provider.as_deref(), Some("groq"));
        assert_eq!(reading.samples, 1);
    }

    #[test]
    fn bridge_array_uses_the_smaller_token_remaining() {
        let value = serde_json::json!([
            {"provider": "groq", "used": 100, "limit": 1000, "remaining": 900},
            {"provider": "other", "remaining": 400, "limit": 800}
        ]);
        let reading = read_ingauge(&value);
        assert_eq!(reading.remaining, Some(400));
        assert_eq!(reading.provider.as_deref(), Some("other"));
        assert_eq!(reading.samples, 2);
    }

    #[test]
    fn missing_measurement_stays_disengaged() {
        let value = serde_json::json!({
            "snapshots": [{
                "provider": "groq",
                "state": "unknown",
                "remaining": [{"metric": "tokens", "remaining": null}],
                "headroom": 0.5
            }]
        });
        let reading = read_ingauge(&value);
        assert!(!reading.engaged);
        assert_eq!(reading.remaining, None);
        assert_eq!(reading.samples, 0);
    }

    #[test]
    fn fractional_remaining_is_not_zero() {
        let value = serde_json::json!({"remaining": 12.5, "limit": 100});
        let reading = read_ingauge(&value);
        assert!(!reading.engaged);
        assert_eq!(reading.remaining, None);
    }

    #[test]
    fn used_and_limit_derive_remaining() {
        let value = serde_json::json!({"provider": "groq", "used": 20, "limit": 50});
        assert_eq!(read_ingauge(&value).remaining, Some(30));
    }

    #[test]
    fn fixture_envelope_is_read_from_the_ingauge_field() {
        let value = serde_json::json!({
            "grant": 20000,
            "ingauge": {"provider": "groq", "remaining": 4500, "limit": 10000}
        });
        assert_eq!(read_ingauge(&value).remaining, Some(4500));
    }

    #[test]
    fn zero_remaining_is_a_real_measurement() {
        let value = serde_json::json!({"remaining": 0, "limit": 10});
        let reading = read_ingauge(&value);
        assert!(reading.engaged);
        assert_eq!(reading.remaining, Some(0));
    }
}
