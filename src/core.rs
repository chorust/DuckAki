use chrono::NaiveDate;
use std::collections::HashSet;
use std::time::Instant;
use std::fmt;

/// Maximum natural days allowed in a single request (inclusive).
pub const MAX_DATE_RANGE_DAYS: i64 = 366;
/// Maximum raw records allowed from source response.
pub const MAX_RAW_RECORDS: usize = 4096;
/// Maximum output records allowed.
pub const MAX_OUTPUT_RECORDS: usize = 366;
/// Supported symbol in version 0.1.
pub const SUPPORTED_SYMBOL: &str = "sz000001";
/// Supported source in version 0.1.
pub const SUPPORTED_SOURCE: &str = "eastmoney";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuckAkiError {
    InvalidArgument(String),
    ExternalAccessDisabled(String),
    SourceAccess(String),
    SourceData(String),
    LimitExceeded(String),
    Internal(String),
}

impl fmt::Display for DuckAkiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DuckAkiError::InvalidArgument(msg) => write!(f, "DUCKAKI_INVALID_ARGUMENT: {msg}"),
            DuckAkiError::ExternalAccessDisabled(msg) => {
                write!(f, "DUCKAKI_EXTERNAL_ACCESS_DISABLED: {msg}")
            }
            DuckAkiError::SourceAccess(msg) => write!(f, "DUCKAKI_SOURCE_ACCESS: {msg}"),
            DuckAkiError::SourceData(msg) => write!(f, "DUCKAKI_SOURCE_DATA: {msg}"),
            DuckAkiError::LimitExceeded(msg) => write!(f, "DUCKAKI_LIMIT_EXCEEDED: {msg}"),
            DuckAkiError::Internal(msg) => write!(f, "DUCKAKI_INTERNAL: {msg}"),
        }
    }
}

impl std::error::Error for DuckAkiError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyRequest {
    pub symbol: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub source: String,
}

