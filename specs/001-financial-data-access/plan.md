# Implementation Plan: 财经数据获取与分析

**Branch**: `master`（实际 Git 分支；尚无提交，本次未创建或切换分支） | **Date**: 2026-10-21 | **Spec**: [spec.md](spec.md)

**Input**: `specs/001-financial-data-access/spec.md`  
**Feature ID**: `001-financial-data-access`；setup-plan.sh JSON 中的 `BRANCH` 返回该 feature 定位名，不表示 Git 分支已切换。

**Status**: Phase 0 调研与 Phase 1 设计完成；尚无产品实现。版本已选定，构建/宿主安全/扩展端到端验收仍须按关卡取得证据。

## Summary

实现 Rust 原生 DuckDB 扩展，公开 `aki_stock_daily` 和离线 `aki_datasets()`；一个 crate 内分开核心、东方财富适配器和绑定。首版候选来源为显式 `eastmoney`，先限定已探测股票 `sz000001`，读取不复权日线，最多 366 个自然日。返回固定 12 列关系，完整校验后才输出，用户使用 DuckDB SQL 关联、聚合并显式保存快照。

固定官方 Rust 模板与 DuckDB 1.5.5 组合，使用同步有界 HTTP。绑定层通过已有 C API 实现无网络 bind、执行前外部访问检查和状态管理。首先验证固定数据构建加载及宿主边界，再接来源；一次 curl 探测不替代扩展验收。决策与证据见 [research.md](research.md)。

## Technical Context

**Language/Version**: Rust 1.92.0，edition 2024；Python 3.11 仅作构建和测试驱动。

**Primary Dependencies**: DuckDB CLI `v1.5.5`；`duckdb = "=1.10505.0"` 的 `loadable-extension` 及现有 FFI；官方模板 commit `abb7b2f3f954a47f90fc37a99cda3ce0618e6094`，ci-tools 子模块 `a285602bf745d6ceaa704b5df47f08b00ca296ae`；同步 `ureq = "=3.4.2"` / Rustls；`serde` 1、`serde_json` 1、`chrono` 0.4。首个实现提交固定工具链和 Cargo.lock，CI 按锁文件构建。

**Storage**: 扩展不写存储。每扫描内存缓冲完整批次；用户显式用 DuckDB 临时/普通表或 Parquet 保存。无自动缓存、迁移、后台同步。

**Testing**: Rust fixture 单测；模板 SQLLogicTest；Python 标准库 unittest + DuckDB 1.5.5 驱动 loopback HTTP、取消及新会话测试；真实来源冒烟单独触发。合成 fixture 不依赖公网。

**Target Platform**: 首个验证目标 Darwin arm64（本机）；CLI 1.5.5 精确版本。平台和版本只在实际加载及回归通过后列为支持，不承诺 WASM/其他 OS/任意 DuckDB ABI。

**Project Type**: 单 crate 的可加载 DuckDB 原生扩展；无独立 SDK、Web 服务或新增 CLI 产品。

**Performance Goals**: SC-003 离线目录连续 5 次、每次 <2 秒且零网络；一个完整自然年可端到端消费。远程延迟只记录实测，不承诺行情 SLA。

**Constraints**: 来源、标的、日期必填；无网络绑定；生产固定 HTTPS endpoint、TLS 校验开启；网络总 deadline 30 秒，DNS/连接各 5 秒且受总预算约束；一次逻辑请求、不分页、无应用层重试/重定向；响应头 ≤32 KiB，解码正文 ≤1 MiB，原始行 ≤4096、输出行 ≤366。全部校验后输出；取消可等待在途请求的剩余 deadline，但必须最终报错。详见[SQL 合同](contracts/sql.md)。

**Scale/Scope**: 一个数据集 × 一个来源 × 初始一个精确标的；每批一个区间、最多 366 个自然日。静态能力声明同时驱动目录与参数校验。不扩展股票列表、其他周期、复权、指标或交易。

## Constitution Check

*GATE: Phase 0 前检查，Phase 1 后复查。*

