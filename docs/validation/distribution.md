# G4 Distribution & Governance Checklist

## 1. Upstream Code & Attribution Audit

| Component | Source / Commit | Upstream License | Attribution Status |
|---|---|---|---|
| Rust Extension Scaffold | `duckdb/extension-template-rs` (`abb7b2f3f954a47f90fc37a99cda3ce0618e6094`) | MIT License | Preserved header comment in `src/lib.rs`, `src/bindings.rs`, `Makefile`, etc. |
| CI & Packaging Submodule | `duckdb/extension-ci-tools` (`a285602bf745d6ceaa704b5df47f08b00ca296ae`) | MIT License | Pinned as git submodule in `.gitmodules`. |
| Rust DuckDB Bindings | `duckdb = "=1.10505.0"` | MIT License | Declared and locked in `Cargo.toml` and `Cargo.lock`. |
| HTTP Client | `ureq = "=3.4.2"` | MIT / Apache-2.0 | Declared and locked in `Cargo.toml` and `Cargo.lock`. |
| AKShare Reference | `akfamily/akshare` | Apache-2.0 | Conceptual interface and data source reference; zero code copied or vendored. |

---

## 2. Upstream Data Source Access & Redistribution Conditions

- **Direct Client Fetching**: The DuckAki extension operates strictly as a local client making direct HTTPS calls from the user's workstation/server to the upstream provider (`push2his.eastmoney.com`).
- **Zero Proprietary Data Bundling**: The extension binary contains zero cached market data. No proprietary stock quotes or klines are committed to the repository or bundled into distribution binaries.
- **Data Licensing**: Users are subject to the upstream provider's terms of service and regulatory rules regarding personal analysis, internal analytics, and commercial use.

---

## 3. Project License & Release Gate G4 Status

- **Project License Decision**: The project maintainer has formally designated **Apache-2.0** as the project open-source license.
- **License File**: `LICENSE` containing the full Apache License Version 2.0 text and copyright notice (`Copyright 2026 DuckAki Contributors`) is committed at the repository root.
- **Cargo Configuration**: `Cargo.toml` declares `license = "Apache-2.0"`.
- **Upstream Attribution**: MIT attributions for `duckdb/extension-template-rs`, `duckdb/extension-ci-tools`, and `duckdb-rs` are fully preserved.
- **G4 Gate Verdict**:
  - **Engineering & Functional Gates (G1, G2, G3)**: **PASSED** for the recorded Darwin arm64 / DuckDB v1.5.5 run; see the current offline and live evidence in `docs/validation/acceptance.md`.
  - **Distribution Gate (G4)**: **PASSED** (Apache-2.0 license established; attribution complete; client-side zero-bundled data confirmed). This records distribution readiness; no public release action is implied.
---

## 4. Platform & Version Support Matrix

| Platform / Target | DuckDB Version | Extension ABI | Status |
|---|---|---|---|
| macOS (Darwin arm64) | `v1.5.5` (`d8cdaa33fd`) | `C_STRUCT_UNSTABLE` | **Verified & Supported** |
| macOS (Darwin x86_64) | `v1.5.5` | `C_STRUCT_UNSTABLE` | Unverified / Unsupported |
| Linux (x86_64) | `v1.5.5` | `C_STRUCT_UNSTABLE` | Unverified / Unsupported |
| Windows (x86_64) | `v1.5.5` | `C_STRUCT_UNSTABLE` | Unverified / Unsupported |
| WebAssembly (Wasm) | `v1.5.5` | `C_STRUCT_UNSTABLE` | Unverified / Unsupported |
