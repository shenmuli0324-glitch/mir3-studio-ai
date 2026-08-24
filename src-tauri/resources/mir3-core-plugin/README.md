# MIR3 Core Plugin

`@mir3-studio/dsh-mir3-core` 是 MIR3 Studio AI 随安装包提供的第一方 Harness 系统插件。

## 职责

- 使用 Harness 原生 `workspaces` 服务创建工作区并启动 Session。
- 将 Studio 当前 996 项目同步到 Harness 工作台。
- 接管“添加工作区”入口，将目录选择交给 Studio/Tauri 的项目边界校验。
- 通过 Harness 已有 MCP Client 连接 MIR3 MCP，不实现第二套文件、编辑器、终端或会话系统。
- 插件完成 `apply` 后向 Studio 发送 ready 信号，作为 Core 更新提交和失败回滚的验收点。

## Harness 兼容边界

- 服务端入口必须默认导出带 `apply` 的插件对象。
- 浏览器入口必须通过 Harness `window.__ModuleLoader__.load` 注册，并从 factory 返回 `module.exports`。
- 只使用 Harness 公共注入能力，不修改 Core 源码、官方包或 `node_modules`。
- Studio 启动时以 Profile 本地 `file:` 依赖安装，并通过受管理的 `cordis.patch.yml` 块挂载。

版本变化与用户可见更新记录见 [CHANGELOG.md](./CHANGELOG.md)。
