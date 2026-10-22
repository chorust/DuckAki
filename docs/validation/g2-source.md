# G2 Gate Validation Report: Real Source Verification (Eastmoney sz000001)

## Verification Environment
- **Date**: 2026-10-21
- **Platform**: Darwin arm64 (macOS 15.x / Darwin 25.6.0)
- **Rust Toolchain**: 1.92.0 (`rust-toolchain.toml`)
- **DuckDB CLI**: `v1.5.5 (Variegata) d8cdaa33fd`
- **HTTP/TLS Engine**: `ureq = "=3.4.2"` with Rustls (strict certificate verification, TLS 1.2/1.3)
- **Target Endpoint**: `https://push2his.eastmoney.com/api/qt/stock/kline/get`
- **Target Security**: `sz000001` (Ping An Bank Co., Ltd. / 平安银行)
- **Query Parameters**: `secid=0.000001`, `klt=101` (daily), `fqt=0` (unadjusted), `fields1=f1..f6`, `fields2=f51..f61,f116`

---

## 1. Rust Native HTTPS Source Verification

Current explicit live run after the volume-parser, response-validation, timeout, header-mapping, and timestamp repairs:

```console
$ cargo test --locked --lib test_live_g2_eastmoney_source_verification -- --ignored --nocapture
running 1 test
test eastmoney::tests::test_live_g2_eastmoney_source_verification ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 18 filtered out; finished in 1.12s
```

Run date: 2026-10-21 (Asia/Shanghai). The test cleared `DUCKAKI_TEST_ENDPOINT` and used production HTTPS for one-day, weekend, and full-year requests.

### Verified Scenarios
1. **Single Day (`2024-01-05` to `2024-01-05`)**: The Rust HTTPS test returned exactly one record for the requested date; OHLC values were positive, volume and amount were present, and currency/source/adjustment matched `CNY`/`eastmoney`/`none`.
2. **Known Weekend (`2024-01-06` to `2024-01-07`)**: The Rust HTTPS test returned zero records for the valid empty array.
3. **Full Year 2024 (`2024-01-01` to `2024-12-31`)**: The Rust HTTPS test passed its 240–245 record bound; the release CLI run returned exactly 242 rows, from `2024-01-02` through `2024-12-31`.

---

## 2. DuckDB CLI Native Extension Smoke Test

The corrected live entry was executed with the ordinary release artifact. DuckDB `-cmd` loads it by absolute path before consuming `tests/live/smoke.sql`; output files were isolated in a temporary working directory.

```sh
mkdir -p /tmp/duckaki-validation
smoke_dir="$(mktemp -d /tmp/duckaki-validation/live.XXXXXX)"
duckdb -unsigned -bail \
  -cmd "LOAD '$PWD/build/release/extension/duckaki/duckaki.duckdb_extension';" \
  "$smoke_dir/live.duckdb" < tests/live/smoke.sql
```

Current release CLI run on 2026-10-21 (Asia/Shanghai) completed in 0.481 s: 242 rows for 2024-01-01–2024-12-31, boundaries 2024-01-02–2024-12-31, and zero weekend rows for 2024-01-06–2024-01-07. The observed 2024-01-05 row had close 9.27, volume 199162200 shares, amount CNY 1852659692.14, `source='eastmoney'`, and `adjustment='none'`; batch `fetched_at` was `2026-10-21 17:00:12.141012+08`.

The current run produced 12 monthly groups and read 242 rows from Parquet. After the live CLI exited, a new CLI session with external access disabled read 242 persistent-table rows (2024-01-02–2024-12-31); a separate CLI session without DuckAki read 242 Parquet rows. Release artifact SHA-256: `d193c618720da5283db0d1494df53299430e46f287f6e6c9c92ff14306c40672`. No `HEAD` commit was available; the artifact was built from the current uncommitted workspace.

### Earlier CLI Output (recorded at 15:23; superseded by the current run summary above)