`.specify/memory/constitution.md` 全部仍为占位模板，没有已采纳原则。依照 spec 的 Assumptions，本次不修改 constitution，也不把模板示例的 TDD/审批要求当作治理规则。因此 constitution 专属 gate 不适用，不能声称符合不存在的原则。

| 需求/既有决策检查 | Phase 0 前 | Phase 1 后 |
| --- | --- | --- |
| Rust 原生扩展，核心独立于 DuckDB 类型，无 Python 行情桥接 | PASS：工程方向一致 | PASS：一个 crate、普通 Rust 核心类型 |
| 按数据集固定 schema、显式来源、无默认/自动切源 | PASS | PASS：两个表函数，详见合同 |
| 日期、单位、NULL、唯一性、全批失败和有限资源 | PASS：纳入研究问题 | PASS：模型、协议、上限及错误明确 |
| 离线目录、显式物化/保存、实际能力与声明一致 | PASS | PASS：静态支持清单和用户 SQL 保存 |
| 只规划实际 MVP，不把候选写成已交付 | PASS | PASS：仅 sz000001；G1–G4 独立标记运行证据 |

以上 PASS 表示**设计符合需求**，不是功能测试通过。G1–G4 的实施验证状态见 [research.md](research.md#phase-0-结论与实施关卡)。若运行证明不可满足合同，必须修正计划，不能以“设计 PASS”豁免缺陷。

## Project Structure

### Documentation (this feature)

```text
specs/001-financial-data-access/
├── spec.md
├── checklists/requirements.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
└── contracts/
    ├── sql.md
    └── eastmoney.md
```

`tasks.md` 由后续 speckit-tasks 生成，本次不生成任务列表或实现代码。

### Source Code (repository root，拟建)

```text
Cargo.toml
Cargo.lock
rust-toolchain.toml
Makefile
extension-ci-tools/          # 固定模板子模块
src/
├── lib.rs                   # 模板入口和注册
├── bindings.rs              # 两个表函数、最小 C API 边界、向量与所有权
├── core.rs                  # 请求、记录、能力清单、校验与错误
└── eastmoney.rs             # HTTP、协议解析及来源专属规范化
test/sql/
├── datasets.test
├── stock_daily.test
└── snapshots.test
tests/
├── fixtures/                # 合成响应、坏响应和期望结果
├── integration/
│   └── test_sql.py           # loopback 故障、安全、取消、快照新会话
└── live/
    └── smoke.sql            # 仅显式联网执行
docs/
└── validation/              # 版本/平台与脱敏冒烟摘要
```

**Structure Decision**: 当前仅 README/docs/specs/.specify，尚无 Cargo、源码或测试工程。引入固定模板必要文件，保留已有文档；不搬迁用户文件、不建立 workspace/provider 插件框架。Rust 纯核心单测放相应模块旁，避免额外测试框架。

## Execution Design

1. **能力和参数**：静态声明 `stock_daily / aki_stock_daily / eastmoney / sz000001`，以同一份定义生成支持清单和参数错误允许值。只有 G1/G2 达到可交付条件才将来源登记进普通构建的目录。纯 fixture 构建仅用于测试，不公开分发或声称真实来源已交付。
2. **绑定**：校验必填值、准确标的、日期有限且区间有效，声明 12 列；不访问网络。日线表函数通过原生 C bind context 读取外部访问配置，读取失败按拒绝处理。普通 EXPLAIN/DESCRIBE 不抓行情。
3. **执行**：bind state 持有已验证请求与受管理 context wrapper；init/首次 scan 再读当前外部访问设置，避免 PREPARE 后设置改变造成绕过。max_threads=1，每扫描独立有界状态。首次扫描完成 HTTP、响应和全批校验，再设置统一 fetched_at、按数据块写出；空批次保持同一 schema。
4. **失败与结束**：网络、协议、限额等错误走 DuckDB error setter；所有 C 回调捕获 panic，禁止 unwind 跨 ABI；context/value/vector/state 的释放在 bindings.rs 集中管理。取消通过宿主 interrupt，最迟等待剩余 HTTP deadline，最终必须为失败。G1 若不能证明这些行为则停止来源交付。
5. **保存**：临时表供反复分析；用户指定普通数据库和 Parquet 路径实现持久保存。只验证并记录 DuckDB 本身的覆盖/失败语义，不增加专属保存逻辑。

采用局部原生 C 回调是 R9 查明的高层配置读取缺口所需；继续使用模板入口、生成 FFI 与打包流程，不 fork 整个绑定库，不自写扩展格式。

## Implementation Order and Evidence

| 顺序 | 交付边界 | 必须验证后才继续 |
| --- | --- | --- |
| 1 | 固定模板、工具链、版本，固定 schema 的原生扩展 | G1 构建/加载；DATE、TIMESTAMPTZ、NULL；不联网 bind；prepared statement 安全与取消；FFI 错误和释放 |
| 2 | Rust 核心和合成 fixture | 参数零请求；边界、量额换算、缺失和损坏值、重复键、空结果、全批失败 |
| 3 | 来源 HTTP 和规范化连通 | G2 Rust 真实年度/空响应、TLS、单位对照；故障与规模上限；不静默截断、补行情或切源 |
| 4 | 目录、JOIN、聚合、快照和可运行说明 | G3：全部 SC，对比普通表/Parquet，新会话断网，发布构建无测试入口 |
| 5 | 公开发布准备 | G4：项目许可、来源使用条件、归属与实际通过的版本/平台矩阵 |

The Rust extension completed the explicit current G2 single-day, weekend, and full-year checks, and SC-007 is recorded in `docs/validation/g2-source.md` and `docs/validation/live-smoke.md`. License and distribution evidence is recorded separately in G4; point-in-time source success is not a long-term availability or redistribution guarantee.

## Validation Coverage

| 需求 | 设计位置 | 主要验收 |
| --- | --- | --- |
| FR-001–FR-004 | SQL 参数、core 校验、来源日期转换 | SC-001/002/004；无效输入请求计数 0、单日/闰年、边界/周末 |
| FR-005–FR-009 | data-model、固定 schema、完整批次 | SC-002；逐列/单位/NULL、非有限值、BIGINT 溢出、重复键 |
| FR-010–FR-013 | 两份合同错误、R5/R9 | SC-004；八类状态；慢速滴流、403/429、body 中断、超限、取消、外部访问禁用 |
| FR-014–FR-015 | 静态能力声明、目录 | SC-003；连续五次每次 <2 秒，零网络、无未来项 |
| FR-016 | quickstart 的 JOIN/月币种汇总 | SC-005；已知三条样本，逐项比较 |
| FR-017–FR-018 | 快照、DuckDB CTAS/COPY | SC-005/006；三次离线重查、新会话两种保存、NULL/追溯信息与类型一致；只读不创建文件 |
| FR-019–FR-020 | SQL 合同、research 范围/单位 | 视图、LIMIT、ORDER BY、来源兼容规则文档检查 |
| FR-021–FR-022 | 支持清单、公开示例、发布说明 | SC-001/007；无未支持选项；真实冒烟参数/时间/版本及范围声明 |

详细执行步骤和期望结果见 [quickstart.md](quickstart.md)。实施报告记录命令、环境、状态、请求计数、耗时和脱敏摘要；普通测试不依赖公网，真实网络失败不能伪装成离线通过。

## Complexity Tracking

没有已采纳 constitution 原则的例外。技术路径中的最小 C API 绑定例外已有 R9 的具体阻碍证据；其余采用单 crate、静态能力、同步 HTTP 和 DuckDB 原生保存，不引入额外服务或通用框架。

## Planning Completion

已运行 setup-plan.sh，读取 spec/constitution/template，汇总研究并生成模型、两个合同及 quickstart；Phase 1 后复查设计一致。`.specify/extensions.yml` 不存在，before_plan/after_plan 均无 hook 需要执行。下一阶段是 speckit-tasks；本次未构建或实现扩展。
