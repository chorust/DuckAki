# User Story 5 Validation Report: Local Table Persistence & Parquet Export

## Current Verification

`make test_offline` passed on Darwin arm64 with 18 Rust tests passed, 1 ignored, 4/4 SQLLogicTest suites successful, and 36 integration tests successful. Persistence coverage is in `TestUserStory5Persistence.test_ctas_and_parquet_export` and `test_ctas_fail_if_exists_and_replace`.

## 1. Full Snapshot Comparison

- The test materializes the four-row fixture into a persistent DuckDB table and compares each row against `tests/fixtures/expected_daily.json`.
- It compares all 12 fields, the exact schema and types, and the unified `fetched_at` value. It injects NULL for `volume_shares` and `amount` on one row to verify NULL round-tripping; this does not claim an unconfirmed Eastmoney marker maps to NULL.
- `DESCRIBE stock_archive` matches the expected schema, including `TIMESTAMP WITH TIME ZONE`.

## 2. Source Revision Does Not Rewrite Saved Data

- After saving the table and Parquet file, the test changes the upstream fixture's close, volume, and amount for one date and fetches that revision.
- A new DuckDB connection with `enable_external_access=false` rereads the database table. Both its rows and schema equal the original baseline.
- A separate DuckDB connection, without loading DuckAki, rereads Parquet and compares every row and schema with the same baseline. DuckDB's `enable_external_access=false` also blocks local filesystem reads, so that setting is not applied to the Parquet connection; it performs no DuckAki/source query, and the mock server request count remains exactly two (initial fetch plus explicit revision fetch).

## 3. Parquet Export and CTAS Conflict Semantics

- `COPY stock_archive TO ... (FORMAT PARQUET)` produces a file readable by DuckDB `read_parquet(...)`; the full rows and schema match the original snapshot.
- A CTAS attempt against an existing table fails. The test first changes a stored close to `42.0`, then verifies the failed CTAS leaves that sentinel value intact and makes no extra source request.
- `CREATE OR REPLACE TABLE` follows DuckDB's native semantics, replacing the old table with the fetched four-row fixture.
