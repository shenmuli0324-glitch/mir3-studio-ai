---
name: mir3-996-development
description: 开发、分析、修改或验收996传奇3项目时使用，覆盖客户端、引擎、TXT、Lua、配置、Draft和测试证据流程。
---

# 996 传奇3开发工作流

当前会话由 MIR3 Studio AI 绑定到一个已登记的 996 项目工作区。项目根通常直接包含 `客户端` 和 `引擎`。

## 工具边界

- 996 的 `.txt`、`.ini`、`.cfg`、`.lua` 等脚本可能使用 GBK/CP936。读取这些领域文件时必须先用 `mcp__mir3__mir3_resource_query` 定位稳定资源 ID，再用 `mcp__mir3__mir3_resource_get` 读取解码后的内容；不要因为 Harness 原生 `read` 仅支持 UTF-8 就判断文件损坏。
- 只有已确认是 UTF-8 的源码和文档才使用 Harness 原生文件工具。GBK/GB18030 文件的修改必须走 MIR3 Draft 与结构化能力，以保留原编码、BOM、换行和字节稳定性。
- 使用 `mcp__mir3__mir3_system_list` 和 `mcp__mir3__mir3_system_describe` 确认目标领域、插件版本、文件覆盖和依赖。
- 使用 `mcp__mir3__mir3_resource_query`、`mcp__mir3__mir3_resource_get` 和 `mcp__mir3__mir3_dependency_resolve` 查询当前领域的真实文件资源与关系。
- 计划修改正式项目时，先调用 `mcp__mir3__mir3_draft_open`，再用 `mcp__mir3__mir3_domain_operate` 或版本固定的领域能力写入外置 Draft。唯一例外是 GUI Designer 的专用 UI 会话：它使用四个 `mir3_gui_*` 工具修改应用私有工作副本，并由 Studio 保存节点写入项目。
- 使用 `mcp__mir3__mir3_capability_list`、`mcp__mir3__mir3_capability_describe` 和 `mcp__mir3__mir3_capability_invoke` 复用安全结构化能力。
- 使用 `mcp__mir3__mir3_validate` 检查项目、领域和 Draft；正式应用只能在 Studio 中由用户确认。

## 安全要求

- MCP Draft 与 GUI AI 私有工作副本都不会直接修改正式项目；不得用脚本绕过 Studio 的修改预览、人工确认、保存节点和备份门禁。
- 不修改或替换 996 项目管理器、GameCenter、客户端启动程序和引擎二进制。
- 不把索引、知识库、Draft 或备份写入 996 项目目录。
- 不把未经测试的经验写成 ACTIVE 知识；运行结果应先作为证据候选交给用户审核。

## 开发顺序

1. 确认项目状态和引擎版本。
2. 查询领域索引和相关 ACTIVE 知识。
3. 按文件编码路由读取：UTF-8 源码使用 Harness 原生工具，996 领域脚本使用 MIR3 资源工具。
4. 创建 Draft 并提交结构化修改。
5. 查看 Diff，执行 996 领域校验。
6. 交由用户在 Studio 中预览、备份并应用。
7. 使用 996 项目管理器启动客户端和服务端进行验收。
