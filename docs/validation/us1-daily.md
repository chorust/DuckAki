# User Story 1 Validation Report: Daily K-Line Query & 12-Column Verification

## Verification Environment
- **Date**: 2026-10-21
- **Platform**: Darwin arm64 (macOS 15.x / Darwin 25.6.0)
- **DuckDB CLI**: `v1.5.5 (Variegata) d8cdaa33fd`
- **Rust Toolchain**: 1.92.0
- **Source Under Test**: Eastmoney K-line data parser & DuckDB native table function

---

## 1. 12-Column Schema & Value Verification

The table function `aki_stock_daily(symbol, start_date, end_date, source='eastmoney')` emits exactly 12 columns in order:

| Col # | Column Name | Logical Type | Sample Value | Description & Constraints |
|---|---|---|---|---|
| 0 | `symbol` | `VARCHAR` | `'sz000001'` | Strict market prefix (`sz`) + 6-digit stock code. |
| 1 | `trade_date` | `DATE` | `2024-01-02` | Actual trading day. Weekends and holidays are omitted. |
| 2 | `open` | `DOUBLE` | `9.18` | Finite opening price in CNY. |
| 3 | `high` | `DOUBLE` | `9.22` | Finite highest price in CNY. |
| 4 | `low` | `DOUBLE` | `9.10` | Finite lowest price in CNY. |
| 5 | `close` | `DOUBLE` | `9.15` | Finite closing price in CNY. |
| 6 | `volume_shares` | `BIGINT` | `1234500` | Exact hand-to-share conversion (`x100`); schema permits NULL, but current parser rejects unconfirmed source missing markers. |
| 7 | `amount` | `DOUBLE` | `11300000.5` | Turnover amount in CNY; schema permits NULL, but current parser rejects unconfirmed source missing markers. |
| 8 | `currency` | `VARCHAR` | `'CNY'` | Fixed currency representation. |
| 9 | `source` | `VARCHAR` | `'eastmoney'` | Source provider identifier. |
| 10 | `adjustment` | `VARCHAR` | `'none'` | Initial delivery supports unadjusted (`none`) only. |
| 11 | `fetched_at` | `TIMESTAMP WITH TIME ZONE` | One batch-completion timestamp | Identical across all rows; assigned after fetch and full validation. |

---

## 2. Validation Test Cases & Evidence

### A. Known Sample Fixture Verification
`tests/fixtures/stock_daily.json` contains 4 trading rows. The SQL query requests `2024-01-01` through `2024-01-07`, including the weekend; only the four fixture trading dates are returned.
- Verified row count and all 12 fields against `expected_daily.json`.
- Verified exact volume conversion: `12345`, `23456`, `34567`, and `45678` hands become `1234500`, `2345600`, `3456700`, and `4567800` shares.
- Verified amounts: `11300000.5`, `21500000.0`, `31500000.75`, and `42100000.2` CNY.
- Verified all returned rows share one completion timestamp.
- Verified in `TestUserStory1StockDaily.test_stock_daily_4_days_fixture`.

### B. Date Range Sub-filtering
- Querying `2024-01-03` through `2024-01-04` from a broader valid response returns exactly those dates.
- Verified in `TestUserStory1StockDaily.test_date_filtering_subrange` and `eastmoney::tests::test_date_filtering`.

### C. Full Response Validation Before Date Filtering
- Malformed rows outside the requested range and duplicate keys outside the range fail the entire response.
- Verified in `TestUserStory1StockDaily.test_out_of_range_bad_rows_and_duplicates_fail` and `eastmoney::tests::test_out_of_range_rows_are_validated_before_filtering`.

### D. Unconfirmed Missing Markers & NULL Safety
- Empty and `'-'` volume/amount markers currently fail with `DUCKAKI_SOURCE_DATA`; no source marker is claimed as confirmed.
- The output binding and persistence path preserve explicit NULL values, tested with an internal SQL NULL projection; this does not prove an upstream missing marker is safe to map to NULL.
- Verified in `TestUserStory1StockDaily.test_unconfirmed_volume_missing_marker_raises_source_data` and the persistence round-trip test.

### E. Duplicate Date Rejection
- Duplicate dates in the full upstream batch fail before filtering and are never emitted.
- Verified in `eastmoney::tests::test_duplicate_dates_rejected` and the out-of-range duplicate integration case.

### F. SQLLogicTest Schema & Parameter Validation
- `test/sql/stock_daily.test` validates the 12-column schema and bind-time parameter rejection.
- `make test_offline` completed 18 Rust tests (1 ignored), all 4 SQLLogicTest suites, and all 36 integration tests successfully.

Offline fixture and contract regressions passed. The current Eastmoney live single-day, weekend, and full-year checks passed; see `docs/validation/g2-source.md` for the recorded run and limits.
