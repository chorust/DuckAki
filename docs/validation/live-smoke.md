# End-to-End Live Smoke Test Report (SC-001, SC-007)

## Execution Environment
- **Date / Time**: 2026-10-21 17:00:12 (Asia/Shanghai); DuckDB smoke elapsed 0.481 s; batch `fetched_at` `2026-10-21 17:00:12.141012+08`.
- **Target DuckDB CLI**: `v1.5.5 (Variegata) d8cdaa33fd`
- **Platform**: Darwin arm64 (macOS 15.x / Darwin 25.6.0)
- **Rust Toolchain**: 1.92.0
- **Extension Name**: `duckaki` (C extension API, unstable struct ABI: `C_STRUCT_UNSTABLE`)
- **Release Binary Path**: `build/release/extension/duckaki/duckaki.duckdb_extension`
- **Release Binary SHA256**: `d193c618720da5283db0d1494df53299430e46f287f6e6c9c92ff14306c40672`
- **Commit State**: No `HEAD` commit was available; smoke used the current uncommitted workspace.

---

## 1. Full Year 2024 Execution Results

The corrected live command loaded the ordinary release extension by absolute path with DuckDB `-cmd`, then fed `tests/live/smoke.sql` to the same CLI process. The run used a unique temporary working directory so the database and Parquet output stayed outside the repository.

```sh
mkdir -p /tmp/duckaki-validation
smoke_dir="$(mktemp -d /tmp/duckaki-validation/live.XXXXXX)"
duckdb -unsigned -bail \
  -cmd "LOAD '$PWD/build/release/extension/duckaki/duckaki.duckdb_extension';" \
  "$smoke_dir/live.duckdb" < tests/live/smoke.sql
```

The CLI exited successfully with no stderr. The current output below is from this live run, not the earlier report.

### A. Capability Discovery
```text
┌─────────────┬─────────────────┬───────────┬────────────────────────────────────────────────┐
│   dataset   │  function_name  │  source   │                  description                   │
│   varchar   │     varchar     │  varchar  │                    varchar                     │
├─────────────┼─────────────────┼───────────┼────────────────────────────────────────────────┤
│ stock_daily │ aki_stock_daily │ eastmoney │ 仅 sz000001；历史日线；不复权；最多 366 自然日 │
└─────────────┴─────────────────┴───────────┴────────────────────────────────────────────────┘
```

### B. Full Year 2024 Range Materialization
- **Total Trading Days**: 242
- **Range Boundaries**: `min_date = 2024-01-02`, `max_date = 2024-12-31`
- **Record Limits**: $242 \le 366$ output records limit.
- **Fetch Batch Invariance**: Unified `fetched_at = 2026-10-21 17:00:12.141012+08` across all rows.
- **Observed 2024-01-05 row**: open `9.10`, high `9.44`, low `9.07`, close `9.27`, volume `199162200` shares, amount `1852659692.14` CNY, source `eastmoney`, adjustment `none`.

### C. Weekend Exclusion
- Querying weekend `2024-01-06` to `2024-01-07` returns strictly 0 rows. No dummy rows or weekend padding.

### D. Relational Composition
- **Watchlist JOIN**: Computed price gap (`target_price - close`) cleanly.
- **Monthly Aggregations**: Grouped all 242 trading days across all 12 calendar months, aggregating `min(low)`, `max(high)`, `avg(close)`, `sum(volume_shares)`, `sum(amount)`.

### E. Persistence & Parquet
- Exported table `persistent_smoke` to `target/smoke_2024.parquet`.
- `read_parquet('target/smoke_2024.parquet')` returned exactly 242 rows.
- In a new CLI session with `enable_external_access=false`, the persistent table returned 242 rows with the same date boundaries; a separate session without DuckAki read 242 Parquet rows.

---

## 2. Gate G3 Evaluation
The corrected live run completed the full-year materialization, discovery, JOIN, 12-month aggregation, and Parquet export/read in 0.481 seconds. New offline CLI sessions reread the persisted table and Parquet. **G3 passed for this recorded artifact and run.** This single run does not imply long-term source availability.
