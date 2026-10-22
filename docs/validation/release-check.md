# Release Binary & Integrity Verification Report (T036)

## Verification Overview
This report verifies that standard production release builds (`make release`) satisfy all packaging, distribution isolation, and security requirements.

---

## 1. Feature Isolation & Security Audits

### A. Zero Test-Support in Production
- **Rule**: Loopback test overrides (`DUCKAKI_TEST_ENDPOINT`) must only be compiled when the `test-support` Cargo feature is explicitly requested.
- **Audit**: Setting `DUCKAKI_TEST_ENDPOINT=http://127.0.0.1:1234` (a non-existent loopback port) while running `aki_stock_daily` with the release binary was completely ignored by the binary:
  ```console
  $ DUCKAKI_TEST_ENDPOINT=http://127.0.0.1:1234 duckdb -unsigned -c "LOAD '$(pwd)/build/release/extension/duckaki/duckaki.duckdb_extension'; SELECT count(*) FROM aki_stock_daily('sz000001', DATE '2024-01-05', DATE '2024-01-05', source='eastmoney');"
  ┌──────────────┐
  │ count_star() │
  │    int64     │
  ├──────────────┤
  │            1 │
  └──────────────┘
  ```
  The release extension routed securely to production HTTPS endpoint and succeeded.

### B. Panic Strategy
- `Cargo.toml` specifies `panic = "unwind"` for dev profile and release profile retains standard unwind so that all C FFI entrypoints safely catch panics via `std::panic::catch_unwind` and convert them into desensitized DuckDB errors.

### C. Extension Metadata & Footer
- Verified metadata footer attached to `build/release/extension/duckaki/duckaki.duckdb_extension`:
  - `header signature`: 4
  - `duckdb_platform`: `osx_arm64`
  - `duckdb_version`: `v1.5.5`
  - `extension_version`: `v0.1.0`
  - `abi_type`: `C_STRUCT_UNSTABLE`

### D. Pure Native Dependencies (No Python Runtime)
- Inspected dynamic linkage via `otool -L build/release/libduckaki.dylib`:
  - `/usr/lib/libiconv.2.dylib`
  - `/System/Library/Frameworks/CoreFoundation.framework/Versions/A/CoreFoundation`
  - `/usr/lib/libSystem.B.dylib`
- Completely self-contained Rust native extension; zero runtime dependency on Python, AKShare, or AKTools.
