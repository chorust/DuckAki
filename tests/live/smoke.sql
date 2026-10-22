-- =====================================================================
-- DuckAki End-to-End Live Smoke Test (SC-001, SC-007)
-- Extension: duckaki (C extension API, unstable struct abi: C_STRUCT_UNSTABLE)
-- Target Platform: Darwin arm64, DuckDB v1.5.5
-- =====================================================================
-- Run via the quickstart command, which loads the release artifact with DuckDB -cmd.

-- 1. Capability Discovery
SELECT * FROM aki_datasets();

-- 2. Materialize 2024 Full Year Daily K-line into In-Memory Table
CREATE TEMP TABLE daily_2024 AS
SELECT *
FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-12-31', source='eastmoney');

-- Check total trading days in 2024
SELECT count(*) AS total_trading_days, min(trade_date) AS min_date, max(trade_date) AS max_date
FROM daily_2024;

-- 3. Query Single Day Verification
SELECT symbol, trade_date, open, high, low, close, volume_shares, amount, currency, source, adjustment, fetched_at
FROM daily_2024
WHERE trade_date = DATE '2024-01-05';

-- 4. Weekend Query Verification (expect strictly 0 rows)
SELECT count(*) AS weekend_rows
FROM aki_stock_daily('sz000001', DATE '2024-01-06', DATE '2024-01-07', source='eastmoney');

-- 5. Relational Composition: JOIN with Watchlist
CREATE TEMP TABLE watchlist (symbol VARCHAR PRIMARY KEY, target_price DOUBLE);
INSERT INTO watchlist VALUES ('sz000001', 10.0);

SELECT w.symbol, d.trade_date, d.close, w.target_price, round(w.target_price - d.close, 2) AS gap
FROM daily_2024 d
JOIN watchlist w ON d.symbol = w.symbol
ORDER BY d.trade_date ASC
LIMIT 5;

-- 6. Relational Composition: Monthly Summary
SELECT
    date_trunc('month', trade_date) AS month,
    round(min(low), 2) AS month_low,
    round(max(high), 2) AS month_high,
    round(avg(close), 2) AS avg_close,
    sum(volume_shares) AS total_volume_shares,
    round(sum(amount), 2) AS total_turnover_cny
FROM daily_2024
GROUP BY month
ORDER BY month ASC;

-- 7. Persistence & Parquet Export
CREATE TABLE persistent_smoke AS SELECT * FROM daily_2024;
COPY persistent_smoke TO 'target/smoke_2024.parquet' (FORMAT PARQUET);

SELECT count(*) AS parquet_read_count FROM read_parquet('target/smoke_2024.parquet');
