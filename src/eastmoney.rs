use crate::core::{
    DailyRecord, DailyRequest, DuckAkiError, FetchBatch, MAX_RAW_RECORDS, SUPPORTED_SOURCE,
    SUPPORTED_SYMBOL,
};
use chrono::{NaiveDate, Utc};
use serde_json::Value;
use std::io::Read;
use std::time::{Duration, Instant};

pub const PROD_ENDPOINT: &str = "https://push2his.eastmoney.com/api/qt/stock/kline/get";
pub const MAX_HEADER_SIZE: usize = 32 * 1024;
pub const MAX_BODY_SIZE: usize = 1024 * 1024;

fn resolve_endpoint() -> Result<String, DuckAkiError> {
    #[cfg(feature = "test-support")]
    {
        if let Ok(endpoint) = std::env::var("DUCKAKI_TEST_ENDPOINT") {
            let endpoint = endpoint.trim();
            if !endpoint.is_empty() {
                if is_loopback_url(endpoint) {
                    return Ok(endpoint.to_string());
                } else {
                    return Err(DuckAkiError::Internal(format!(
                        "DUCKAKI_TEST_ENDPOINT must be a loopback URL, got: {endpoint}"
                    )));
                }
            }
        }
    }

    Ok(PROD_ENDPOINT.to_string())
}

fn is_loopback_url(url: &str) -> bool {
    [
        "http://127.0.0.1:",
        "http://localhost:",
        "http://[::1]:",
        "https://127.0.0.1:",
        "https://localhost:",
        "https://[::1]:",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix))
}

