# MIR3 Core Plugin

`@mir3-studio/dsh-mir3-core` 是 MIR3 Studio AI 随安装包提供的唯一第一方 Harness 兼容适配器。

## 职责

- 使用 Harness 公共 `workspaces` 和 `sessions` 服务创建工作区与会话。
- 将 Studio 当前 996 项目同步到 Harness 工作台。
- 为系统开发区创建绑定 `cwd` 的 Session，创建后立即通过公共 `archiveSession` 接口归档，避免出现在普通工作区列表中。
- 通过协议 v2 提供系统会话创建、恢复、Prompt、取消、交互回复、快照订阅和完成通知。
- 快照和完成通知只从工具结果投影结构化 Draft/校验/资源变更，Studio 会再次核验项目、系统、任务、会话、序列和真实 Draft 绑定后才刷新 Diff 或消费 `returnTo` 深链。
- 系统任务升级为全局任务时创建正常可见的新 Session，并接收 Studio 签发的短期多系统作用域、固定插件版本、组合 Draft 和结构化摘要上下文。
- 通过 Harness 已有 MCP Client 连接 MIR3 MCP，不实现第二套文件、编辑器、终端或会话系统。
- 不向 iframe 暴露无任务作用域的文件写入命令；AI 写入统一使用作用域凭证和外置 Draft，Studio 人工编辑由 Tauri 领域工作区承担。
- 服务端为 `mir3-system-` 和 `global-` 保留会话命名空间：只将 Studio 管理的系统/全局会话切换为只读，并在文件意图层拒绝其直接改写已验证项目根目录内的任何真实文件；普通 Harness 会话不受该策略影响。
- 无法确认系统会话的项目根目录时，系统 AI 写入失败关闭；MCP 只写项目外 Draft，用户确认后的项目应用仍由 Studio/Tauri 完成。
- 插件完成 `apply` 后向 Studio 发送 ready 加载信号；该信号本身不推进 Core LKG，Studio 仍须完成协议描述、归档会话、MCP 和领域注册表 canary。

## Harness 兼容边界

- 服务端入口必须默认导出带 `apply` 的插件对象。
- 浏览器入口必须通过 Harness `window.__ModuleLoader__.load` 注册，并从 factory 返回 `module.exports`。
- 仅接受 `document.referrer` 推导出的精确父窗口 origin，并同时校验 `event.source`、协议版本和完整任务标识。
- Studio 请求和 Core 响应分别按项目、任务、会话维护严格单调的 `sequence`，旧响应不会覆盖新快照。
- 仍在运行的系统/全局任务可接收新短期作用域令牌；取消或完成会话后停止续租并撤销令牌，不回传或复制完整聊天历史。
- 快照只投影可结构化克隆的数据；不把 Harness Runtime 对象、函数或响应句柄暴露给 Studio。
- 只使用 Harness 公共注入能力，不修改 Core 源码、官方包或 `node_modules`。
- 不读取 Harness DOM 文案，不拦截或模拟 Harness 界面点击。
- Studio 启动时以 Profile 本地 `file:` 依赖安装，并通过受管理的 `cordis.patch.yml` 块挂载。

版本变化与用户可见更新记录见 [CHANGELOG.md](./CHANGELOG.md)。
