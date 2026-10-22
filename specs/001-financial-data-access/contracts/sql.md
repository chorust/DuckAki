# SQL 接口合同 v0.3 / 001 实施细化

本合同细化[已采纳接口](../../../docs/spec.md)，不新增公开入口。**当前实现已通过 Spec 001 验收**；以下行为对应记录的 Darwin arm64 / DuckDB CLI v1.5.5 构建。字段定义见[数据模型](../data-model.md)。

## aki_stock_daily

```text
aki_stock_daily(symbol VARCHAR, start_date DATE, end_date DATE,
                source = <VARCHAR>) → TABLE
```

三个位置参数和必填命名参数 source；无默认来源。首版来源 `eastmoney`，标的清单仅 `sz000001`，不接受裸代码、其他市场/标的、大小写别名或含空格的近似值。错误说明当前允许值，不发送探测请求来猜标的。

start/end 必须是非 NULL 的有限合法 DATE；两端包含，start ≤ end，包含日数 ≤366。非法 DATE 字面量可由 DuckDB 拒绝；DATE infinity 同样拒绝。单日范围合法，完整 2024 年合法，367 天范围明确超限。不提供 period/adjust，也不静默忽略未知命名参数。

输出固定顺序为：

```text
symbol VARCHAR, trade_date DATE,
open DOUBLE, high DOUBLE, low DOUBLE, close DOUBLE,
volume_shares BIGINT, amount DOUBLE,
currency VARCHAR, source VARCHAR, adjustment VARCHAR,
fetched_at TIMESTAMPTZ
```

仅 volume_shares/amount 允许 NULL；其余逻辑非空约束通过校验保证，不依赖 DESCRIBE 是否将表函数列标注为 NOT NULL。DATE 是交易所本地交易日；成交量为股、价格为 CNY 元/股、金额为 CNY 元；adjustment=none。fetched_at 为同批完成时间，不是交易时间。一个合法空批次返回零行，保留相同 schema。

有效范围外记录过滤掉，不造非交易日；坏行、重复键、非有限数值、无法解释的单位、溢出及含糊空响应使整个读取失败。只有来源合同认可的显式缺失才能成为 NULL。来源端失败不得自动切换。

## aki_datasets

```text
aki_datasets() → TABLE (
    dataset VARCHAR, function_name VARCHAR,
    source VARCHAR, description VARCHAR
)
```

全部逻辑非空，无参数。每个**已实现、可交付**的数据集/来源组合一行，键 `(dataset, source)`。首版通过来源关卡后的预期行：

| dataset | function_name | source | description |
| --- | --- | --- | --- |
| stock_daily | aki_stock_daily | eastmoney | 仅 sz000001；历史日线；不复权；最多 366 自然日 |

目录和读取校验从同一静态定义产生，不访问网络，不展示计划中的来源/数据集，不承诺来源当前在线。来源暂时离线不改变已交付目录；尚未完成接入的来源不提前登记。关闭外部访问时目录仍可查询。

## 执行和等待合同

- bind 只校验、声明 schema、检查宿主设置。DESCRIBE 和普通 EXPLAIN 不发网络请求；EXPLAIN ANALYZE 会执行。
- 每次扫描有独立状态，首次执行前重读当前 enable_external_access；false、设置缺失或读取失败均拒绝请求，包含 PREPARE 后变更设置的情况。
- 每扫描最多一个逻辑 HTTP 请求，不分页、不应用层重试、不重定向。原生传输层的重发行为通过请求计数测试核实，不能用这条约定声称 SQL 多次引用只会抓一次。
- 网络全局 deadline 30 秒，覆盖 DNS、连接、TLS 和完整响应体；DNS/连接各最多 5 秒且不叠加到总预算之外。解析/校验按相同 deadline 检查，超时即丢弃批次。
- 响应头最多 32 KiB；解码后正文最多 1 MiB；原始记录最多 4096 行；输出最多 366 行。超过任一限额失败，不返回前 N 行成功结果。
- 完整响应与全批校验通过后才输出。用户取消时查询必须最终为中断失败；同步在途请求不保证即时停止，等待受剩余 30 秒预算限制。取消回归不通过则不得交付。
- EOF 中断、服务器可见截断或分段要求使整个请求失败。首版不续页，不把“目前拿到的行”标成完整历史。合法 HTTP 响应的来源内部遗漏无法仅由本地推断，不提供独立行情审计保证。

30 秒是扩展控制的请求/校验预算，不包括宿主排队和操作系统暂停；测试以独立墙钟记录并单列少量宿主调度误差，不用无限等待伪装满足上限。

## 错误合同

查询失败，不返回“状态行”混入行情。扩展错误消息使用稳定类别前缀，后接来源、阶段和必要的脱敏原因；用户输入无法转换或未知命名参数由 DuckDB 自身报错同样合法。

| 类别前缀/宿主错误 | 情况 | 请求/结果 |
| --- | --- | --- |
| DUCKAKI_INVALID_ARGUMENT | 缺失/NULL 参数、未支持来源/标的、倒置日期 | 来源请求 0 次 |
| DUCKAKI_EXTERNAL_ACCESS_DISABLED | 宿主禁止或无法确认外部访问 | 来源请求 0 次 |
| DUCKAKI_SOURCE_ACCESS | 网络/TLS/超时、非 2xx（含 3xx/403/429） | 整批失败，原因区分 timeout/http_status/network |
| DUCKAKI_SOURCE_DATA | rc/结构/身份错误、坏字段、重复、含糊空结果、可见截断 | 整批失败，不跳过坏行 |
| DUCKAKI_LIMIT_EXCEEDED | >366 天、头/正文/行数超限 | 日期超限时 0 请求；响应超限不报告成功 |
| DuckDB interruption error | 宿主取消 | 查询失败；不要求伪造扩展前缀 |
| DUCKAKI_INTERNAL | 捕获的内部 panic 或绑定不变量失败 | 脱敏报错，保持宿主可用 |

不打印凭据、代理认证、完整 URL query、整段响应或本机无关路径。来源错误可以保留 HTTP 状态码和日期/标的等必要查询上下文，不依赖完整报错句子的字节一致性。

## 查询、快照与兼容边界

用户 `ORDER BY` 决定输出顺序；LIMIT 不保证减少上游数据量；重复调用、视图及重扫描可能再次请求。稳定分析先显式 CTAS，临时表会话内保留，普通表/Parquet 跨会话保存。扩展不自动写文件、建持久表、覆盖或合并快照。

新增标的/来源须更新静态能力范围、文档、单位依据、fixture 和真实冒烟证据。新来源只有满足相同粒度、单位、字段含义与复权口径才能归入 stock_daily；不满足则单独命名数据集，不静默变更现有类型或单位。

本地开发可用 CLI `-unsigned` 加载已知的开发产物；这不是长期分发或社区安装方案。精确版本、平台、许可证与来源条件通过后，才另行给出分发方式。
