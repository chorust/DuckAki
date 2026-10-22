# User Story 4 Validation Report: SQL Composition (JOIN, Aggregation, Materialization)

## Verification Overview
This report documents the verification that DuckAki integrates seamlessly with DuckDB's relational SQL features without introducing proprietary caching, bespoke aggregation engines, or hidden table mutations.

---

## 1. Relational Capabilities Verified

### A. Materialization via CTAS (Create Table As Select)
- Materializing `aki_stock_daily` into `CREATE TEMP TABLE daily_temp AS SELECT ...` executes exactly 1 scan.
- Subsequent queries on `daily_temp` query memory directly without contacting the network.
- Verified in `TestUserStory4SQLComposition.test_ctas_snapshot_offline_repeated_queries`.
- Querying `daily_temp` 3 consecutive times with upstream server disconnected or in 500 error state resulted in strictly 0 new network requests (total request count remained 1).

### B. Relational JOINs
- Joining `aki_stock_daily` (or materialized snapshot) against a user-defined watchlist table (`watchlist(symbol, target_price)`) evaluates correctly.
- Price difference `w.target_price - d.close` computes with exact precision.
- Verified in `TestUserStory4SQLComposition.test_join_with_watchlist` and `test/sql/snapshots.test`.

### C. Aggregations & Grouping
- Grouping by `symbol`, `date_trunc('month', trade_date)`, and `currency` returns the expected monthly sums. The source fixture yields January CNY amount `106400001.45`; the SQLLogicTest synthetic snapshot yields January `12000.00` and February `22000.00` CNY.
- Verified in `TestUserStory4SQLComposition.test_aggregation_and_window` and `test/sql/snapshots.test`.

### D. Zero Implicit Side Effects
- Queries on `aki_stock_daily` produce zero tables or schemas in DuckDB's catalog.
- Verified via `information_schema.tables` before and after query execution in `TestUserStory4SQLComposition.test_read_only_query_does_not_create_tables_or_cache`.
- Views defined over `aki_stock_daily` re-invoke the table function upon execution.
