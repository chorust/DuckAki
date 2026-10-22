# 001 构建与验收指南

**当前状态：Spec 001 验收通过**，范围为记录的 Darwin arm64 / DuckDB CLI `v1.5.5` 构建和运行。当前来源结果是时间点证据，不承诺长期可用；逐项状态与限制见 [验收记录](../../docs/validation/acceptance.md)。
本指南证明用户流程及错误边界；字段/限额见 [SQL 合同](contracts/sql.md)，来源形状见 [eastmoney 合同](contracts/eastmoney.md)，不在此复制实现代码。

## 1. 环境与构建关卡 G1

工程基线已固定：官方 Rust 扩展模板 commit `abb7b2f3f954a47f90fc37a99cda3ce0618e6094`、`extension-ci-tools` 子模块 commit `a285602bf745d6ceaa704b5df47f08b00ca296ae`、Rust/Cargo 1.92.0、DuckDB CLI 1.5.5、Python 3.11 与 Make。首次配置会下载构建和测试依赖；依赖准备后，“离线回归”表示测试不访问公网行情。

从仓库根目录执行：

```sh
rustc --version
duckdb --version
git submodule update --init --recursive
DUCKDB_TEST_VERSION=v1.5.5 make configure
make debug
cargo test --locked --lib
```

预期：工具版本符合锁定组合；生成带 metadata footer 的 `build/debug/extension/duckaki/duckaki.duckdb_extension`。普通 cargo build 的共享库不能代替该产物。保留模板 configure/debug 构建流程，确保内部 Cargo 命令使用锁文件。

用已知本地产物启动开发 CLI：

```sh
duckdb -unsigned
```

```sql
LOAD './build/debug/extension/duckaki/duckaki.duckdb_extension';
SELECT * FROM aki_datasets() ORDER BY dataset, source;
```

普通构建已登记唯一的 `stock_daily/eastmoney/sz000001` 组合。用已生成的 release 产物执行 schema 检查：

```sql
DESCRIBE SELECT * FROM aki_stock_daily(
  'sz000001', DATE '2024-01-01', DATE '2024-12-31', source='eastmoney'
);
```

`aki_datasets()` 当前返回恰好一个 `stock_daily/eastmoney` 组合，description 明示仅支持 `sz000001`；固定数据的 G1 schema 与零网络断言由测试构建验证。

## 2. 全离线回归入口

`make test_offline` 复用模板 SQLLogicTest 并运行 `tests/integration/test_sql.py`。该目标构建隔离的 test-support 产物，启动临时 loopback HTTP fixture server，并使用固定版本的测试 Python DuckDB 1.5.5；普通 debug/release 产物不被测试配置覆盖。

```sh
make test_offline
```

该目标运行 Rust 单测、`test/sql/*.test` 和 unittest 集成测试，任一失败返回非零。测试驱动为各场景创建新连接/进程和临时目录，记录请求计数、墙钟耗时、退出状态并清理。只有测试构建接受受限 loopback endpoint；不新增 fixture SQL 来源，也不改变 `source='eastmoney'` 用户合同。依赖准备后，即使公网出站被阻断，离线套件仍全部通过。

### 合成样本和确定性结果（SC-002/005）

自行构造三条记录，**不是原站行情**：

| trade_date | 原始成交手数 | volume_shares | amount |
| --- | --- | --- | --- |
| 2024-01-05 | 12 | 1200 | 12000 |
| 2024-01-08 | 内部可选值缺失 | NULL | NULL |
| 2024-02-01 | 20 | 2000 | 22000 |

统一 symbol=sz000001、currency=CNY、source=eastmoney、adjustment=none，OHLC 为固定有限值。请求 2024-01-05–2024-02-01，结果恰为三行，无周末补行，首尾均包含；加入一条 2024-01-04 有效行后仍只输出三行。内部缺失样例用于证明 NULL 不补零，不声称原站使用某种缺失字符串。

逐列比较值与类型，检查同批 count(DISTINCT fetched_at)=1。加入相同重复行、损坏值、NaN/Infinity、量额非整数换算或溢出必须失败；坏行即使落在区间外也不能被隐藏。单日合法；366 天合法；367 天在请求前超限。

