# 参考资料

核对日期：2026-10-20。以下为本次读取的一手资料；网页和主分支会更新，真正实现时应固定所依赖的版本或 commit。

## 原讨论

| 资料 | 用途与限制 |
| --- | --- |
| [ChatGPT：akshare](https://chatgpt.com/c/6ad6f653-b8f0-83e8-b19d-80dbcc7a0f20) | 原始会话；接口最初只返回部分用户消息 |
| [用户补充的分享页](https://chatgpt.com/share/6ad6ffcb-4724-83ec-ab0f-0e023cd701b6) | 浏览器读取到七条用户消息，补齐 Rust + DuckDB extension 的开场需求；助手回复仍无法加载，详见[讨论记录](discussion-notes.md) |

## AKShare 与数据语义

| 资料 | 本次确认的内容 | 后续用途 |
| --- | --- | --- |
| [AKShare 官方仓库](https://github.com/akfamily/akshare) | Python 财经接口库；列有 MIT 许可和 AKTools 入口 | 依赖接入、查找来源实现、核对代码复用条件 |
| [AKShare 股票数据](https://akshare.akfamily.xyz/data/stock/stock.html) | `stock_zh_a_hist` 的输入、来源、输出和单位 | 对照首个行情合同；注意同页不同接口口径可能不同 |
| [AKShare 数据说明](https://github.com/akfamily/akshare/blob/main/docs/data_tips.md) | 包含复权数据异常等具体示例 | 确定数据质量测试，避免把不熟悉的数值一律纠正 |

参考历史日线接口文档列出日/周/月周期，以及不复权、前复权和后复权选项；DuckAki 已采纳的 MVP 只选日线不复权子集。上游量额单位用于验证标准合同的成交股数与金额币种，不能直接照搬列名。文档中接口存在，不证明当前网络环境下可用，也不构成长期服务保证。

## DuckDB 工程与接入

| 资料 | 本次确认的内容 | 后续用途 |
| --- | --- | --- |
| [官方 Rust Extension Template](https://github.com/duckdb/extension-template-rs) | 基于 C Extension API 的 Rust 模板，仍标为 experimental | 首选工程验证入口；社区分发可行性须另行确认 |
| [duckdb-rs](https://github.com/duckdb/duckdb-rs) | Rust 绑定，提供扩展及表函数示例；加载产物需要元数据封装 | 核对绑定与扩展产物，固定版本后做兼容验证 |
| [Table Functions（C API）](https://duckdb.org/docs/current/clients/c/table_functions) | 原生表函数及绑定、初始化、执行回调接口 | 理解 Rust 绑定的底层生命周期 |
| [CREATE TABLE](https://duckdb.org/docs/current/sql/statements/create_table) | 可将查询结果显式物化为表 | 保存会话内快照或持久结果，复用 DuckDB SQL |
| [COPY](https://duckdb.org/docs/current/sql/statements/copy) | 查询结果或表的导出语法 | Parquet 保存示例，不另建扩展保存接口 |
| [C++ Extension Template](https://github.com/duckdb/extension-template) | 另一路扩展工程参考 | 对比构建与既有实现，不作为默认主线 |
| [Community Extension Development](https://duckdb.org/community_extensions/development) | 社区发布及维护流程，主要围绕 C++ 模板介绍 | 发布准备；不能直接推导 Rust 模板已完整接入 |
| [Extension Distribution](https://duckdb.org/docs/current/extensions/extension_distribution) | DuckDB 扩展分发说明 | 区分本地开发加载与正式分发 |
| [Import from Pandas](https://duckdb.org/docs/current/guides/python/import_pandas) | DuckDB Python 可消费 Pandas 数据 | 对照验证工具；不能替代原生扩展验收 |

本次没有编译模板、选择 DuckDB 版本或实际加载扩展；官方模板支持构建某个平台，也不表示 DuckAki 的网络依赖已通过该平台验证。

## 相关项目

| 资料 | 定位 | 对 DuckAki 的价值 |
| --- | --- | --- |
| [Scrooge McDuck 仓库](https://github.com/pdet/Scrooge-McDuck) | DuckDB 金融分析扩展，含行情扫描器 | 参考函数入口、来源接入和工程组织；本次没有审计其代码 |
| [scrooge 社区目录](https://duckdb.org/community_extensions/extensions/scrooge) | 发布目录中的扩展信息 | 核对目录功能与安装形态，不假设与主分支完全同步 |
| [finance 社区目录](https://www.duckdb.org/community_extensions/extensions/finance) | 本地、确定性的量化金融计算扩展；页面说明不调用远程行情服务 | 区分“获取数据”与“计算指标”，为暂缓指标库提供生态参考 |

以上只证明已有相关项目，不是完整竞品调查；不宣称“此前无人做过”。未找到针对 DuckAki 某项贡献的维护者承诺，也未代用户向上游发消息。

## 使用资料的约定

1. 接口文档用于确认语义，真实冒烟用于确认当时可用性，固定 fixture 用于回归；三者不能互相替代。
2. 移植或引用代码时补充精确文件、commit 与许可声明；本次未复制任何上游实现。
3. 接入每个新来源前核对访问和使用条件，尤其是缓存、批量抓取和再分发；本次未进行逐来源条款审查。
4. 后续发现原讨论引用的其他材料时补录，并校正受影响的提案。
