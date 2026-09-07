---
name: mir3-996-development
description: 开发、分析、修改或验收996传奇3项目时使用，覆盖客户端、引擎、TXT、Lua、配置、工作副本、保存节点和测试证据流程。
---

# 996 传奇3开发工作流

当前会话由 MIR3 Studio AI 绑定到一个已登记的 996 项目工作区。项目根通常直接包含 `客户端` 和 `引擎`。

## 工具边界

- 996 的 `.txt`、`.ini`、`.cfg`、`.lua` 等脚本可能使用 GBK/CP936。读取这些领域文件时必须先用 `mcp__mir3__mir3_resource_query` 定位稳定资源 ID，再用 `mcp__mir3__mir3_resource_get` 读取解码后的内容；不要因为 Harness 原生 `read` 仅支持 UTF-8 就判断文件损坏。
- 只有已确认是 UTF-8 的源码和文档才使用 Harness 原生文件工具。GBK/GB18030 文件的修改必须走 MIR3 工作副本与结构化能力，以保留原编码、BOM、换行和字节稳定性。
- 使用 `mcp__mir3__mir3_system_list` 和 `mcp__mir3__mir3_system_describe` 确认目标领域、插件版本、文件覆盖和依赖。
- 使用 `mcp__mir3__mir3_resource_query`、`mcp__mir3__mir3_resource_get` 和 `mcp__mir3__mir3_dependency_resolve` 查询当前领域的真实文件资源与关系。
- 计划修改正式项目时，先调用 `mcp__mir3__mir3_working_copy_open`，再用 `mcp__mir3__mir3_domain_operate` 或版本固定的领域能力写入应用私有工作副本。人工编辑与 AI 必须复用当前系统的同一个工作副本；项目文件只能由 Studio 的“保存”操作写入。
- 使用 `mcp__mir3__mir3_capability_list`、`mcp__mir3__mir3_capability_describe` 和 `mcp__mir3__mir3_capability_invoke` 复用安全结构化能力。
- 使用 `mcp__mir3__mir3_working_copy_inspect` 按需查看修改，使用 `mcp__mir3__mir3_validate` 检查项目、领域和工作副本；正式写入只能由用户在 Studio 中保存。

## 安全要求

- MCP 与 GUI AI 私有工作副本都不会直接修改正式项目；不得用脚本绕过 Studio 的保存、必要风险确认、保存节点和备份门禁。
- 不修改或替换 996 项目管理器、GameCenter、客户端启动程序和引擎二进制。
- 不把索引、知识库、工作副本、保存节点内容或备份写入 996 项目目录。
- 不把未经测试的经验写成 ACTIVE 知识；运行结果应先作为证据候选交给用户审核。

## 开发顺序

1. 确认项目状态和引擎版本。
2. 查询领域索引和相关 ACTIVE 知识。
3. 按文件编码路由读取：UTF-8 源码使用 Harness 原生工具，996 领域脚本使用 MIR3 资源工具。
4. 打开或复用当前领域工作副本并提交结构化修改。
5. 执行 996 领域校验；需要时查看修改，高风险或跨系统操作必须明确确认。
6. 交由用户在 Studio 中保存并创建保存节点；需要恢复时使用“撤回上次保存”。
7. 使用 996 项目管理器启动客户端和服务端进行验收。
