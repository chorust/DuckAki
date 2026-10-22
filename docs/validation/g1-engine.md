# G1 Gate Validation Report: Engine Safety & Host Boundary

## Verification Environment
- **Date**: 2026-10-21
- **Platform**: Darwin arm64 (macOS 15.x / Darwin 25.6.0)
- **Rust Toolchain**: 1.92.0 (`rust-toolchain.toml`)
- **DuckDB CLI**: `v1.5.5 (Variegata) d8cdaa33fd`
- **DuckDB Python**: `duckdb==1.5.5` with Python 3.11 (`/usr/local/bin/python3.11`)
- **Git Commit**: Initial commit / working tree on branch `master`
- **Extension Name**: `duckaki` (C extension API, unstable struct abi: `C_STRUCT_UNSTABLE`)

---

## 1. Extension Build & Binary Metadata
Extension debug build with metadata footer was generated via `make debug`:
- Extension binary: `build/debug/extension/duckaki/duckaki.duckdb_extension`
- Metadata fields:
  - Header signature: `4`
  - DuckDB platform: `osx_arm64`
  - DuckDB version: `v1.5.5`
  - Extension version: `v0.1.0`
  - ABI type: `C_STRUCT_UNSTABLE`

Initial debug-build CLI probe (before G2 registered the delivered source; current one-row catalog output is recorded in `live-smoke.md`):
```console
$ duckdb -unsigned -c "LOAD '$(pwd)/build/debug/extension/duckaki/duckaki.duckdb_extension'; SELECT * FROM aki_datasets();"
┌─────────┬───────────────┬─────────┬─────────────┐
│ dataset │ function_name │ source  │ description │
│ varchar │    varchar    │ varchar │   varchar   │
└─────────┴───────────────┴─────────┴─────────────┘
                      0 rows
```

Schema reflection verification via `DESCRIBE`:
```console
$ duckdb -unsigned -c "LOAD '$(pwd)/build/debug/extension/duckaki/duckaki.duckdb_extension'; DESCRIBE SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
┌────────────────────────────────────────┐
│                Describe                │
│                                        │
│ symbol        varchar                  │
│ trade_date    date                     │
│ open          double                   │
│ high          double                   │
│ low           double                   │
│ close         double                   │
│ volume_shares bigint                   │
│ amount        double                   │
│ currency      varchar                  │
│ source        varchar                  │
│ adjustment    varchar                  │
│ fetched_at    timestamp with time zone │
└────────────────────────────────────────┘
```

---

## 2. G1 Host Boundary Assertions

### A. Zero Network During Bind Phase
- `DESCRIBE SELECT * FROM aki_stock_daily(...)` executes entirely within the bind phase and sends 0 network requests.
- `PREPARE p1 AS SELECT * FROM aki_stock_daily(...)` executes entirely within the bind phase and sends 0 network requests.
- Verified in `TestG1EngineSafety.test_zero_network_during_bind_phase`.

### B. `enable_external_access=false` Configuration Boundary
- Disabling external access (`SET enable_external_access=false;`) blocks `aki_stock_daily(...)` at bind time with error `DUCKAKI_EXTERNAL_ACCESS_DISABLED: external access is disabled in DuckDB configuration`.
- `aki_datasets()` remains 100% offline and succeeds even when external access is disabled.
- Verified in `TestG1EngineSafety.test_external_access_disabled_blocks_network_call`.

### C. Prepared Statement Re-check Boundary
- Preparing a statement while external access is allowed succeeds at bind time.
- If external access is disabled after bind (`SET enable_external_access=false;`), executing the prepared statement (`EXECUTE p1;`) is blocked at execution start with `DUCKAKI_EXTERNAL_ACCESS_DISABLED: external access was disabled prior to execution`.
- 0 network requests are dispatched to upstream.
- Verified in `TestG1EngineSafety.test_prepare_then_disable_external_access_blocks_execution`.

### D. In-Flight Interrupt Handling
- Cancellation at headers, body, and output surfaces DuckDB `InterruptException`; the host connection remains usable.
- Verified in `TestG1EngineSafety.test_in_flight_interrupt_covers_headers_body_and_output`.

### E. Host Resilience
- After any query failure (parameter error, network error, upstream error), the DuckDB connection remains completely usable for subsequent queries (e.g. `SELECT 1`).
- Verified in `TestG1EngineSafety.test_host_resilience_after_error`.

---

## 3. Current Automated Test Suite

Latest `make test_offline` result, recorded in `docs/validation/acceptance.md`:

- Rust library tests: 18 passed, 1 ignored, 0 failed.
- SQLLogicTest: all 4 suites passed.
- Python integration tests: 36 passed in 5.046 s.

G1 host-boundary checks pass in the current offline suite. Current release-artifact load and live-source evidence are recorded in `g2-source.md` and `live-smoke.md`.
