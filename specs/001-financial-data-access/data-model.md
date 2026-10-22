# 001 数据模型

依据 [spec.md](spec.md) 和 [SQL 合同](contracts/sql.md)。以下是拟实现模型，不代表数据库中已有这些表。核心使用普通 Rust 值，FFI 指针只存在于绑定状态。

## DailyRequest（一次读取请求）

| 字段 | 核心含义 | 校验 |
| --- | --- | --- |
| symbol | 原样保留的市场前缀字符串 | 首版精确接受 `sz000001`，保留前导零；不 trim/猜市场 |
| start_date / end_date | 交易所本地日历日期 | 非 NULL、有限日期；start ≤ end；两端包含天数 ≤366；单日合法 |
| source | 实际来源标识 | 非 NULL、非空；仅 `eastmoney`，无默认值、不接受别名 |

不公开 period/adjust、URL、缓存或复权参数。DuckDB 无法转换的 DATE 输入可由宿主提前拒绝，错误仍必须发生在请求前；`infinity`/`-infinity` 不在支持日期范围。

## Capability（静态能力目录项）

| 字段 | SQL 类型 | 首版可交付后取值 |
| --- | --- | --- |
| dataset | VARCHAR，非 NULL | stock_daily |
| function_name | VARCHAR，非 NULL | aki_stock_daily |
| source | VARCHAR，非 NULL | eastmoney |
| description | VARCHAR，非 NULL | 说明仅 sz000001、不复权、最多 366 自然日 |

键为 `(dataset, source)`，每个已实现组合一行。内部记录同时持有允许标的和对应适配器的静态关联，驱动参数校验。未交付/验证未通过的来源不登记；空目录也保留四列。没有在线状态或健康探测时间字段。

## DailyRecord（日线记录）

| 字段（固定顺序） | SQL 类型 | NULL | 约束 |
| --- | --- | --- | --- |
| symbol | VARCHAR | 否 | 与请求一致，保留市场和前导零 |
| trade_date | DATE | 否 | 区间内的来源实际交易日，不补日历 |
| open | DOUBLE | 否 | 有限数值，原币每股，不复权 |
| high | DOUBLE | 否 | 同上 |
| low | DOUBLE | 否 | 同上 |
| close | DOUBLE | 否 | 同上 |
| volume_shares | BIGINT | 是 | 经过确认的单位换算；非负整数，检查溢出；缺失不补零 |
| amount | DOUBLE | 是 | 有限、非负的 CNY 元值；缺失不补零 |
| currency | VARCHAR | 否 | 首版 CNY，不隐式换汇 |
| source | VARCHAR | 否 | eastmoney，不写 AKShare |
| adjustment | VARCHAR | 否 | none |
| fetched_at | TIMESTAMPTZ | 否 | 完整获取并通过校验时的 UTC instant，微秒精度；全批一致 |

批内唯一键为 `(symbol, trade_date, source, adjustment)`，重复即失败，即使数值完全相同。核心用 HashSet 校验，不让绑定静默去重。`fetched_at` 是观测时间，不是交易时间或全球唯一批次 ID；不同请求可能时间相同，不据此自动合并快照。

价格/金额以 DOUBLE 供分析，不作结算精度承诺。保留来源价格，不重新计算复权或“修复”看起来异常的价格；非法、不可解析和非有限值明确失败。来源没有提供可确认的量额缺失标记时，不猜测空串或横线的含义，详见[来源合同](contracts/eastmoney.md)。

## FetchBatch（瞬时获取批次）

一个 DailyRequest 产生一个 FetchBatch，含 0..366 条 DailyRecord 和统一获取时间。核心可先返回已验证行，再由绑定/受注入的时钟赋予同一个完成时间；空批次没有记录，schema 不变。响应和记录受[SQL 合同](contracts/sql.md)限额约束，没有持久 batch 表或自动批次编号。

```text
Unbound → Bound（参数和 schema，无网络）
        → Fetching（执行前重查外部访问，扫描独立）
        → Validating（完整响应、字段、唯一性、范围、单位、限额）
        → Ready（全部通过，统一 fetched_at，允许输出）
        → Exhausted → Released
```

Fetching/Validating 中任何失败均进入 Failed，再释放全部状态；Ready 前没有用户数据块可被报告为成功。任何阶段取消都必须使整个查询以中断失败结束。Ready/Exhausted 不构成隐式保存。重新执行或重扫描可创建新批次，不共享可变全局缓存。

BindState 除请求外持有宿主 context wrapper，只在绑定层按 C API 生命周期使用；InitState 持有记录与输出游标。每项只有一个清晰所有者及 drop callback，失败和零行同样释放。取消/失败后不得复用游标继续输出。

## SavedSnapshot（用户创建的结果）

用户显式 CTAS 得到临时/持久表，或 COPY 得到 Parquet。记录字段、NULL 与类型沿用 DailyRecord；没有额外扩展专属保存格式或表名约束。多次保存由用户命名，不根据后续行情更新或覆盖旧表。保存失败与覆盖策略遵守 DuckDB 原生语义，扩展不报告保存成功。

关系：`Capability → DailyRequest → FetchBatch → DailyRecord[]`；用户可以将一个批次物化为多个独立 SavedSnapshot。本地 watchlist 以相同 `symbol` 关联，无需额外映射实体。跨批次保存到同表时，用户自行决定如何区分批次，批内唯一键不等于该表的全局主键。
