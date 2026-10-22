# DuckAki v0.1.0 Formal Acceptance & Verification Summary

## 1. Quality Gates (G1–G4) Summary

| Gate | Description | Status | Evidence Document |
|---|---|---|---|
| **G1** | Engine safety and host security boundary: zero-network bind, external-access rejection, PREPARE security, cancellation, host resilience | **PASSED — current offline suite** | [`docs/validation/g1-engine.md`](g1-engine.md), `make test_offline` |
| **G2** | Real Eastmoney source verification: `sz000001`, unadjusted daily HTTPS, weekend empty response, full-year run | **PASSED — current live checks** | [`docs/validation/g2-source.md`](g2-source.md): Rust single-day/weekend/full-year checks and release CLI full-year smoke |
| **G3** | Full-year native workflow: materialization, discovery, JOIN, monthly aggregation, Parquet export and reread | **PASSED — current live and offline rereads** | [`docs/validation/live-smoke.md`](live-smoke.md) |
| **G4** | Distribution readiness: attribution, Apache-2.0 license, no bundled data | **RECORDED PASS** | [`docs/validation/distribution.md`](distribution.md); unchanged by this repair |

**Spec 001 acceptance passed on Darwin arm64 / DuckDB v1.5.5 for the recorded artifact and run.** The live-source result is a point-in-time check, not a long-term availability guarantee; unconfirmed missing markers still fail closed.

---

## 2. Functional Requirements (FR-001–FR-022)

| ID | Requirement Description | Verification Evidence |
|---|---|---|
| FR-001 | Table function `aki_stock_daily(symbol, start_date, end_date, source=...)` | `test/sql/host.test`, `test_sql.py` |
| FR-002 | Table function `aki_datasets()` returning 4 columns | `test/sql/datasets.test`, `test_sql.py` |
| FR-003 | Named parameter `source` is required and mandatory | `test/sql/stock_daily.test` |
| FR-004 | Support `source='eastmoney'` | `test_sql.py`, `tests/live/smoke.sql` |
| FR-005 | Support strictly symbol `sz000001` | `src/core.rs`, `test_sql.py` |
| FR-006 | Inclusive date range `[start_date, end_date]` | `src/eastmoney.rs`, `test_sql.py` |
| FR-007 | Maximum date range $\le 366$ natural days | `src/core.rs`, `test/sql/stock_daily.test` |
| FR-008 | Output 12 columns in specified order and types | `test/sql/stock_daily.test`, `us1-daily.md` |
| FR-009 | Volume conversion: hands to shares ($x100 \rightarrow \text{BIGINT}$) | `src/eastmoney.rs`, `test_sql.py` |
| FR-010 | Amount in CNY as `DOUBLE` | `src/eastmoney.rs`, `test_sql.py` |
| FR-011 | Unadjusted daily bars (`adjustment='none'`) | `src/eastmoney.rs`, `test_sql.py` |
| FR-012 | Unified batch timestamp `fetched_at` as `TIMESTAMP_TZ` | `test_sql.py`, `us1-daily.md` |
| FR-013 | Bind phase performs zero network access | `test_sql.py` (request count == 0) |
| FR-014 | `SET enable_external_access=false` blocks `aki_stock_daily` | `test/sql/stock_daily.test`, `test_sql.py` |
| FR-015 | `aki_datasets()` remains 100% accessible offline | `test/sql/datasets.test`, `test_sql.py` |
| FR-016 | Re-check external access prior to execution (prepared statement safety) | `test_sql.py` (`test_prepare_then_disable_external_access`) |
| FR-017 | Bounded HTTP client: 30-second monotonic request deadline, 5-second DNS/connect limits, 32 KiB headers, 1 MiB body | `src/eastmoney.rs`, `test_sql.py`; DNS-specific delay is not injected offline |
| FR-018 | Zero application-level retries or endpoint switching | `test_sql.py` (`test_source_access_http_403_and_no_retry`) |
| FR-019 | Host cancellation surfaces `InterruptException` cleanly at headers, body, and output | `test_sql.py` (`test_in_flight_interrupt_covers_headers_body_and_output`) |
| FR-020 | Materialization via standard `CREATE TABLE AS SELECT` (CTAS) | `test_sql.py`, `tests/live/smoke.sql` |
| FR-021 | Export to Parquet via standard DuckDB `COPY ... TO ... (FORMAT PARQUET)` | `test_sql.py`, `tests/live/smoke.sql` |
| FR-022 | Query persisted tables offline in fresh DuckDB session | `test_sql.py`, `us5-persistence.md` |

