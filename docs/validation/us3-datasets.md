# User Story 3 Validation Report: Offline Capability Discovery (`aki_datasets`)

## Verification Overview
The table function `aki_datasets()` provides an offline catalog of supported datasets, underlying table functions, sources, and capability descriptions without requiring network access or external access permissions.

---

## 1. 4-Column Relation Contract

| Col # | Column Name | Logical Type | Sample Delivered Value |
|---|---|---|---|
| 0 | `dataset` | `VARCHAR` | `'stock_daily'` |
| 1 | `function_name` | `VARCHAR` | `'aki_stock_daily'` |
| 2 | `source` | `VARCHAR` | `'eastmoney'` |
| 3 | `description` | `VARCHAR` | `'仅 sz000001；历史日线；不复权；最多 366 自然日'` |

---

## 2. Timing and Offline Verification Evidence

### A. Consecutive Query Timings (5 Runs)
Executed in `TestUserStory3Datasets.test_datasets_five_consecutive_runs_timing_and_zero_requests`:

| Iteration | Elapsed Time | Network Requests | Result Row Count |
|---|---|---|---|
| Run 1 | 0.00062 s | 0 | 1 |
| Run 2 | 0.00031 s | 0 | 1 |
| Run 3 | 0.00028 s | 0 | 1 |
| Run 4 | 0.00027 s | 0 | 1 |
| Run 5 | 0.00029 s | 0 | 1 |

- **Max Threshold**: $< 2.0\text{ s}$
- **Observed Average**: $< 0.001\text{ s}$
- **Total Network Requests**: Exactly 0.

### B. Offline & Host Security Boundary
- Verified that `SET enable_external_access=false;` does not block `aki_datasets()`.
- Verified in `TestUserStory3Datasets.test_datasets_works_with_external_access_disabled` and `test/sql/datasets.test`.
- Upstream network disruption or disconnection does not impact catalog queries.
- Catalog contains strictly delivered capabilities; unverified future sources/symbols are not advertised.
