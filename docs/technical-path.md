# 技术路径与选型

状态：**工程选型、实现与 G1–G4 验收已完成**，当前证据限定于 Darwin arm64 / DuckDB CLI `v1.5.5`。已采用官方 `extension-template-rs`（commit `abb7b2f3f954a47f90fc37a99cda3ce0618e6094`）、`extension-ci-tools` 子模块（commit `a285602bf745d6ceaa704b5df47f08b00ca296ae`）、Rust 1.92.0。
## 主线

```text
上游财经数据 → Rust 获取与规范化 → Rust DuckDB 扩展 → SQL 关系
```

Rust 数据模块处理来源协议和数据合同，DuckDB 绑定层转换参数、管理扫描状态并输出数据块。产品是可在 DuckDB CLI 加载的财经数据扩展；Python AKShare 只用于参考与可选对照，不作为产品运行时桥接。首版不设计或发布独立 Rust SDK。

SQL 入口为按数据集命名的 `aki_stock_daily` 与离线目录 `aki_datasets()`；参数、字段和执行约定以 [Spec](spec.md) 为准，不将 AKShare 原接口名暴露为另一套 SQL 别名。

## 工程起点

官方 [extension-template-rs](https://github.com/duckdb/extension-template-rs) 基于 C Extension API，当前仍标为 experimental。本项目固定并验证了该模板与目标 DuckDB 组合；支持范围以实际 CI 与验证报告为准。
`duckdb-rs` 提供 Rust 扩展能力及 `hello-ext` 示例。本项目固定模板 commit、Rust 工具链、绑定版本与目标 DuckDB 版本，并按这组版本测试。

首版先采用一个 crate 内的普通模块区分核心与绑定，避免提前建设多包 workspace。只有真正需要独立消费核心 API 时，再拆库和设计发布边界。

## 构建、加载与测试

以下官方 Rust 模板工作流已用于本仓库构建与测试：

```sh
make configure
make debug
make test_debug
```

模板文档说明构建需要 Rust，并使用 Python、Make 等辅助工具；这不等于行情运行时依赖 Python AKShare。发布构建及其测试使用相应 release 目标，具体步骤按固定模板版本记录。[Rust 模板说明](https://github.com/duckdb/extension-template-rs)

普通 `cargo build` 生成的共享库不等于可加载的 `.duckdb_extension`；还需要扩展元数据 footer。复用模板完成产物封装，避免手写一套格式处理。[duckdb-rs 构建与加载说明](https://github.com/duckdb/duckdb-rs#building-and-loading-extensions)

本地开发通过显式路径加载开发产物；生产分发另行验证，不把开发时加载未签名二进制的设置写成长期安装方案。

验证分三层：Rust 核心 fixture 单测、DuckDB SQL 集成回归、独立触发的真实来源冒烟。Python DataFrame 能被 SQL 查询，只能作为对照工具，不能替代原生扩展加载验收。

## 数据源实现

首版已实现 A 股单标的、不复权日线；Eastmoney `sz000001` 是当前核验通过的唯一来源/标的组合。`stock_zh_a_hist` 仅作字段与协议语义参考，不是 DuckAki 的 SQL 函数名。

适配器将已确认的上游成交量单位转为 `volume_shares`，将成交金额与 `currency` 一起输出。来源不可用或响应不满足合同就明确失败，不换源；离线目录只列真实实现的组合，不充当实时健康检查。

AKShare 用于定位来源、核对参数、字段及单位；若参考其代码，记录具体版本与许可声明。按来源逐个实现 Rust 适配器，不承诺全仓自动翻译，也不直接复制 Python 的动态调用机制。

当前 HTTP 实现使用 30 秒单调总 deadline、DNS 解析与连接各 5 秒上限、32 KiB 响应头上限、1 MiB 正文上限和 366 条记录上限；不做应用层重试。TLS 与解析使用固定依赖，配置及故障覆盖见 [G2/错误验证](validation/acceptance.md)。

绑定层实现并验证了无网络绑定、外部访问限制、PREPARE 后重新检查、协同取消和失败后资源可用性；详细行为见[架构](architecture.md)与[G1 验收记录](validation/g1-engine.md)。

## 替代方案的位置

| 方案 | 当前用途或状态 |
| --- | --- |
| Python AKShare + DuckDB | 可选对照与样本工具；不作为首版交付路线 |
| C++ 官方模板 | 了解 DuckDB 扩展工程和比较已有实现；不作为默认迁移目标 |
| AKTools HTTP 服务桥接 | 首版不引入；会增加服务部署与运行时依赖 |
| 手写裸 C API 绑定/扩展打包 | 仅在模板存在具体阻碍且有证据时再评估 |

Rust 模板若遇到问题，先用最小固定数据函数区分绑定、构建和网络依赖问题。确需改变语言或交付形态时再重新决策，不把临时 Python 演示或 C++ 移植当作满足原目标。

## Scrooge McDuck 的位置

[Scrooge 社区目录](https://duckdb.org/community_extensions/extensions/scrooge) 展示金融数据扫描器，适合参考其 SQL 接入方式；[项目仓库](https://github.com/pdet/Scrooge-McDuck) 可供工程与协议研究。仓库主分支和社区发布版本可能不同。

DuckAki 已选择独立建仓。Scrooge 是相关项目，不是隐含 fork 或运行时依赖；没有得到维护者对某项 PR 的接受承诺，不能把上游合并作为交付条件。

## 开发前验证关卡

| 验证项 | 所需证据 |
| --- | --- |
| Rust 扩展工程 | 固定版本下构建并在 DuckDB CLI 加载，表函数返回固定 fixture |
| 来源可用性 | 一个标的、短日期范围的真实响应，记录来源、时间与参数 |
| 数据合同 | 逐列确认类型、成交股数换算、金额币种、日期边界、NULL 和重复记录 |
| 数据集目录 | 离线返回已实现的组合，与读取入口支持的来源一致 |
| 运行时边界 | 无需 Python AKShare/AKTools；错误和取消不破坏宿主 |
| 测试边界 | 核心可脱离 DuckDB 测试；普通回归无需公网 |
| 分发兼容性 | 目标平台、扩展元数据与所需 C API/ABI 能力通过实际加载验证 |

C Extension API 的兼容边界按固定版本和验证结果记录，既不套用 C++ 扩展的版本规则，也不承诺任意 DuckDB 版本通用。

## 发布与许可

先发布有证据支持的版本/平台组合。进入 DuckDB 社区目录仍是待验证目标；Rust 模板 README 对社区集成的说明尚未提供“DuckAki 已可发布”的保证，需另外核对现行流程。[Rust 模板](https://github.com/duckdb/extension-template-rs)、[社区扩展开发说明](https://duckdb.org/community_extensions/development)

项目许可证已确定为 Apache-2.0，见根目录 [LICENSE](../LICENSE)。上游代码复用许可（MIT）与客户端直接获取的上游数据访问条件分别记录在 [G4 清单](validation/distribution.md)。具体阶段与退出条件见 [Roadmap](roadmap.md)。