#### 1. Capability Discovery (`aki_datasets()`)
```text
┌─────────────┬─────────────────┬───────────┬────────────────────────────────────────────────┐
│   dataset   │  function_name  │  source   │                  description                   │
│   varchar   │     varchar     │  varchar  │                    varchar                     │
├─────────────┼─────────────────┼───────────┼────────────────────────────────────────────────┤
│ stock_daily │ aki_stock_daily │ eastmoney │ 仅 sz000001；历史日线；不复权；最多 366 自然日 │
└─────────────┴─────────────────┴───────────┴────────────────────────────────────────────────┘
```

#### 2. Single Trading Day (`2024-01-05`)
```text
┌──────────┬────────────┬────────┬────────┬────────┬────────┬───────────────┬───────────────┬──────────┬───────────┬────────────┬───────────────────────────────┐
│  symbol  │ trade_date │  open  │  high  │  low   │ close  │ volume_shares │    amount     │ currency │  source   │ adjustment │          fetched_at           │
│ varchar  │    date    │ double │ double │ double │ double │     int64     │    double     │ varchar  │  varchar  │  varchar   │   timestamp with time zone    │
├──────────┼────────────┼────────┼────────┼────────┼────────┼───────────────┼───────────────┼──────────┼───────────┼────────────┼───────────────────────────────┤
│ sz000001 │ 2024-01-05 │    9.1 │   9.44 │   9.07 │   9.27 │     199162200 │ 1852659692.14 │ CNY      │ eastmoney │ none       │ 2026-10-21 15:23:33.853459+08 │
└──────────┴────────────┴────────┴────────┴────────┴────────┴───────────────┴───────────────┴──────────┴───────────┴────────────┴───────────────────────────────┘
```

#### 3. Weekend Filter Verification (`2024-01-06` to `2024-01-07`)
```text
┌──────────────┐
│ weekend_rows │
│    int64     │
├──────────────┤
│            0 │
└──────────────┘
```

#### 4. Multi-Day Trading Window (`2024-01-01` to `2024-01-10`)
```text
┌──────────┬────────────┬────────┬────────┬────────┬────────┬───────────────┬───────────────┐
│  symbol  │ trade_date │  open  │  high  │  low   │ close  │ volume_shares │    amount     │
│ varchar  │    date    │ double │ double │ double │ double │     int64     │    double     │
├──────────┼────────────┼────────┼────────┼────────┼────────┼───────────────┼───────────────┤
│ sz000001 │ 2024-01-02 │   9.39 │   9.42 │   9.21 │   9.21 │     115836600 │ 1075742252.45 │
│ sz000001 │ 2024-01-03 │   9.19 │   9.22 │   9.15 │    9.2 │      73361000 │  673673613.54 │
│ sz000001 │ 2024-01-04 │   9.19 │   9.19 │   9.08 │   9.11 │      86419400 │  787470082.37 │
│ sz000001 │ 2024-01-05 │    9.1 │   9.44 │   9.07 │   9.27 │     199162200 │ 1852659692.14 │
│ sz000001 │ 2024-01-08 │   9.23 │    9.3 │   9.11 │   9.15 │     112115600 │ 1029006548.73 │
│ sz000001 │ 2024-01-09 │   9.16 │   9.19 │   9.09 │   9.18 │      76619400 │   700375720.2 │
│ sz000001 │ 2024-01-10 │   9.16 │   9.19 │   9.09 │   9.09 │      85862100 │  783771150.01 │
└──────────┴────────────┴────────┴────────┴────────┴────────┴───────────────┴───────────────┘
```

---

## 3. Boundary & Protection Audits
- **Headers bounded**: Response headers verified $< 32\text{ KiB}$.
- **Body bounded**: Full year response size verified $< 64\text{ KiB}$ (well within $1\text{ MiB}$).
- **Single HTTP Request**: No application-level retries or pagination loops.
- **TLS verification**: Strict platform CA bundle via Rustls; MITM or plaintext rejected.
- **G2 Verification Conclusion**: G2 passed on the current explicit Rust source checks and release CLI full-year smoke. The corrected command preloads the extension with an absolute path; no long-term source availability is implied.
