# DuckAki

**AKShare, but in DuckDB.**

DuckAki 是面向 DuckDB 的原生财经数据扩展（Rust C API extension），支持在 DuckDB 中通过标准 SQL 表函数直接获取行情数据、与本地表关联，并保存为本地 DuckDB 数据库表或 Parquet 文件。

- **核心架构**：基于 DuckDB Unstable C API 深度绑定，纯 Rust（edition 2024）实现，不依赖 Python AKShare、AKTools 或任何后台服务。
- **安全与边界**：严格遵守 DuckDB 宿主安全策略（Bind 阶段零网络请求、`SET enable_external_access=false` 彻底阻断网络获取、防 PREPARE 绕过、支持协同式中断与错误隔离）。
- **已交付能力**：
  - `aki_datasets()`：离线发现支持的数据集和来源。
  - `aki_stock_daily(symbol, start_date, end_date, source='eastmoney')`：A 股标的 `sz000001` 历史日线（不复权，严格单次最多 366 自然日）。
- **验收状态**：G1–G4 与 SC-001–SC-007 在记录的 Darwin arm64 / DuckDB CLI `v1.5.5` 构建上通过。真实来源检查是时间点证据，不代表长期可用；详见 [验收记录](docs/validation/acceptance.md)。

---

## 快速上手

### 1. 构建与测试
```bash
# 配置虚拟环境与依赖
make configure

# 编译 release 产物 (含 DuckDB metadata header/footer)
make release

# 运行全套离线回归测试（包含 Rust 单测、SQLLogicTest 和 Python 边界测试）
make test_offline
```

### 2. 在 DuckDB CLI 中使用
```bash
# 启动 DuckDB CLI (指定允许加载未签名扩展)
duckdb -unsigned
```

```sql
-- 1. 加载扩展 (提供产物的绝对路径)
LOAD '/path/to/DuckAki/build/release/extension/duckaki/duckaki.duckdb_extension';

-- 2. 离线查看支持的数据集与来源
SELECT * FROM aki_datasets();

-- 3. 查询平安银行历史日线 (12 列固定标准输出)
SELECT
    symbol,
    trade_date,
    open,
    high,
    low,
    close,
    volume_shares,
    amount,
    currency,
    source,
    adjustment,
    fetched_at
FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-10', source='eastmoney')
ORDER BY trade_date ASC;

-- 4. 与本地自选股表做关联计算
CREATE TEMP TABLE watchlist (symbol VARCHAR, target_price DOUBLE);
INSERT INTO watchlist VALUES ('sz000001', 10.0);

SELECT
    w.symbol,
    d.trade_date,
    d.close,
    w.target_price,
    round(w.target_price - d.close, 2) AS price_gap
FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-10', source='eastmoney') d
JOIN watchlist w ON d.symbol = w.symbol;

-- 5. 物化并导出为 Parquet
CREATE TABLE stock_archive AS
SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-10', source='eastmoney');

COPY stock_archive TO 'stock_archive.parquet' (FORMAT PARQUET);
```

---

## 文档索引

| 文档 | 内容 |
|---|---|
| [Feature Spec](specs/001-financial-data-access/spec.md) | 需求规格、8 类错误分类 (SC-004) 与成功标准 |
| [001 实施计划](specs/001-financial-data-access/plan.md) | 设计决策、数据模型与合同要求 |
| [接口与数据合同](docs/spec.md) | SQL 签名、12 列定义、单位及 DuckDB SQL 习惯 |
| [架构设计](docs/architecture.md) | 数据流、状态生命周期、安全模型与错误隔离 |
| [技术路径](docs/technical-path.md) | C API 绑定、有界 HTTP、测试隔离与发布要求 |
| [SQL 完整示例](docs/examples.sql) | 包含 CTAS、JOIN、月度统计、Parquet 读写示例 |
| [G1 宿主安全报告](docs/validation/g1-engine.md) | 宿主安全边界、零网络 Bind、取消及中断证据 |
| [G2 真实来源核验](docs/validation/g2-source.md) | 东方财富 HTTPS 来源核验证据与真实参数依据 |
| [G3 端到端冒烟](docs/validation/live-smoke.md) | 2024 全年 242 交易日物化、关联与聚合验收证据 |
| [发布与许可清单](docs/validation/distribution.md) | G4 分发检查清单、上游代码与数据归属说明 |

---

## 边界与约束说明
- **严格标的与来源**：当前版本仅支持 `symbol='sz000001'` 和 `source='eastmoney'`，其他标的或来源立即返回 `DUCKAKI_INVALID_ARGUMENT`。
- **日期上限**：查询区间严格限制在 366 自然日内；超过限制返回 `DUCKAKI_LIMIT_EXCEEDED`。
- **单位规范**：成交量已严格从东方财富的上游“手”换算为“股” (`volume_shares = hands * 100`)；金额保持元。
- **开源许可与准入**：本项目采用 [Apache-2.0](LICENSE) 许可证，完整保留上游扩展模板归属声明。数据获取由用户客户端直接请求上游，产物与仓库不捆绑分发专有行情数据。
