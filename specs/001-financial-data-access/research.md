# 001 研究与工程决策

**日期**：2026-10-21（Asia/Shanghai）  
**依据**：[需求](spec.md)、[SQL 基线](../../docs/spec.md)、[技术路径](../../docs/technical-path.md)。三个 GPT-6 Luna max worker 分别研究工程版本、来源协议、宿主执行安全；主代理整合并执行下述探测。本文区分选型、静态证据与运行证据。

## R1：固定一个原生扩展工程组合

- **Decision**：一个 Rust crate，edition 2024，工具链固定 `1.92.0`；DuckDB CLI `v1.5.5`（本机 `d8cdaa33fd`）；`duckdb = "=1.10505.0"`，启用 `loadable-extension`。模板固定为 `abb7b2f3f954a47f90fc37a99cda3ce0618e6094`，随该提交固定 `extension-ci-tools` 子模块 `a285602bf745d6ceaa704b5df47f08b00ca296ae`。引入时提交 `Cargo.lock`，普通构建使用 `--locked`。
- **Rationale**：这组模板和绑定对应 DuckDB 1.5.5，与当前 CLI 一致。模板负责 metadata footer；单独 `cargo build` 不是扩展加载验收。模板启用 unstable C API，因此首版只验证这个精确 DuckDB 版本。
- **Alternatives considered**：不跟随模板浮动 main；不改为 C++、Python 桥接或多 crate SDK；不自写扩展打包器。
- **Evidence**：[模板提交](https://github.com/duckdb/extension-template-rs/commit/abb7b2f3f954a47f90fc37a99cda3ce0618e6094)、[Makefile](https://github.com/duckdb/extension-template-rs/blob/abb7b2f3f954a47f90fc37a99cda3ce0618e6094/Makefile)、[Cargo.toml](https://github.com/duckdb/extension-template-rs/blob/abb7b2f3f954a47f90fc37a99cda3ce0618e6094/Cargo.toml)、[绑定发布](https://github.com/duckdb/duckdb-rs/releases/tag/v1.10505.0)。

实施时保留模板与依赖的许可声明，移除示例函数，产物统一命名 `duckaki.duckdb_extension`。首个目标是 Darwin arm64；其他平台和版本不出现在支持矩阵中，直到各自构建、加载和回归通过。

## R2：保留表函数合同，绑定阶段不抓取

- **Decision**：公开入口只有 `aki_stock_daily(symbol, start_date, end_date, source = ...)` 与 `aki_datasets()`。绑定检查参数并声明固定类型；首次执行获取完整有界批次，校验完成后才输出数据块，单扫描最大线程数设为 1。
- **Rationale**：`VTab` 可声明和读取命名参数，逻辑类型包含 Date 与 TimestampTZ；缺失/NULL `source` 仍需显式拒绝。每次扫描持有自己的缓冲区和游标，避免各输出块重复抓取；不把网络状态放在可变全局变量。
- **Alternatives considered**：不在 bind 访问网络；不边抓边输出；不增加泛型 provider、SQL 别名、一般谓词下推或自动缓存。
- **Evidence**：[VTab](https://github.com/duckdb/duckdb-rs/blob/v1.10505.0/crates/duckdb/src/vtab/mod.rs)、[BindInfo](https://github.com/duckdb/duckdb-rs/blob/v1.10505.0/crates/duckdb/src/vtab/function.rs)、[逻辑类型](https://github.com/duckdb/duckdb-rs/blob/v1.10505.0/crates/duckdb/src/core/logical_type.rs)。DATE 解码、NULL bitmap 和 TIMESTAMPTZ 微秒写入须通过实际 SQL 回归，不能由枚举存在推导已可用。

## R3：首版来源和标的范围精确收窄

- **Decision**：候选来源仍是 `eastmoney`，首版支持清单先只包含 `sz000001`，精确映射到 `secid=0.000001`；不承诺所有深圳股票或全 A 股覆盖。日期区间两端包含，最多 366 个自然日，涵盖完整闰年。其他标的在请求前拒绝并说明当前支持值；新增标的须补市场、单位、响应及回归证据。
- **Rationale**：满足 spec 至少一个标的、一个完整自然年的最小范围，避免将 AKShare 的代码首位判断当作市场识别合同。`sh600000` 等仅是未来验证对象。
- **Alternatives considered**：不接裸六位代码、不从首位数字猜市场、不额外查询股票列表、不自动切源。
- **Evidence**：[AKShare 固定实现](https://github.com/akfamily/akshare/blob/4771e6ea34f6139f226e2bf4fab88ce17579831d/akshare/stock_feature/stock_hist_em.py#L940-L1027)、[东方财富标的页](https://quote.eastmoney.com/concept/sz000001.html?from=classic)。参考实现不进入产品运行时。

## R4：协议、单位和空结果分别校验

- **Decision**：使用固定 HTTPS K 线 endpoint，`klt=101`、`fqt=0`，日期传 `YYYYMMDD`。`rc=0`、`data` 对象、匹配的 `code/market` 与数组 `klines` 构成可接受的响应形状；其中 `klines=[]` 是合法空结果。`data=null`、字段缺失、HTML、非零或缺失 `rc` 均失败，不从日历推断无数据。
- **Decision**：已核对的原始行顺序是日期、开、收、高、低、成交手数、成交元额及四个派生字段。成交手数以十进制值精确换算为股数，乘 100 后必须是非负、可表示的 BIGINT 整数；不得先转浮点数再截断。金额为人民币元，币种 CNY；OHLC/金额只接受有限值。
- **Rationale**：AKShare 文档将该接口量额标为手和元，深交所材料解释一手 100 股。此换算只适用于已验证的这个股票来源，不推广到其他资产。原始 11 字段与请求 `f116` 的差异要在解析测试中锁定。
- **Alternatives considered**：不照搬 AKShare 将多种空响应都变成空 DataFrame 的行为；不将缺失量额补零；不对其他来源无条件乘 100；不以整个标的的 `dktotal` 作为请求区间应有行数。
- **Evidence**：[接口文档](https://akshare.akfamily.xyz/data/stock/stock.html)、[深交所投教材料](https://investor.szse.cn/warning/activities/stockintroduction/t20190613_567790.html)、上述固定源码及本次响应摘要。

原始量额缺失仅接受经来源证据确认的显式缺失标记；未确认的 `"-"`、空字符串等不能自行当 NULL。合成 fixture 可以验证内部 NULL 语义，但不能证明原站采用某个标记。范围外的有效记录过滤掉；坏行、重复键和异常数值在整个响应校验时失败，不因后续过滤而隐藏。

## R5：同步、有界、无自动重试的 HTTP

- **Decision**：采用同步 `ureq = "=3.4.2"` 与 Rustls TLS；JSON 用 `serde` 1 / `serde_json` 1，日期用 `chrono` 0.4，解析所需的小版本及传递依赖由首个 `Cargo.lock` 固定。不新增 async runtime、调度器或请求线程池。
- **Decision**：网络全局 deadline 30 秒（DNS 到响应体读完），DNS 和连接各 5 秒且都受剩余总预算约束；零应用层重试、零重定向、一次请求、不分页。响应头最多 32 KiB，解码后正文最多 1 MiB，原始记录最多 4096 行，最终批次最多 366 行。读取上限加 1 字节/行用于检测超限，禁止静默截断；不只信任 Content-Length。解析与输出前继续检查 deadline。
- **Rationale**：ureq 明确提供覆盖整个调用的 `timeout_global`，同步入口适合单次小响应。请求库保留 TLS 校验；生产地址固定且不允许任意 URL 或 SQL 拼接。
- **Alternatives considered**：评估了 reqwest blocking；本方案优先 ureq 的同步模型和明确全局超时。首次不做自动重试、分页或大区间分割，因而没有“上一页成功、下一页失败”后输出部分数据的路径。
- **Evidence**：[ureq 配置](https://docs.rs/ureq/latest/ureq/config/struct.ConfigBuilder.html#method.timeout_global)、[reqwest blocking](https://docs.rs/reqwest/0.13.5/reqwest/blocking/struct.ClientBuilder.html)。固定 ureq 版本的慢速滴流、DNS、TLS、EOF 中断及正文限额仍须在实施时以本地受控响应验证。

若原站出现分页、截断标记或达到已知服务端返回上限，整批失败；单次年度探测不证明所有年份都完整。不得将 `dktotal`、`full` 等尚未明确的字段臆解为区间完整性证明。发现协议不能可靠满足年度请求时阻断该来源交付，更新协议验证，不静默补抓或换源。

## R6：复用 DuckDB 快照能力

- **Decision**：使用临时表保存会话快照，普通表/Parquet 跨会话保存；扩展只返回关系。没有 save API、自动持久化、版本表或缓存目录。
- **Rationale**：同批 `fetched_at` 在完整获取和校验完成后生成一次，保存时和业务字段一起保留。视图保存定义；重复调用/重扫描可能再发请求；LIMIT 不承诺减少抓取。
- **Alternatives considered**：不实现独立快照格式、自动 upsert 或增量同步。
- **Evidence**：[CREATE TABLE](https://duckdb.org/docs/stable/sql/statements/create_table)、[COPY](https://duckdb.org/docs/stable/sql/statements/copy)。2026-10-21 已在本机 CLI 1.5.5 用三条**合成记录**验证 JOIN、按月和币种汇总、普通表和 Parquet、新会话双向 EXCEPT ALL 比较；全部通过。月分组展示显式转 DATE。这不是扩展加载测试。

## R7：离线回归与真实来源冒烟分开

- **Decision**：Rust 核心 fixture 单测 + 模板 SQLLogicTest + Python 标准库 unittest 驱动的宿主/本地 HTTP 故障集成测试。真实来源冒烟单独显式触发，普通回归不访问公网。
- **Rationale**：只有故障注入能稳定覆盖 429、403、损坏 JSON、重复、慢响应、正文中断和取消。测试专用构建 feature `test-support` 允许一个只指向 loopback 的测试 endpoint；生产构建不包含这个覆盖入口，不增加公开 `fixture` 来源或 URL 参数。所有来源状态和 schema 仍走同一核心与绑定。
- **Alternatives considered**：不让测试依赖当天行情，不用 DataFrame 注册替代原生扩展；不提交来源许可不明的真实行情 fixture。合成样本和期望结果自行构造。

`test-support` 的 debug 产物与普通 release 产物隔离，正式发布检查 feature 列表及 loopback 覆盖入口不可用。测试依赖 Python DuckDB 固定为 1.5.5，仅供驱动测试和取消查询，产品运行时不依赖 Python。

## R8：许可与发布边界

- **Decision**：本次只产出计划，不添加推测的项目 LICENSE；保留所有上游许可归属。项目许可证选定、来源使用条件确认和真实数据再分发授权是公开发布前的门槛。合成 fixture 不携带真实行情。
- **Rationale**：代码许可与数据使用条件是不同事项；网页/API 可访问不等于允许任意复制和分发。现有 spec 已要求逐来源核对。
- **Alternatives considered**：不默认继承 AKShare 的 MIT 为本项目许可证，不将来源探测成功写成开放数据许可。
- **Evidence**：[东方财富法律声明](https://about.eastmoney.com/home/disclaimer)、[服务协议](https://about.eastmoney.com/home/protocol)、[AKShare 许可](https://github.com/akfamily/akshare/blob/4771e6ea34f6139f226e2bf4fab88ce17579831d/LICENSE)。本记录不作法律适用结论。

## R9：用已有 C API 补足宿主安全边界

- **Decision**：保留 Rust 模板入口及其生成的 FFI；两个表函数的注册、bind/init/scan 在一个小型 `bindings.rs` 内直接使用已有 C API，不 fork/vendoring 整个 duckdb-rs。原因是当前高层 `BindInfo` 没有已证实的 client-context 配置读取接口，不能编造安全包装方法或重解释其私有内存布局。
- **Decision**：bind 通过 `duckdb_table_function_get_client_context` 取得拥有明确释放责任的 context wrapper，用 `duckdb_client_context_get_config_option(..., "enable_external_access", ...)` 读取设置，检查类型、NULL 和返回状态；缺失/读取失败/false 一律拒绝网络读取。context 存入 bind state，在执行首次请求前重新读取，以覆盖 PREPARE 后关闭外部访问的情况。临时 value 立即 `duckdb_destroy_value`，context 由 bind state 的 drop callback `duckdb_destroy_client_context`，不重复释放。目录函数不需要外部访问授权。
- **Decision**：宿主负责 `interrupt()`/CLI Ctrl-C；在途同步 HTTP 不承诺即时中断，最迟受 30 秒网络 deadline 约束。取消后查询必须最终报 DuckDB 中断错误，不能成功返回缓冲记录；若精确版本实测不满足，G1 失败，不能带着该缺陷交付。首版不引入后台线程模拟取消。
- **Rationale**：v1.5.5 C API 可访问设置，但直接发 socket 的库不会自动继承 DuckDB 的访问限制。宿主的 Connection interrupt handle 也不等于扩展回调可用的取消令牌。此处需要少量显式绑定，不能只凭 VTab 的便利接口声称满足安全要求。
- **Alternatives considered**：不在回调里对同一连接递归执行 SQL；不使用缓存的“加载时允许”代替执行时检查；不关闭 TLS；不降级为无效取消或绕过安全设置。
- **Evidence**：[v1.5.5 C API 定义](https://github.com/duckdb/duckdb/blob/v1.5.5/src/include/duckdb.h)、[表函数 context 与状态生命周期](https://github.com/duckdb/duckdb/blob/v1.5.5/src/main/capi/table_function-c.cpp)、[Rust VTab](https://docs.rs/crate/duckdb/1.10505.0/source/src/vtab/mod.rs)、[宿主 interrupt handle](https://docs.rs/duckdb/1.10505.0/duckdb/struct.Connection.html#method.interrupt_handle)。

直接 C 回调不会自动享有高层 VTab 的 panic 包装：bind/init/scan 各自 `catch_unwind`，用对应 C error setter 转为脱敏的内部错误，构建保持 `panic=unwind`；析构回调本身不得 panic。注册失败、空指针、向量有效性、字符串及状态所有权都集中在该模块。G1 必须证明 context 跨 bind/init 的有效生命周期、prepared statement 行为、取消、重复扫描及释放；本次没有编译或执行这些回调。

## Phase 0 结论与实施关卡

工程路线、范围、协议解释、依赖、上限、测试和安全方案均已作出明确决策，没有留给实现者猜测的技术选项。以下是须以运行结果关闭的交付关卡，不是已经通过的验收：

| 关卡 | 实施时必须取得的证据 | 不通过时 |
| --- | --- | --- |
| G1 工程/宿主 | 固定组合构建加载；原生 C API 可用；无网络 bind；执行前外部访问检查；prepared statement；取消最终失败；panic/资源释放 | 停止网络接入交付，最小复现并修正绑定方案；不更换产品路线掩盖失败 |
| G2 来源/合同 | Rust HTTP/TLS 请求成功；年度及空响应验证；单位与字段逐项对照；来源使用条件核对；目录与实际能力一致 | 不登记或宣称支持 eastmoney；不自动切源 |
| G3 完整验收 | SC-001–SC-007、故障注入、快照新会话对比、文档示例及平台测试证据齐全 | 不宣称 MVP 已完成 |
| G4 公开分发 | 项目许可证、上游归属、数据使用/再分发条件、构建与支持矩阵 | 不公开发布扩展或真实数据样本 |

constitution 仍是未采纳模板，不能据此杜撰原则；planning 的前后检查均记录这一事实。源码和运行验证属于后续 tasks/implement，本次结束于设计文档。

## 本次运行证据

| 检查 | 观察 | 能证明的范围 |
| --- | --- | --- |
| 环境探测 | Darwin arm64；rustc/cargo 1.92.0；CLI 1.5.5；Python 3.11.6；GNU Make 3.81 | 工具存在及版本，未证明模板构建兼容 |
| 年度原站探测 | 系统 curl，TLS 校验开启；`secid=0.000001, beg=20240101, end=20241231, klt=101, fqt=0`；10 秒上限、1 MiB 上限；`rc=0`，code `000001`、market `0`；19198 字节、242 行，每行 11 字段；首日 2024-01-02、末日 2024-12-31 | 当时网络可达及返回形状，不是 Rust 扩展冒烟或完整性审计 |
| 周末原站探测 | 相同接口，20240106–20240107；183 字节；`rc=0`，身份完整，`klines=[]`；`dktotal=8500` | 此形状可表达合法空结果；dktotal 不是本区间条数 |
| 本地 SQL | 合成三行，含一行 NULL 量额；JOIN 得三行；月额 12000 / 22000；新会话普通表与 Parquet 双向 EXCEPT ALL 均零差异 | 保存、NULL、类型和 SQL 语法可行；未加载 DuckAki |

探测日期均为 2026-10-21（Asia/Shanghai）；HTTP 明细参数见[来源合同](contracts/eastmoney.md)。Python urllib 初次尝试因本机证书链失败；改用系统 curl 的正常证书验证后成功，没有关闭 TLS 校验。真实响应仅临时存放于系统临时目录，摘要读取后已删除，未提交行情数据。242 行不作为未来线上测试的固定断言。
