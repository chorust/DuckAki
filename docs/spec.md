# DuckAki Spec

状态：**已实现并验证 (Darwin arm64, DuckDB v1.5.5 `d8cdaa33fd`)**。原生表函数 `aki_stock_daily` 与离线目录 `aki_datasets()` 已通过 G1–G4 验收；本次 live 证据是有范围的一次核验，不代表来源长期可用。

2026-10-21 完成 [Feature Spec](../specs/001-financial-data-access/spec.md) 核心用例和验收。本文件记录精确 SQL 接口、字段类型、单位约定及执行习惯。
## 目标与用户场景

让使用 DuckDB 做本地分析的人，以稳定的表结构消费财经数据，减少重复抓取、字段清洗和文件转换。

产品是财经数据 DuckDB 扩展，先做数据获取。Rust 数据模块负责请求与规范化，服务扩展且可独立测试；首版不设计或发布通用 Rust SDK。AKShare 是接口和来源参考，未来可整合其他项目梳理的数据入口。

最小完整流程：获取一个标的的一段历史行情 → 用 SQL 筛选、聚合或 JOIN 本地数据 → 显式保存结果。先验证这个流程，再扩充接口数量。

“开放财经数据”描述项目关注的数据访问场景，不代表所有上游数据都具有开放许可或可无限调用。

## MVP 范围

| 项目 | 首版边界 |
| --- | --- |
| 数据集 | A 股单标的历史日线 |
| 来源 | 一个具名来源；Eastmoney `eastmoney` 已实现并通过当前真实来源核验 |
| 查询参数 | 显式市场代码、开始日期、结束日期，必填命名参数 `source` |
| 周期与复权 | 仅日线、不复权；暂不暴露尚未支持的选项 |
| 消费方式 | Rust 实现的 DuckDB 原生扩展，通过 SQL 表函数返回关系 |
| 运行时 | Rust 直接接入来源；不依赖 Python AKShare 或 AKTools 服务 |
| 持久化 | 用户显式使用 DuckDB SQL 保存；不自动写缓存或建表 |
| 成功标准 | 固定样例可离线回归，真实来源可完成一条端到端查询 |
| 数据集发现 | `aki_datasets()` 离线列出本版本已实现的数据集及支持来源 |

暂不纳入：AKShare 全接口移植、实时订阅、分钟/tick 数据、多源自动切换、技术指标库、回测、交易执行、Web 服务、仪表盘、定时同步和自动缓存。

## SQL 接口

每个数据集一个具名表函数，固定返回结构。首版提供两个可执行入口：

```text
aki_stock_daily(symbol VARCHAR, start_date DATE, end_date DATE,
                source = <VARCHAR>) -> TABLE
aki_datasets() -> TABLE
```

`source` 是必填命名参数，没有隐式默认值；遗漏、NULL 或未支持的来源都在请求前报错。当前能力目录显式登记已实现并核验的 `eastmoney`，不做自动回退。

`symbol` 使用带市场前缀的字符串。当前只接受 `sz000001`；未覆盖的市场明确拒绝，不仅凭首位数字猜市场。裸代码兼容不在首版范围。

日期范围采用两端包含的 `[start_date, end_date]`。必须提供合法、非 NULL 的参数且开始日不晚于结束日；无交易日时不补造行情。若上游边界不同，适配器负责转换并对结果再次过滤。

首版只支持日线、不复权，不暴露 `period`、`adjust` 等尚未支持的选项。AKShare 接口名称在文档中映射，不作为第二套 SQL 别名。

Python AKShare 可生成对照样本，帮助确认字段和来源行为；DataFrame 注册演示不计作 DuckDB 扩展验收。构建和测试工具可能使用 Python，这与产品运行时依赖不同。

## 日线输出合同

每行表示同一来源、同一标的、同一复权口径下一个交易日的数据。

| 字段 | DuckDB 类型 | 含义与约束 |
| --- | --- | --- |
| `symbol` | `VARCHAR` | 带市场前缀，保留前导零 |
| `trade_date` | `DATE` | 交易所本地交易日期，不当作 UTC 时间戳 |
| `open` / `high` / `low` / `close` | `DOUBLE` | 每股价格，币种由 `currency` 指定；首版不复权 |
| `volume_shares` | `BIGINT` | 成交股数；需验证来源单位及换算规则 |
| `amount` | `DOUBLE` | 成交金额，使用 `currency` 的主货币单位；来源不提供时为 NULL |
| `currency` | `VARCHAR` | 币种代码；首版 A 股为 `CNY`，不做汇率转换 |
| `source` | `VARCHAR` | 实际行情提供方，例如 `eastmoney`；不把 AKShare 适配库当作行情提供方 |
| `adjustment` | `VARCHAR` | 首版固定为 `none`，为保存后的数据保留口径 |
| `fetched_at` | `TIMESTAMPTZ` | 本次获取完成时间，同一结果批次一致；不是行情发生时间 |

价格和金额使用浮点数服务分析用途，不提供记账金额精度承诺。更改为定点类型前需核对各数据源精度。