#[cfg(feature = "test-support")]
fn request_timeout_for_url(url: &str) -> Duration {
    if is_loopback_url(url) {
        if let Some(timeout_ms) = std::env::var("DUCKAKI_TEST_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|timeout_ms| *timeout_ms > 0)
        {
            return Duration::from_millis(timeout_ms);
        }
    }
    Duration::from_secs(30)
}

#[cfg(not(feature = "test-support"))]
fn request_timeout_for_url(_url: &str) -> Duration {
    Duration::from_secs(30)
}

fn check_deadline(deadline: Instant, stage: &str) -> Result<(), DuckAkiError> {
    if Instant::now() >= deadline {
        return Err(DuckAkiError::SourceAccess(format!(
            "[{SUPPORTED_SOURCE}] request deadline exceeded during {stage}"
        )));
    }
    Ok(())
}

pub fn build_url(base_url: &str, req: &DailyRequest) -> Result<String, DuckAkiError> {
    if req.symbol != SUPPORTED_SYMBOL {
        return Err(DuckAkiError::InvalidArgument(format!(
            "unsupported symbol '{}'",
            req.symbol
        )));
    }
    // sz000001 -> secid 0.000001
    let secid = "0.000001";
    let beg = req.start_date.format("%Y%m%d").to_string();
    let end = req.end_date.format("%Y%m%d").to_string();

    let query = format!(
        "secid={}&beg={}&end={}&klt=101&fqt=0&fields1=f1,f2,f3,f4,f5,f6&fields2=f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61,f116",
        secid, beg, end
    );

    let separator = if base_url.contains('?') { "&" } else { "?" };
    Ok(format!("{base_url}{separator}{query}"))
}

pub fn http_get(url: &str) -> Result<String, DuckAkiError> {
    let deadline = Instant::now() + request_timeout_for_url(url);
    http_get_until(url, deadline)
}

fn http_get_until(url: &str, deadline: Instant) -> Result<String, DuckAkiError> {
    check_deadline(deadline, "request setup")?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let phase_timeout = remaining.min(Duration::from_secs(5));
    let config = ureq::config::Config::builder()
        .timeout_global(Some(remaining))
        .timeout_resolve(Some(phase_timeout))
        .timeout_connect(Some(phase_timeout))
        .timeout_recv_body(Some(remaining))
        .max_redirects(0)
        .max_response_header_size(MAX_HEADER_SIZE)
        .http_status_as_error(false)
        .build();

    let agent: ureq::Agent = config.new_agent();

    let resp = match agent.get(url).call() {
        Ok(r) => r,
        Err(ureq::Error::LargeResponseHeader(actual, limit)) => {
            return Err(DuckAkiError::LimitExceeded(format!(
                "[{SUPPORTED_SOURCE}] response header size {actual} exceeds {limit} bytes limit"
            )));
        }
        Err(e) => {
            let msg = e.to_string();
            return Err(DuckAkiError::SourceAccess(format!(
                "[{SUPPORTED_SOURCE}] connection/request error: {msg}"
            )));
        }
    };

    check_deadline(deadline, "response headers")?;
    let status = resp.status().as_u16();
    if status < 200 || status >= 300 {
        return Err(DuckAkiError::SourceAccess(format!(
            "[{SUPPORTED_SOURCE}] http_status={status}"
        )));
    }

    // Read body bounded by MAX_BODY_SIZE + 1
    let reader = resp.into_body().into_reader();
    let mut buf = Vec::new();
    let take_limit = (MAX_BODY_SIZE + 1) as u64;
    match reader.take(take_limit).read_to_end(&mut buf) {
        Ok(_) => {
            if buf.len() > MAX_BODY_SIZE {
                return Err(DuckAkiError::LimitExceeded(format!(
                    "[{SUPPORTED_SOURCE}] response body exceeds {} bytes limit",
                    MAX_BODY_SIZE
                )));
            }
        }
        Err(e) => {
            return Err(DuckAkiError::SourceAccess(format!(
                "[{SUPPORTED_SOURCE}] read body failed: {e}"
            )));
        }
    }
    check_deadline(deadline, "response body")?;

    String::from_utf8(buf).map_err(|e| {
        DuckAkiError::SourceData(format!("[{SUPPORTED_SOURCE}] body is not valid UTF-8: {e}"))
    })
}

pub fn parse_raw_volume_to_shares(vol_str: &str) -> Result<i64, DuckAkiError> {
    let s = vol_str.trim();
    if s.is_empty() {
        return Err(DuckAkiError::SourceData(
            "empty volume string is not allowed".to_string(),
        ));
    }

    let (whole_str, frac_str) = match s.split_once('.') {
        Some((whole, frac)) if !frac.contains('.') => (whole, Some(frac)),
        Some(_) => {
            return Err(DuckAkiError::SourceData(format!(
                "invalid volume format: {s}"
            )));
        }
        None => (s, None),
    };
    if whole_str.is_empty()
        || !whole_str.bytes().all(|byte| byte.is_ascii_digit())
        || frac_str.is_some_and(|frac| {
            frac.is_empty() || !frac.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(DuckAkiError::SourceData(format!(
            "invalid non-negative volume format: {s}"
        )));
    }

    let whole: i64 = whole_str.parse().map_err(|_| {
        DuckAkiError::SourceData(format!("invalid volume integer part: {whole_str}"))
    })?;
    let mut frac_cents = 0i64;
    if let Some(frac_str) = frac_str {
        if frac_str.len() > 2 {
            let (first_two, rest) = frac_str.split_at(2);
            if rest.bytes().any(|byte| byte != b'0') {
                return Err(DuckAkiError::SourceData(format!(
                    "volume {s} hands does not convert to integer shares"
                )));
            }
            frac_cents = first_two.parse().map_err(|_| {
                DuckAkiError::SourceData(format!("invalid volume fraction: {frac_str}"))
            })?;
        } else if frac_str.len() == 2 {
            frac_cents = frac_str.parse().map_err(|_| {
                DuckAkiError::SourceData(format!("invalid volume fraction: {frac_str}"))
            })?;
        } else {
            frac_cents = frac_str.parse::<i64>().map_err(|_| {
                DuckAkiError::SourceData(format!("invalid volume fraction: {frac_str}"))
            })? * 10;
        }
    }

    whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(frac_cents))
        .ok_or_else(|| DuckAkiError::LimitExceeded(format!("volume {s} overflowed i64 shares")))
}

pub fn parse_kline_response(
    body: &str,
    req: &DailyRequest,
    fetched_at_micros: i64,
) -> Result<Vec<DailyRecord>, DuckAkiError> {
    let deadline = Instant::now() + Duration::from_secs(30);
    parse_kline_response_until(body, req, fetched_at_micros, deadline)
}

fn parse_kline_response_until(
    body: &str,
    req: &DailyRequest,
    fetched_at_micros: i64,
    deadline: Instant,
) -> Result<Vec<DailyRecord>, DuckAkiError> {
    let json: Value = serde_json::from_str(body).map_err(|e| {
        DuckAkiError::SourceData(format!("[{SUPPORTED_SOURCE}] JSON decode error: {e}"))
    })?;
    check_deadline(deadline, "JSON parsing")?;

    // Check envelope
    let rc = json.get("rc").and_then(|v| v.as_i64()).ok_or_else(|| {
        DuckAkiError::SourceData(format!("[{SUPPORTED_SOURCE}] missing or invalid 'rc' in envelope"))
    })?;
    if rc != 0 {
        return Err(DuckAkiError::SourceData(format!(
            "[{SUPPORTED_SOURCE}] envelope error: rc={rc}"
        )));
    }

    let data = match json.get("data") {
        Some(Value::Object(map)) => map,
        Some(Value::Null) | None => {
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] envelope data is null or missing"
            )));
        }
        _ => {
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] envelope data is not an object"
            )));
        }
    };

    // Check identity
    let code = data.get("code").and_then(|v| v.as_str()).ok_or_else(|| {
        DuckAkiError::SourceData(format!("[{SUPPORTED_SOURCE}] missing 'code' in envelope data"))
    })?;
    // Expected code for sz000001 is "000001"
    if code != "000001" {
        return Err(DuckAkiError::SourceData(format!(
            "[{SUPPORTED_SOURCE}] unexpected code in response: expected '000001', got '{code}'"
        )));
    }

    let market = data.get("market").and_then(|v| v.as_i64()).ok_or_else(|| {
        DuckAkiError::SourceData(format!(
            "[{SUPPORTED_SOURCE}] missing 'market' in envelope data"
        ))
    })?;
    if market != 0 {
        return Err(DuckAkiError::SourceData(format!(
            "[{SUPPORTED_SOURCE}] unexpected market: expected 0, got {market}"
        )));
    }

    let klines = match data.get("klines") {
        Some(Value::Array(arr)) => arr,
        Some(Value::Null) | None => {
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] missing 'klines' array in envelope data"
            )));
        }
        _ => {
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] 'klines' is not an array"
            )));
        }
    };

    if klines.is_empty() {
        check_deadline(deadline, "response validation")?;
        return Ok(Vec::new());
    }

    if klines.len() > MAX_RAW_RECORDS {
        return Err(DuckAkiError::LimitExceeded(format!(
            "[{SUPPORTED_SOURCE}] raw record count {} exceeds maximum {}",
            klines.len(),
            MAX_RAW_RECORDS
        )));
    }

    let mut records = Vec::with_capacity(klines.len());
    let mut seen_dates = std::collections::HashSet::new();
    for item in klines {
        check_deadline(deadline, "record parsing")?;
        let line = item.as_str().ok_or_else(|| {
            DuckAkiError::SourceData(format!("[{SUPPORTED_SOURCE}] kline item is not a string"))
        })?;
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() != 11 {
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] expected 11 fields per kline row, got {}: '{}'",
                fields.len(),
                line
            )));
        }

        // 1: trade_date (YYYY-MM-DD)
        let trade_date = NaiveDate::parse_from_str(fields[0].trim(), "%Y-%m-%d").map_err(|e| {
            DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] invalid date '{}': {e}",
                fields[0]
            ))
        })?;


        // Reject duplicates within batch
        if !seen_dates.insert(trade_date) {
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] duplicate trade date in response: {trade_date}"
            )));
        }
        // 2: open
        let open: f64 = fields[1].trim().parse().map_err(|_| {
            DuckAkiError::SourceData(format!("[{SUPPORTED_SOURCE}] invalid open '{}'", fields[1]))
        })?;
        if !open.is_finite() {
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] non-finite open price on {trade_date}"
            )));
        }

        // 3: close
        let close: f64 = fields[2].trim().parse().map_err(|_| {
            DuckAkiError::SourceData(format!("[{SUPPORTED_SOURCE}] invalid close '{}'", fields[2]))
        })?;
        if !close.is_finite() {
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] non-finite close price on {trade_date}"
            )));
        }

        // 4: high
        let high: f64 = fields[3].trim().parse().map_err(|_| {
            DuckAkiError::SourceData(format!("[{SUPPORTED_SOURCE}] invalid high '{}'", fields[3]))
        })?;
        if !high.is_finite() {
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] non-finite high price on {trade_date}"
            )));
        }

        // 5: low
        let low: f64 = fields[4].trim().parse().map_err(|_| {
            DuckAkiError::SourceData(format!("[{SUPPORTED_SOURCE}] invalid low '{}'", fields[4]))
        })?;
        if !low.is_finite() {
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] non-finite low price on {trade_date}"
            )));
        }

        // 6: volume (hands -> shares)
        let vol_raw = fields[5].trim();
        let volume_shares = if vol_raw == "-" || vol_raw.is_empty() {
            // Source missing marker not yet confirmed by upstream evidence
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] unconfirmed volume missing marker '{vol_raw}' on {trade_date}"
            )));
        } else {
            Some(parse_raw_volume_to_shares(vol_raw)?)
        };

        // 7: amount (yuan)
        let amt_raw = fields[6].trim();
        let amount = if amt_raw == "-" || amt_raw.is_empty() {
            return Err(DuckAkiError::SourceData(format!(
                "[{SUPPORTED_SOURCE}] unconfirmed amount missing marker '{amt_raw}' on {trade_date}"
            )));
        } else {
            let val: f64 = amt_raw.parse().map_err(|_| {
                DuckAkiError::SourceData(format!(
                    "[{SUPPORTED_SOURCE}] invalid amount '{}' on {trade_date}",
                    amt_raw
                ))
            })?;
            if !val.is_finite() || val < 0.0 {
                return Err(DuckAkiError::SourceData(format!(
                    "[{SUPPORTED_SOURCE}] non-finite or negative amount on {trade_date}"
                )));
            }
            Some(val)
        };

        records.push(DailyRecord {
            symbol: req.symbol.clone(),
            trade_date,
            open,
            high,
            low,
            close,
            volume_shares,
            amount,
            currency: "CNY".to_string(),
            source: SUPPORTED_SOURCE.to_string(),
            adjustment: "none".to_string(),
            fetched_at_micros,
        });
    }

    check_deadline(deadline, "record validation")?;
    records.retain(|record| {
        record.trade_date >= req.start_date && record.trade_date <= req.end_date
    });
    check_deadline(deadline, "date filtering")?;
    Ok(records)
}