### 故障与宿主边界（SC-004）

| 场景 | 必须观察到的结果 |
| --- | --- |
| 遗漏/NULL/空或未支持 source、symbol；空/非法/倒置日期 | 参数或宿主转换错误；请求计数为 0；错误提示支持值 |
| rc=0、正确身份、klines=[] | 零行，schema 与非空一致 |
| 超时、断网/连接失败、TLS 失败 | SOURCE_ACCESS，含明确原因，有限时间结束 |
| HTTP 429 | SOURCE_ACCESS，http_status=429，无应用层重试 |
| HTTP 403 / 3xx | SOURCE_ACCESS，不跟随跳转、不切源 |
| data=null、坏 JSON/HTML、缺字段、错误身份/数值 | SOURCE_DATA，无部分成功数据 |
| 重复键（包括值相同） | SOURCE_DATA，不静默去重 |
| 正文只传一部分便断开/声明分页或截断 | 整批失败，无先前记录成功声明 |
| 头 >32 KiB、解码正文 >1 MiB、原始行 >4096 或输出行 >366 | LIMIT_EXCEEDED，不静默裁剪 |
| 慢速滴流/不结束 body | 即使持续收到字节也在 30 秒总预算内超时，另记录宿主调度误差 |
| DESCRIBE、普通 EXPLAIN | 请求为 0；EXPLAIN ANALYZE 属于执行 |
| LOAD 后关闭 enable_external_access | 目录可查，日线读取失败，来源请求为 0 |
| PREPARE 时允许，EXECUTE 前 SET enable_external_access=false | EXECUTE 被拒，来源请求仍为 0；使用独立连接测试设置不可逆影响 |
| 宿主在请求中执行 interrupt() | 查询最终中断失败，最多等待在途请求剩余预算；不返回成功批次；后续连接查询仍正常 |
| 重复成功/失败、空批次、捕获的测试 panic | 宿主可继续使用，状态/context/value 释放，无跨 ABI unwind |

取消测试必须实际在 `tests/integration/test_sql.py` 的另一宿主线程调用对应连接的 interrupt，在等待 headers、body 和输出三个时点验证；不能用超时或 kill 进程冒充用户取消。请求计数同时核对绑定零请求、单扫描不按数据块反复抓取、无来源切换。来源安全/取消未通过就不得关闭 G1。

### 目录和默认只读（SC-003）

在公网关闭条件下连续查询当前交付的 `aki_datasets()` 五次，分别计时；每次 <2 秒，内容恰为实现清单，HTTP 请求计数为 0。单次普通 SELECT 结束前后比较隔离目录和数据库对象，确认扩展未创建缓存文件或持久表；DuckDB 自身的临时工作文件单独识别，不将其误认为扩展缓存。

## 3. 获取一次，关联、汇总并保存（SC-001/005/006）

`make test_offline` 先在隔离的 test-support 环境执行与下面等价的 SQL，并对照合成样本。下面的手动 CLI 命令使用**普通构建和真实来源**，须在 G1/G2 通过、网络可用时执行；合成样本的固定期望由测试驱动核对，不套用到真实行情。选用全新的验证数据库与输出文件，明确保存由用户 SQL 触发。

```sh
mkdir -p /tmp/duckaki-validation
duckdb -unsigned /tmp/duckaki-validation/snapshot.duckdb
```

在仓库根目录启动的 CLI 中：

```sql
LOAD './build/debug/extension/duckaki/duckaki.duckdb_extension';
CREATE TEMP TABLE prices AS
SELECT * FROM aki_stock_daily(
  'sz000001', DATE '2024-01-05', DATE '2024-02-01', source='eastmoney'
);
CREATE TEMP TABLE watchlist AS
SELECT 'sz000001' AS symbol, '示例自选股' AS name;

SELECT w.name, p.trade_date, p.close
FROM prices p JOIN watchlist w USING(symbol) ORDER BY p.trade_date;
SELECT date_trunc('month', trade_date)::DATE AS month,
       currency, SUM(amount) AS total_amount
FROM prices GROUP BY 1, 2 ORDER BY 1, 2;

CREATE TABLE saved_prices AS SELECT * FROM prices;
COPY prices TO '/tmp/duckaki-validation/prices.parquet' (FORMAT PARQUET);
```

