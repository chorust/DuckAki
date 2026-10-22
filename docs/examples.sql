-- =====================================================================
-- DuckAki Extension SQL Usage Examples
-- Target: DuckDB v1.5.5, Darwin arm64
-- Extension: duckaki
-- =====================================================================

-- 0. Load the Extension
-- Note: Provide absolute path when loading unsigned extension locally
LOAD '/path/to/duckaki.duckdb_extension';

-- 1. Capability Discovery
-- Discover available financial datasets and their parameters offline
SELECT dataset, function_name, source, description
FROM aki_datasets();

-- 2. Direct Table Function Query
-- Query daily K-line for Ping An Bank (sz000001) over a date window
SELECT
    symbol,
    trade_date,
    open,
    high,
    low,
    close,
    volume_shares,
    amount,
    currency,
    source,
    adjustment,
    fetched_at
FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-10', source='eastmoney')
ORDER BY trade_date ASC;

-- 3. Explicit In-Memory Materialization (CTAS)
-- Cache raw fetch into a temporary table to avoid duplicate network fetches
CREATE TEMP TABLE daily_temp AS
SELECT *
FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-10', source='eastmoney');

-- 4. Joining with User Watchlist
-- Compare closing price against user target price
CREATE TEMP TABLE watchlist (
    symbol VARCHAR PRIMARY KEY,
    target_price DOUBLE
);
INSERT INTO watchlist VALUES ('sz000001', 9.50);

SELECT
    w.symbol,
    d.trade_date,
    d.close,
    w.target_price,
    ROUND(w.target_price - d.close, 2) AS price_gap
FROM daily_temp d
JOIN watchlist w ON d.symbol = w.symbol
ORDER BY d.trade_date ASC;

-- 5. Monthly Aggregations
-- Calculate monthly high, low, average close, and total volume
SELECT
    symbol,
    date_trunc('month', trade_date) AS month,
    MIN(low) AS month_low,
    MAX(high) AS month_high,
    ROUND(AVG(close), 2) AS avg_close,
    SUM(volume_shares) AS total_shares,
    ROUND(SUM(amount), 2) AS total_turnover,
    currency
FROM daily_temp
GROUP BY symbol, month, currency
ORDER BY month ASC;

-- 6. Export to Persistent Parquet File
-- Export table into Parquet format for long-term offline storage
COPY daily_temp TO 'stock_daily_sz000001_2024.parquet' (FORMAT PARQUET);

-- 7. Querying Exported Parquet File Offline
-- Works completely offline with external access disabled if permitted
SELECT
    trade_date,
    open,
    high,
    low,
    close,
    volume_shares
FROM read_parquet('stock_daily_sz000001_2024.parquet')
ORDER BY trade_date ASC;