参考接口 `stock_zh_a_hist` 的官方文档区分成交量“手”和成交额“元”，并支持不同复权参数。接入时需证明“手”到“股”的换算规则，不能无条件对所有来源乘固定倍数；换算无法确认的来源不纳入标准日线接口。[AKShare 股票数据文档](https://akshare.akfamily.xyz/data/stock/stock.html)

约束：

- 标的、日期、来源、币种、复权口径和获取时间不可缺失；OHLC 为必需值。来源明确标记缺失的量额可为 NULL，不能填零冒充观测值。
- 数字无法解析、必需字段缺失或出现非有限值时失败；不跳过坏行后返回“成功”。
- 同一批次的 `(symbol, trade_date, source, adjustment)` 不得重复；冲突作为数据质量错误暴露。
- 不生成非交易日、不向前填充值、不自行重算复权。结果顺序由用户的 `ORDER BY` 保证。
- 首版无需返回上游全部派生指标；新增列、改变单位或类型都必须同步文档、样例与测试。

## 数据集发现与后续接入

`aki_datasets()` 无参数、无需联网，每行表示一个已实现的“数据集 × 来源”组合；按下列固定结构返回，顺序由调用者指定：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `dataset` | `VARCHAR` | 数据集名称，例如 `stock_daily` |
| `function_name` | `VARCHAR` | 读取入口，例如 `aki_stock_daily` |
| `source` | `VARCHAR` | 该数据集已实现支持的实际来源 |
| `description` | `VARCHAR` | 数据内容与首版范围的简要说明 |

目录列出本版本实际注册并实现的读取能力，不列未来计划，也不把目录记录当作来源持续在线的保证。当前 `aki_datasets()` 返回 `stock_daily / aki_stock_daily / eastmoney` 一项。

新增来源只有在粒度、字段语义、单位和复权口径符合合同后，才能加入现有函数的 `source` 选项。无法对齐的数据另建具名函数并说明差异。股票列表、财报、宏观序列等按各自数据集定义类型，不塞入一个返回 JSON 的通用入口。

## 查询、聚合与保存

以下为已实现的完整使用流程；现场来源验证命令及当次结果见 [Quickstart](../specs/001-financial-data-access/quickstart.md) 与验收报告：

```sql
SELECT * FROM aki_datasets() ORDER BY dataset, source;

CREATE TEMP TABLE prices AS
SELECT * FROM aki_stock_daily(
    'sz000001', DATE '2024-01-01', DATE '2024-12-31',
    source = 'eastmoney'
);

-- 独立的本地示例表；实际使用时也可替换为已有的自选表。
CREATE TEMP TABLE watchlist AS
SELECT 'sz000001' AS symbol, '示例自选股' AS name;

SELECT w.name, p.trade_date, p.close
FROM prices AS p
JOIN watchlist AS w USING (symbol)
ORDER BY p.trade_date;

SELECT date_trunc('month', trade_date) AS month,
       currency, SUM(amount) AS total_amount
FROM prices
GROUP BY 1, 2
ORDER BY 1, 2;

COPY prices TO 'prices.parquet' (FORMAT PARQUET);
```

建表和导出复用 DuckDB SQL，不另设 `save()`。会话内多次分析先显式物化到临时表；跨会话可保存普通表或 Parquet。普通视图不保存远程结果，重扫描或重复调用可能再次请求来源。`LIMIT 10` 不保证只从上游获取十行，首版不承诺一般谓词下推。

## 错误与可复现性

| 情况 | 约定 |
| --- | --- |
| 参数不合法、市场或来源未支持 | 在网络请求前失败，说明参数及允许值，不尝试其他来源 |
| 来源明确返回有效的空结果 | 返回零行且表结构不变 |
| 网络失败、限流、拒绝访问、响应结构变化 | 明确失败，不能转为空表 |
| 无法判断空响应是无数据还是上游异常 | 作为来源异常，不猜测 |
| 分页中断或记录冲突 | 整次获取失败，不把不完整数据标为成功 |

网络调用遵循 SQL 合同：30 秒单调总 deadline 覆盖请求、解析、校验与输出准备；DNS 解析和连接各有 5 秒上限，响应头最多 32 KiB，正文最多 1 MiB，记录最多 366 条。无应用层重试或自动切换来源。

实时获取可能随时间改变。需要复现的分析应保存本次结果及获取时间，并固定查询参数、DuckAki 版本及来源。离线回归用固定 fixture，不能依赖每次联网结果相同。

## MVP 验收

1. 在选定版本的 DuckDB CLI 中加载 Rust 扩展，通过 `aki_stock_daily` 获得规定关系，`aki_datasets()` 离线列出实际实现的能力；运行时无需 Python AKShare 或外部 AKTools 服务。
2. 固定样例逐列验证日期边界、代码前导零、列类型、成交股数换算、金额币种、NULL 及重复记录处理。
3. 参数错误不触发请求；无交易记录与网络/结构错误可区分。
4. 用户能用普通 SELECT、JOIN、聚合消费结果，并显式保存为表或 Parquet 后离线重读。
5. 有一次真实来源的冒烟记录，写明时间、版本、参数、结果及限制；一次成功不等于长期可用性承诺。
6. Rust 核心通过离线回归；所有公开宣称支持的 DuckDB 版本和平台通过扩展构建、加载及 SQL 回归。

具体实施顺序见 [Roadmap](roadmap.md)，职责划分见[架构](architecture.md)。