impl DailyRequest {
    pub fn new(
        symbol: impl Into<String>,
        start_date: NaiveDate,
        end_date: NaiveDate,
        source: impl Into<String>,
    ) -> Result<Self, DuckAkiError> {
        let symbol = symbol.into();
        let source = source.into();

        if source.is_empty() {
            return Err(DuckAkiError::InvalidArgument(
                "source parameter is required and cannot be empty".to_string(),
            ));
        }
        if source != SUPPORTED_SOURCE {
            return Err(DuckAkiError::InvalidArgument(format!(
                "unsupported source '{source}', currently only '{SUPPORTED_SOURCE}' is supported"
            )));
        }

        if symbol.is_empty() {
            return Err(DuckAkiError::InvalidArgument(
                "symbol parameter is required and cannot be empty".to_string(),
            ));
        }
        if symbol != SUPPORTED_SYMBOL {
            return Err(DuckAkiError::InvalidArgument(format!(
                "unsupported symbol '{symbol}', currently only '{SUPPORTED_SYMBOL}' is supported"
            )));
        }

        if start_date > end_date {
            return Err(DuckAkiError::InvalidArgument(format!(
                "start_date ({start_date}) must be <= end_date ({end_date})"
            )));
        }

        let days = (end_date - start_date).num_days() + 1;
        if days > MAX_DATE_RANGE_DAYS {
            return Err(DuckAkiError::LimitExceeded(format!(
                "date range from {start_date} to {end_date} exceeds {MAX_DATE_RANGE_DAYS} days limit: got {days} days"
            )));
        }

        Ok(Self {
            symbol,
            start_date,
            end_date,
            source,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DailyRecord {
    pub symbol: String,
    pub trade_date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume_shares: Option<i64>,
    pub amount: Option<f64>,
    pub currency: String,
    pub source: String,
    pub adjustment: String,
    pub fetched_at_micros: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchBatch {
    pub records: Vec<DailyRecord>,
    pub fetched_at_micros: i64,
}

impl FetchBatch {
    pub fn validate_and_create(
        records: Vec<DailyRecord>,
        request: &DailyRequest,
        fetched_at_micros: i64,
    ) -> Result<Self, DuckAkiError> {
        Self::validate_and_create_inner(records, request, fetched_at_micros, None)
    }

    pub fn validate_and_create_with_deadline(
        records: Vec<DailyRecord>,
        request: &DailyRequest,
        fetched_at_micros: i64,
        deadline: Instant,
    ) -> Result<Self, DuckAkiError> {
        Self::validate_and_create_inner(records, request, fetched_at_micros, Some(deadline))
    }

    fn validate_and_create_inner(
        records: Vec<DailyRecord>,
        request: &DailyRequest,
        fetched_at_micros: i64,
        deadline: Option<Instant>,
    ) -> Result<Self, DuckAkiError> {
        let mut seen_keys = HashSet::new();
        let mut filtered = Vec::new();

        for record in records {
            if deadline.is_some_and(|limit| Instant::now() >= limit) {
                return Err(DuckAkiError::SourceAccess(format!(
                    "[{}] request deadline exceeded during batch validation",
                    request.source
                )));
            }

            // Check non-finite numbers
            if !record.open.is_finite()
                || !record.high.is_finite()
                || !record.low.is_finite()
                || !record.close.is_finite()
            {
                return Err(DuckAkiError::SourceData(format!(
                    "[{}] non-finite price encountered on {}",
                    request.source, record.trade_date
                )));
            }
            if let Some(amt) = record.amount {
                if !amt.is_finite() || amt < 0.0 {
                    return Err(DuckAkiError::SourceData(format!(
                        "[{}] invalid amount {} on {}",
                        request.source, amt, record.trade_date
                    )));
                }
            }
            if let Some(vol) = record.volume_shares {
                if vol < 0 {
                    return Err(DuckAkiError::SourceData(format!(
                        "[{}] negative volume {} on {}",
                        request.source, vol, record.trade_date
                    )));
                }
            }

            // Key uniqueness check: (symbol, trade_date, source, adjustment)
            let key = (
                record.symbol.clone(),
                record.trade_date,
                record.source.clone(),
                record.adjustment.clone(),
            );
            if !seen_keys.insert(key) {
                return Err(DuckAkiError::SourceData(format!(
                    "[{}] duplicate record key for {} on {}",
                    request.source, record.symbol, record.trade_date
                )));
            }

            // Filter within user requested range
            if record.trade_date >= request.start_date && record.trade_date <= request.end_date {
                filtered.push(record);
            }
        }
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            return Err(DuckAkiError::SourceAccess(format!(
                "[{}] request deadline exceeded during batch validation",
                request.source
            )));
        }

        if filtered.len() > MAX_OUTPUT_RECORDS {
            return Err(DuckAkiError::LimitExceeded(format!(
                "output record count {} exceeds maximum {}",
                filtered.len(),
                MAX_OUTPUT_RECORDS
            )));
        }

        Ok(Self {
            records: filtered,
            fetched_at_micros,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub dataset: &'static str,
    pub function_name: &'static str,
    pub source: &'static str,
    pub description: &'static str,
}

// Initial delivered capabilities list: empty before G2 source verification in normal build.
// When G2 is verified (T020), this will be populated with stock_daily/eastmoney.
#[cfg(not(feature = "delivered-eastmoney"))]
pub static DELIVERED_CAPABILITIES: &[Capability] = &[];

#[cfg(feature = "delivered-eastmoney")]
pub static DELIVERED_CAPABILITIES: &[Capability] = &[Capability {
    dataset: "stock_daily",
    function_name: "aki_stock_daily",
    source: "eastmoney",
    description: "仅 sz000001；历史日线；不复权；最多 366 自然日",
}];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_requests() {
        // Single day
        let d = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let req = DailyRequest::new("sz000001", d, d, "eastmoney").unwrap();
        assert_eq!(req.symbol, "sz000001");
        assert_eq!(req.source, "eastmoney");

        // Leap year 366 days
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        assert_eq!((end - start).num_days() + 1, 366);
        assert!(DailyRequest::new("sz000001", start, end, "eastmoney").is_ok());
    }

    #[test]
    fn test_date_exceeds_366_days() {
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
        let err = DailyRequest::new("sz000001", start, end, "eastmoney").unwrap_err();
        assert!(matches!(err, DuckAkiError::LimitExceeded(_)));
        assert!(err.to_string().starts_with("DUCKAKI_LIMIT_EXCEEDED:"));
    }

    #[test]
    fn test_inverted_dates() {
        let start = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let err = DailyRequest::new("sz000001", start, end, "eastmoney").unwrap_err();
        assert!(matches!(err, DuckAkiError::InvalidArgument(_)));
        assert!(err.to_string().starts_with("DUCKAKI_INVALID_ARGUMENT:"));
    }

    #[test]
    fn test_unsupported_symbol() {
        let d = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let err = DailyRequest::new("sh600000", d, d, "eastmoney").unwrap_err();
        assert!(matches!(err, DuckAkiError::InvalidArgument(_)));
        assert!(err.to_string().contains("sz000001"));
    }

    #[test]
    fn test_unsupported_source() {
        let d = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let err = DailyRequest::new("sz000001", d, d, "akshare").unwrap_err();
        assert!(matches!(err, DuckAkiError::InvalidArgument(_)));
        assert!(err.to_string().contains("eastmoney"));
    }

    #[test]
    fn test_empty_source_and_symbol() {
        let d = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        assert!(DailyRequest::new("", d, d, "eastmoney").is_err());
        assert!(DailyRequest::new("sz000001", d, d, "").is_err());
    }

    #[test]
    fn test_expired_batch_deadline_is_source_access() {
        let trade_date = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        let request = DailyRequest::new("sz000001", trade_date, trade_date, "eastmoney").unwrap();
        let record = DailyRecord {
            symbol: "sz000001".to_string(),
            trade_date,
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume_shares: Some(0),
            amount: Some(0.0),
            currency: "CNY".to_string(),
            source: "eastmoney".to_string(),
            adjustment: "none".to_string(),
            fetched_at_micros: 0,
        };
        let deadline = std::time::Instant::now() - std::time::Duration::from_millis(1);

        let err = FetchBatch::validate_and_create_with_deadline(vec![record], &request, 0, deadline)
            .unwrap_err();
        assert!(matches!(err, DuckAkiError::SourceAccess(_)));
    }

    #[test]
    fn test_capabilities_configuration() {
        #[cfg(not(feature = "delivered-eastmoney"))]
        assert!(DELIVERED_CAPABILITIES.is_empty());
        #[cfg(feature = "delivered-eastmoney")]
        {
            assert_eq!(DELIVERED_CAPABILITIES.len(), 1);
            assert_eq!(DELIVERED_CAPABILITIES[0].dataset, "stock_daily");
            assert_eq!(DELIVERED_CAPABILITIES[0].function_name, "aki_stock_daily");
            assert_eq!(DELIVERED_CAPABILITIES[0].source, "eastmoney");
        }
    }
}
