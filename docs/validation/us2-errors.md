# User Story 2 Validation Report: SC-004 Error Classification Taxonomy

## Verification Overview
This report documents the verification of the 8 distinct error classes and host boundary conditions defined by SC-004:
1. `DUCKAKI_INVALID_ARGUMENT`: Parameter missing, unsupported, or inverted.
2. `DUCKAKI_EXTERNAL_ACCESS_DISABLED`: External access blocked by DuckDB host config.
3. `DUCKAKI_SOURCE_ACCESS`: Network failure, HTTP non-2xx status, blocked redirects.
4. `DUCKAKI_SOURCE_DATA`: Corrupted envelope, missing required fields, non-finite values, unconfirmed missing markers.
5. `DUCKAKI_LIMIT_EXCEEDED`: Date range > 366 days, response body > 1 MiB.
6. `DUCKAKI_INTERNAL`: Unexpected system / invariant error.
7. `INTERRUPT`: Host-driven query cancellation cleanly surfacing `InterruptException`.
8. `EMPTY_RESULT`: Valid empty dataset returns 0 rows with invariant 12-column schema.

---

## Error Classification Test Matrix

| SC-004 Error Category | Test Scenario | Trigger Condition | Observable Result |
|---|---|---|---|
| `DUCKAKI_INVALID_ARGUMENT` | Symbol without prefix | `'000001'` | Binder Error `DUCKAKI_INVALID_ARGUMENT: unsupported symbol '000001'` |
| `DUCKAKI_INVALID_ARGUMENT` | Unsupported symbol | `'sh600519'` | Binder Error `DUCKAKI_INVALID_ARGUMENT: unsupported symbol 'sh600519'` |
| `DUCKAKI_INVALID_ARGUMENT` | Inverted dates | start `2024-01-10`, end `2024-01-05` | Binder Error `DUCKAKI_INVALID_ARGUMENT: start_date must be earlier than end_date` |
| `DUCKAKI_INVALID_ARGUMENT` | Unsupported source | `source='sina'` | Binder Error `DUCKAKI_INVALID_ARGUMENT: unsupported source 'sina'` |
| `DUCKAKI_INVALID_ARGUMENT` | NULL parameter | `symbol=NULL` | Binder Error `DUCKAKI_INVALID_ARGUMENT: symbol parameter cannot be NULL` |
| `DUCKAKI_EXTERNAL_ACCESS_DISABLED` | Bind-time check | `SET enable_external_access=false;` | Binder Error `DUCKAKI_EXTERNAL_ACCESS_DISABLED: external access is disabled in DuckDB configuration` |
| `DUCKAKI_EXTERNAL_ACCESS_DISABLED` | Execute-time check | `PREPARE` then `SET enable_external_access=false; EXECUTE;` | Execution Error `DUCKAKI_EXTERNAL_ACCESS_DISABLED: external access was disabled prior to execution` |
| `DUCKAKI_SOURCE_ACCESS` | Upstream HTTP 500 | Server responds with 500 | `DUCKAKI_SOURCE_ACCESS: [eastmoney] http_status=500` |
| `DUCKAKI_SOURCE_ACCESS` | Upstream HTTP 403 | Server responds with 403 | `DUCKAKI_SOURCE_ACCESS: [eastmoney] http_status=403` (1 request, no retry) |
| `DUCKAKI_SOURCE_ACCESS` | Upstream HTTP 429 | Server responds with 429 | `DUCKAKI_SOURCE_ACCESS: [eastmoney] http_status=429` |
| `DUCKAKI_SOURCE_ACCESS` | Upstream HTTP 302 | Server responds with 302 | `DUCKAKI_SOURCE_ACCESS: [eastmoney] http_status=302` (redirects forbidden) |
| `DUCKAKI_SOURCE_ACCESS` | Connection failure | Port not listening | `DUCKAKI_SOURCE_ACCESS: [eastmoney] connection/request error` |
| `DUCKAKI_SOURCE_ACCESS` | TLS handshake failure | Loopback plaintext server on an HTTPS endpoint | TLS transport failure remains `DUCKAKI_SOURCE_ACCESS`; it is not misclassified as source data. |
| `DUCKAKI_SOURCE_DATA` | Invalid JSON | Upstream returns invalid payload | `DUCKAKI_SOURCE_DATA: [eastmoney] JSON decode error` |
| `DUCKAKI_SOURCE_DATA` | Truncated response | Premature body close | `DUCKAKI_SOURCE_DATA: [eastmoney] JSON decode error` |
| `DUCKAKI_SOURCE_DATA` | Envelope rc != 0 | Upstream returns `rc=10001` | `DUCKAKI_SOURCE_DATA: [eastmoney] envelope error: rc=10001` |
| `DUCKAKI_SOURCE_DATA` | Envelope data=null | Upstream returns `data=null` | `DUCKAKI_SOURCE_DATA: [eastmoney] envelope data is null or missing` |
| `DUCKAKI_SOURCE_DATA` | Identity mismatch | Upstream returns `code="000002"` | `DUCKAKI_SOURCE_DATA: [eastmoney] unexpected code in response` |
| `DUCKAKI_SOURCE_DATA` | Market mismatch | Upstream returns `market=1` | `DUCKAKI_SOURCE_DATA: [eastmoney] unexpected market in response` |
| `DUCKAKI_SOURCE_DATA` | Non-finite price | Upstream returns `NaN` | `DUCKAKI_SOURCE_DATA: [eastmoney] non-finite open price` |
| `DUCKAKI_SOURCE_DATA` | Unconfirmed marker | Upstream returns `'-'` for volume | `DUCKAKI_SOURCE_DATA: [eastmoney] unconfirmed volume missing marker '-'` |
| `DUCKAKI_SOURCE_DATA` | Negative fractional volume | Source returns volume `-0.5` | Entire query fails; no positive share count is produced. |
| `DUCKAKI_SOURCE_DATA` | Malformed out-of-range row or duplicate date | Invalid value or duplicate outside requested dates | Entire response fails before date filtering. |
| `DUCKAKI_LIMIT_EXCEEDED` | Date span > 366 days | `2023-01-01` to `2024-02-01` (397 days) | Binder Error `DUCKAKI_LIMIT_EXCEEDED: date range span (397 days) exceeds maximum 366` |
| `DUCKAKI_LIMIT_EXCEEDED` | Oversized response body | Response body > 1 MiB | `DUCKAKI_LIMIT_EXCEEDED: [eastmoney] response body exceeds maximum 1048576 bytes` |
| `DUCKAKI_LIMIT_EXCEEDED` | Oversized response headers | Response headers exceed 32 KiB | `DUCKAKI_LIMIT_EXCEEDED`, not `DUCKAKI_SOURCE_ACCESS`. |
| `DUCKAKI_SOURCE_ACCESS` | Slow-drip response | Server continues sending bytes past a 120 ms test deadline | Deadline terminates the request with `DUCKAKI_SOURCE_ACCESS`. |
| `INTERRUPT` | Cancellation at three phases | `con.interrupt()` while waiting for headers, reading the body, and consuming output | Each query terminates cleanly with `InterruptException`; no fallback result is returned. |
| `EMPTY_RESULT` | Weekend query | `2024-01-06` to `2024-01-07` | Query returns 0 rows cleanly with identical 12-column schema |

Current offline evidence: `make test_offline` passed 18 Rust tests (1 ignored), 4 SQLLogicTest suites, and 36 native integration tests. DNS has a configured five-second resolver timeout; the offline suite does not inject a DNS-specific stall.

---

## Host Usability & Absence of Side Effects
1. **Host Usability**: Every failed query leaves the connection and process stable; subsequent queries on the same connection execute normally (`SELECT 100 + 23 AS answer` returns `123`).
2. **Zero Partial Writes / Zero Disk Footprint**: Queries that fail do not emit partial rows into results; standard queries do not write temporary or persistent cache files to disk.
3. **No Retries / No Fallback**: Exactly 1 request is made per query execution. Upstream errors are immediately propagated without speculative retries or alternative endpoint switching.