合成样本的 JOIN 恰三行；月度金额为 2024-01-01/CNY/12000 和 2024-02-01/CNY/22000，NULL 按 DuckDB 既有聚合语义处理。显式保存后停止 mock server/关闭公网，在同会话重复分析 prices 三次，请求计数不增加。

关闭 CLI 后，启动**新会话**，不加载 DuckAki，不需要联网：

```sh
duckdb /tmp/duckaki-validation/snapshot.duckdb
```

```sql
SELECT count(*) AS differences FROM (
  (SELECT * FROM saved_prices
   EXCEPT ALL SELECT * FROM read_parquet('/tmp/duckaki-validation/prices.parquet'))
  UNION ALL
  (SELECT * FROM read_parquet('/tmp/duckaki-validation/prices.parquet')
   EXCEPT ALL SELECT * FROM saved_prices)
);
DESCRIBE saved_prices;
DESCRIBE SELECT * FROM read_parquet('/tmp/duckaki-validation/prices.parquet');
```

预期 differences=0，schema 与原快照一致。集成驱动还须把保存前的三行和两种新会话结果分别逐列比较，不能只让两个可能同样写错的保存结果相互比较；包括 NULL、DOUBLE、BIGINT、DATE、TIMESTAMPTZ 和追溯字符串。再模拟来源修订、重新获取，原 saved_prices/文件保持不变。

保存失败测试：在隔离目录预建有内容的目标文件/表，触发冲突或不可写路径，核对 DuckDB 报错和既有内容未被扩展改写；不承诺原生 COPY 所不具备的跨多个输出原子性。

## 4. 真实来源冒烟（G2、SC-007）

仅在准备验证真实来源时显式执行，普通回归不包含此步骤。先运行 `make release` 生成普通构建产物，并在仓库根目录运行以下命令。DuckDB 的 `-cmd` 在读取 SQL 文件前加载 release 扩展；路径取绝对值以兼容 macOS CLI 的动态库路径限制。

```sh
mkdir -p /tmp/duckaki-validation
smoke_dir="$(mktemp -d /tmp/duckaki-validation/live.XXXXXX)"
duckdb -unsigned -bail \
  -cmd "LOAD '$PWD/build/release/extension/duckaki/duckaki.duckdb_extension';" \
  "$smoke_dir/live.duckdb" < tests/live/smoke.sql
```


预期：至少一条有行情的记录、全在区间内、键无重复、口径 none、币种 CNY、统一 fetched_at；数量和值以当次真实响应为准，不能将本次调研的 242 行写成永久断言。另用单日和周末区间验证边界与合法空响应，核对一条来源数据的量额单位。网络失败应为明确失败，不能改用 fixture 把 live 检查标绿。

在 `docs/validation/` 保存一份脱敏摘要：项目提交/产物校验和、模板/绑定/Rust/CLI 版本、平台、Asia/Shanghai 时间与 fetched_at、SQL 参数、来源、请求预算、状态、条数、最早/最晚日、耗时、字段/单位核对依据及限制。原始真实数据不默认提交。该步骤成功才满足 SC-007；本次 curl 响应不是该证据。

## 5. 发布前关卡与说明核对

普通 release 构建不得启用 test-support；环境里的测试 endpoint 对其无效。重复构建/加载与所声明平台、CLI 精确版本一致；新平台重新执行完整回归。真实来源/源码许可和归属按 G4 完成后再分发。

使用说明必须讲清：两端包含日期、单位、仅支持标的、无默认来源、显式 ORDER BY、视图不保存快照、重复读取可能再次请求、LIMIT 不保证减少上游抓取、没有自动缓存。目录和成功示例只包含实际交付能力。所有 SC-001–SC-007 有独立证据后才标记 MVP 完成。