---

## 3. Edge Conditions (EC-001–EC-013)

| EC ID | Contract condition from `spec.md` | Current status and evidence |
|---|---|---|
| EC-001 | 日期倒置、非法、缺失，或来源、标的缺失及不支持的市场，必须在访问来源前失败；开始日等于结束日是合法单日范围。 | **PASS OFFLINE** — binder and zero-request checks in `test_sql.py` and `test/sql/stock_daily.test`. |
| EC-002 | 周末、节假日、停牌或区间外无记录，仅在来源明确返回合法无数据时返回零行；不凭含糊响应猜测“无交易”。 | **PASS** — offline empty-array tests and current live weekend query both returned zero rows for an explicit valid empty response. |
| EC-003 | 上游边界规则不同或返回超出请求范围的记录，最终结果仍严格遵守两端包含的用户范围。 | **PASS OFFLINE** — valid subrange filtering plus malformed and duplicate out-of-range response tests. |
| EC-004 | 来源量额缺失可保留缺失；OHLC 等必需字段损坏、非有限数值或类型无法解释必须失败，不用零掩盖。 | **PASS FOR CURRENT SOURCE DATA** — tested source responses contained no missing amount/volume markers; malformed values fail, unknown markers are rejected, and nullable output values round-trip. |
| EC-005 | 无法确认成交量的原始单位或股数换算，不将该来源列为标准日线的受支持来源。 | **PASS FOR CURRENT SOURCE** — fixture conversion and current Eastmoney live output agree with the documented hands-to-shares conversion. |
| EC-006 | 同一获取批次内，来源、标的、交易日与复权口径的组合重复，即使数值相同也作为数据质量错误，不静默去重。 | **PASS OFFLINE** — duplicate dates fail in the complete response, including outside the requested range. |
| EC-007 | 超时、限流、拒绝访问、结构变化或分段获取中断，都不得退化为成功空表、完整结果声明或自动切源。 | **PASS OFFLINE AND LIVE** — HTTP/TLS fault injection, slow-drip timeout, truncation, error propagation, no-retry, cancellation tests, and current live parsing pass. |
| EC-008 | 超过已声明的等待、响应大小或记录数量上限时失败，并说明原因；不无限重试、不返回被截断的成功结果。 | **PASS OFFLINE WITH DNS TEST LIMITATION** — HTTP deadline, header/body bounds and error mapping pass; five-second resolver timeout is configured but no DNS stall is injected. |
| EC-009 | 原来源离线或数据修订不改变已经保存的快照；不同获取批次的获取时间不能当作行情发生时间。 | **PASS OFFLINE** — exact table/Parquet round-trip after source revision; one completion timestamp is assigned per fetched batch. |
| EC-010 | 用户未显式排序时不保证结果顺序；限制输出行数不保证限制来源请求量；视图不等于保存快照。 | **PASS BY CONTRACT** — `docs/spec.md`, quickstart semantics, and composition tests state and preserve standard DuckDB behavior. |
| EC-011 | 后续来源口径无法满足现有合同，不能仅更换来源名称后塞入同一个数据集；需使用独立、说明差异的数据集。 | **PASS CURRENT SCOPE** — only Eastmoney `stock_daily` is delivered; unsupported source names fail. |
| EC-012 | 用户取消或保存失败时，不把未完成操作报告为成功，不隐式改写已有快照；具体保存覆盖行为遵循所用分析环境。 | **PASS OFFLINE** — three cancellation points and failed CTAS sentinel-preservation test pass; replace behavior follows DuckDB semantics. |
| EC-013 | 未交付的周期、复权选项、来源或数据集不出现在可用能力与成功示例中；尝试使用未支持能力时不得隐式转成另一种查询。 | **PASS CURRENT SCOPE** — static capability list contains only the delivered daily Eastmoney combination; unsupported source is rejected. |