pub fn fetch_and_parse(
    req: &DailyRequest,
    fetched_at_micros: i64,
) -> Result<Vec<DailyRecord>, DuckAkiError> {
    let base_endpoint = resolve_endpoint()?;
    let url = build_url(&base_endpoint, req)?;
    let deadline = Instant::now() + request_timeout_for_url(&url);
    let body = http_get_until(&url, deadline)?;
    parse_kline_response_until(&body, req, fetched_at_micros, deadline)
}

pub(crate) fn fetch_and_validate(req: &DailyRequest) -> Result<FetchBatch, DuckAkiError> {
    let base_endpoint = resolve_endpoint()?;
    let url = build_url(&base_endpoint, req)?;
    let deadline = Instant::now() + request_timeout_for_url(&url);
    let body = http_get_until(&url, deadline)?;
    let records = parse_kline_response_until(&body, req, 0, deadline)?;
    let mut batch = FetchBatch::validate_and_create_with_deadline(records, req, 0, deadline)?;
    check_deadline(deadline, "output preparation")?;

    let fetched_at_micros = Utc::now().timestamp_micros();
    batch.fetched_at_micros = fetched_at_micros;
    for record in &mut batch.records {
        record.fetched_at_micros = fetched_at_micros;
    }
    check_deadline(deadline, "output preparation")?;
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    #[test]
    fn test_build_url() {
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let req = DailyRequest::new("sz000001", start, end, "eastmoney").unwrap();
        let url = build_url("https://example.com/api", &req).unwrap();
        assert!(url.contains("secid=0.000001"));
        assert!(url.contains("beg=20240101"));
        assert!(url.contains("end=20240110"));
        assert!(url.contains("klt=101"));
        assert!(url.contains("fqt=0"));
    }

    #[test]
    fn test_volume_conversion() {
        assert_eq!(parse_raw_volume_to_shares("12").unwrap(), 1200);
        assert_eq!(parse_raw_volume_to_shares("12.5").unwrap(), 1250);
        assert_eq!(parse_raw_volume_to_shares("0").unwrap(), 0);
        assert!(parse_raw_volume_to_shares("-1").is_err());
        assert!(parse_raw_volume_to_shares("12.555").is_err());
        assert!(parse_raw_volume_to_shares("-0.5").is_err());
        assert!(parse_raw_volume_to_shares("-0").is_err());
    }

    #[test]
    fn test_parse_g1_synthetic() {
        let json_body = r#"{
          "rc": 0,
          "data": {
            "code": "000001",
            "market": 0,
            "klines": [
              "2024-01-05,9.18,9.20,9.25,9.15,12,12000.0,1.09,0.22,0.02,0.01"
            ]
          }
        }"#;
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let req = DailyRequest::new("sz000001", start, end, "eastmoney").unwrap();
        let records = parse_kline_response(json_body, &req, 1234567890).unwrap();
        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert_eq!(r.symbol, "sz000001");
        assert_eq!(
            r.trade_date,
            NaiveDate::from_ymd_opt(2024, 1, 5).unwrap()
        );
        assert_eq!(r.open, 9.18);
        assert_eq!(r.volume_shares, Some(1200));
        assert_eq!(r.amount, Some(12000.0));
        assert_eq!(r.currency, "CNY");
        assert_eq!(r.source, "eastmoney");
        assert_eq!(r.adjustment, "none");
        assert_eq!(r.fetched_at_micros, 1234567890);
    }

    #[test]
    fn test_expired_deadline_after_json_parse_is_source_access() {
        let body = include_str!("../tests/fixtures/stock_daily.json");
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let req = DailyRequest::new("sz000001", start, end, "eastmoney").unwrap();
        let deadline = Instant::now() - Duration::from_millis(1);

        let err = parse_kline_response_until(body, &req, 0, deadline).unwrap_err();
        assert!(matches!(err, DuckAkiError::SourceAccess(_)));
    }

    #[test]
    fn test_parse_empty_klines() {
        let json_body = r#"{
          "rc": 0,
          "data": {
            "code": "000001",
            "market": 0,
            "klines": []
          }
        }"#;
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let req = DailyRequest::new("sz000001", start, end, "eastmoney").unwrap();
        let records = parse_kline_response(json_body, &req, 1234567890).unwrap();
        assert!(records.is_empty());
    }
    #[test]
    fn test_parse_stock_daily_fixture() {
        let fixture_raw = include_str!("../tests/fixtures/stock_daily.json");
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let req = DailyRequest::new("sz000001", start, end, "eastmoney").unwrap();
        let records = parse_kline_response(fixture_raw, &req, 1704441600000000).unwrap();
        assert_eq!(records.len(), 4);

        // Row 0: 2024-01-02
        assert_eq!(records[0].trade_date, NaiveDate::from_ymd_opt(2024, 1, 2).unwrap());
        assert_eq!(records[0].open, 9.18);
        assert_eq!(records[0].high, 9.22);
        assert_eq!(records[0].low, 9.10);
        assert_eq!(records[0].close, 9.15);
        assert_eq!(records[0].volume_shares, Some(1234500));
        assert_eq!(records[0].amount, Some(11300000.5));

        // Row 3: 2024-01-05
        assert_eq!(records[3].trade_date, NaiveDate::from_ymd_opt(2024, 1, 5).unwrap());
        assert_eq!(records[3].open, 9.10);
        assert_eq!(records[3].high, 9.28);
        assert_eq!(records[3].low, 9.08);
        assert_eq!(records[3].close, 9.25);
        assert_eq!(records[3].volume_shares, Some(4567800));
        assert_eq!(records[3].amount, Some(42100000.2));
    }

    #[test]
    fn test_date_filtering() {
        let fixture_raw = include_str!("../tests/fixtures/stock_daily.json");
        // Narrow window: 2024-01-03 to 2024-01-04
        let start = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 4).unwrap();
        let req = DailyRequest::new("sz000001", start, end, "eastmoney").unwrap();
        let records = parse_kline_response(fixture_raw, &req, 1704441600000000).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].trade_date, NaiveDate::from_ymd_opt(2024, 1, 3).unwrap());
        assert_eq!(records[1].trade_date, NaiveDate::from_ymd_opt(2024, 1, 4).unwrap());
    }
    #[test]
    fn test_out_of_range_rows_are_validated_before_filtering() {
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let req = DailyRequest::new("sz000001", start, end, "eastmoney").unwrap();

        let malformed = r#"{"rc":0,"data":{"code":"000001","market":0,"klines":[
            "2023-12-29,BROKEN,9.20,9.25,9.15,12,12000.0,1.09,0.22,0.02,0.01"
        ]}}"#;
        let err = parse_kline_response(malformed, &req, 0).unwrap_err();
        assert!(matches!(err, DuckAkiError::SourceData(_)));

        let duplicate = r#"{"rc":0,"data":{"code":"000001","market":0,"klines":[
            "2023-12-29,9.18,9.20,9.25,9.15,12,12000.0,1.09,0.22,0.02,0.01",
            "2023-12-29,9.18,9.20,9.25,9.15,12,12000.0,1.09,0.22,0.02,0.01"
        ]}}"#;
        let err = parse_kline_response(duplicate, &req, 0).unwrap_err();
        assert!(matches!(err, DuckAkiError::SourceData(_)));
    }


    #[test]
    fn test_duplicate_dates_rejected() {
        let json_body = r#"{
          "rc": 0,
          "data": {
            "code": "000001",
            "market": 0,
            "klines": [
              "2024-01-05,9.18,9.20,9.25,9.15,12,12000.0,1.09,0.22,0.02,0.01",
              "2024-01-05,9.18,9.20,9.25,9.15,12,12000.0,1.09,0.22,0.02,0.01"
            ]
          }
        }"#;
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 10).unwrap();
        let req = DailyRequest::new("sz000001", start, end, "eastmoney").unwrap();
        let res = parse_kline_response(json_body, &req, 1234567890);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), DuckAkiError::SourceData(_)));
    }
    #[test]
    fn test_source_errors_fixture() {
        let fixture_raw = include_str!("../tests/fixtures/source-errors.json");
        let fixture: serde_json::Value = serde_json::from_str(fixture_raw).unwrap();

        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let req = DailyRequest::new("sz000001", start, end, "eastmoney").unwrap();

        for (case_name, body) in fixture.as_object().unwrap() {
            let body_str = serde_json::to_string(body).unwrap();
            let res = parse_kline_response(&body_str, &req, 1234567890);
            assert!(
                res.is_err(),
                "Case '{case_name}' should have returned error but succeeded"
            );
            let err = res.unwrap_err();
            assert!(
                matches!(err, DuckAkiError::SourceData(_)),
                "Case '{case_name}' expected SourceData, got: {err:?}"
            );
        }
    }
    #[test]
    #[ignore]
    fn test_live_g2_eastmoney_source_verification() {
        unsafe {
            std::env::remove_var("DUCKAKI_TEST_ENDPOINT");
        }
        let fetched_at = Utc::now().timestamp_micros();

        // 1. Single day (2024-01-05)
        let d_start = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let d_end = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let req_single = DailyRequest::new("sz000001", d_start, d_end, "eastmoney").unwrap();
        let recs_single = fetch_and_parse(&req_single, fetched_at).expect("fetch single day failed");
        assert_eq!(recs_single.len(), 1);
        let r = &recs_single[0];
        assert_eq!(r.symbol, "sz000001");
        assert_eq!(r.trade_date, d_start);
        assert!(r.open > 0.0);
        assert!(r.high >= r.open.min(r.close));
        assert!(r.low <= r.open.max(r.close));
        assert!(r.volume_shares.is_some());
        assert!(r.amount.is_some());
        assert_eq!(r.currency, "CNY");
        assert_eq!(r.source, "eastmoney");
        assert_eq!(r.adjustment, "none");

        // 2. Known weekend (2024-01-06 to 2024-01-07)
        let w_start = NaiveDate::from_ymd_opt(2024, 1, 6).unwrap();
        let w_end = NaiveDate::from_ymd_opt(2024, 1, 7).unwrap();
        let req_weekend = DailyRequest::new("sz000001", w_start, w_end, "eastmoney").unwrap();
        let recs_weekend = fetch_and_parse(&req_weekend, fetched_at).expect("fetch weekend failed");
        assert_eq!(recs_weekend.len(), 0, "Weekend must return 0 records");

        // 3. Full year 2024 (2024-01-01 to 2024-12-31)
        let y_start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let y_end = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let req_year = DailyRequest::new("sz000001", y_start, y_end, "eastmoney").unwrap();
        let recs_year = fetch_and_parse(&req_year, fetched_at).expect("fetch full year failed");
        assert!(recs_year.len() >= 240 && recs_year.len() <= 245, "Expected ~242 trading days in 2024, got {}", recs_year.len());
    }
}