---

## 4. Success Criteria (SC-001–SC-007)

| SC ID | Original criterion from `spec.md` | Current status and evidence |
|---|---|---|
| SC-001 | 用户能够对至少 1 个受支持标的、1 个明确来源和至少 1 个完整自然年的请求区间完成读取、关联汇总、保存重读的端到端流程，需要额外编写的抓取或格式转换脚本数量为 0。 | **PASS LIVE** — current release CLI run completed full-year read, JOIN, monthly aggregation, table/Parquet saves and new-session rereads; 242 rows, 2024-01-02–2024-12-31. |
| SC-002 | 在包含至少 2 个已知交易日、1 个非交易日及已知量额单位的受控验收样本中，预期记录、字段与换算结果 100% 相符；非交易日伪造记录、范围外记录和重复键数量均为 0。 | **PASS OFFLINE** — Jan 1–7 query returns the four expected fixture rows; exact 12 fields, unit conversion, out-of-range filtering and full-response failures are covered. |
| SC-003 | 在常规本地验收环境中，用户断网查看首版能力目录连续 5 次，每次均在 2 秒内看到完整清单；所列组合与实际交付范围 100% 一致，网络访问次数为 0。 | **PASS OFFLINE** — `test_datasets_five_consecutive_runs_timing_and_zero_requests`. |
| SC-004 | 参数无效、合法空结果、网络失败、限流、拒绝访问、损坏响应、重复记录和分段获取中断这 8 类验收情形全部呈现约定状态；访问来源前拒绝的非法输入、有限等待及不自动切源的行为全部符合约定。 | **PASS OFFLINE** — `docs/validation/us2-errors.md`; includes TLS, slow-drip, header-limit and three cancellation phases. DNS stall is not fault-injected. |
| SC-005 | 对一份已知快照完成一次本地信息关联和一次按月、币种汇总，结果均与预期 100% 相符；随后断网重复分析 3 次，原来源访问次数保持为 0。 | **PASS OFFLINE** — fixture JOIN, CNY monthly totals, synthetic Jan/Feb totals, and three CTAS reads with unchanged request count. |
| SC-006 | 同一快照分别保存为持久表和文件，在新会话离线重读后，记录数量、价格、量额、缺失值及全部追溯信息与原快照 100% 一致；来源后续修订导致的隐式快照改写次数为 0。 | **PASS OFFLINE** — row-by-row 12-field/type checks for table and Parquet, source revision immutability, offline database session, and preserved NULL values. |
| SC-007 | 按公开示例完成一次首版支持来源的真实读取，记录来源、时间、参数和结果；至少 1 次成功读取有可追溯证据，同时说明该结果不代表长期可用性保证。 | **PASS LIVE** — current Rust single-day/weekend/full-year checks and release CLI run are recorded in `g2-source.md` and `live-smoke.md`; no long-term guarantee is claimed. |

---

## 5. Current Test and Smoke Verification

```console
$ make test_offline
cargo test --locked --features test-support --lib
test result: ok. 18 passed; 0 failed; 1 ignored

[1/4] test/sql/datasets.test SUCCESS
[2/4] test/sql/host.test SUCCESS
[3/4] test/sql/stock_daily.test SUCCESS
[4/4] test/sql/snapshots.test SUCCESS

Ran 36 tests in 5.046s
OK
```

`make release` built the artifact, and its absolute-path `-cmd` load was verified in DuckDB CLI. The current live smoke completed in 0.481 s with 242 full-year rows and a zero-row weekend result; new offline sessions reread the persistent table and Parquet. Artifact SHA-256 and the unavailable `HEAD` commit state are recorded in `g2-source.md` and `live-smoke.md`.
