# MIR3 Core Plugin 更新记录

## 1.0.4 - 2026-08-27

- 系统 Session 只有在归档成功后才允许打开、发送 Prompt 或接受并发控制命令，防止失败路径污染普通 Harness 工作区。
- 创建或归档失败会释放任务所有权；重试复用已创建的残留 Session 并再次归档，避免生成重复 Session。
- 系统与全局 Session 恢复时会重新归档并恢复事件订阅；Studio 重启后可继续接收 Snapshot、提问、完成和错误事件。
- 新增 `globalSession.resume`，持久化全局任务、组合 Draft、未完成计划和回到具体系统的深链状态。

## 1.0.3 - 2026-08-26

- Core 候选升级 canary 通过公共 Runtime 创建、打开并归档一个非 managed 普通 Session，避免仅验证系统会话旁路。
- Studio 使用一次性项目和数据库启动真实 MIR3 MCP sidecar，调用只读系统工具和官方只读领域能力；失败时禁止推进 LKG 并恢复上一版本。

## 1.0.2 - 2026-08-26

- 固定系统/全局会话的项目、系统、任务与 Session 所有者，拒绝跨任务控制、缺失 payload、空保留 ID 与旧序列。
- 补齐结构化 Draft/校验/资源变更回传和 `returnTo` 深链，并以可执行测试固定 create → archive → open → prompt 的顺序。
- Core 直接写保护改为可独立审计的策略函数，证明普通 Harness 会话保持原行为；候选 canary 失败后恢复 LKG、重启 Harness 并刷新 iframe。

## 1.0.1 - 2026-08-26

- 全局 MIR3 会话纳入与系统会话相同的只读和 Draft-only 策略，禁止直接改写已验证项目根目录内的任何真实文件且不影响普通 Harness 会话。
- 协议 v2 的 Studio 请求与 Core 响应按会话分别使用严格单调序列，并完整校验 `sessionId`。
- 全局会话要求 Studio 保留的 `global-` 会话 ID，拒绝无作用域的伪全局请求。
- 系统与全局会话的快照/完成事件只投影结构化 Draft 结果，支持安全回传版本、校验、资源变更和 Studio `returnTo` 深链；长任务可接收短期作用域续租且结束即撤销。
- 会话命令固定绑定项目、系统、任务和 Session 所有者，拒绝跨任务控制、缺失 payload、空保留 ID 和重放序列；归档顺序与普通 Harness 写权限加入可执行回归测试。

## 1.0.0 - 2026-08-26

- 新增协议 v2 能力探测和精确来源通信。
- 新增归档 cwd 系统会话、快照、提问回复、取消和恢复。
- 新增系统任务到可见全局 Session 的结构化上下文交接。
- 将 `plugin.ready` 限定为加载信号，LKG 由完整 canary 决定。
- 移除关键工作区流程对 Harness DOM 文案的依赖。
- 作为唯一 Harness 兼容适配器运行，不再配套第二个文件桥插件；AI 文件写入统一经任务作用域和 MIR3 MCP Draft。
- 合并系统会话专用 sandbox/fs write policy；仅保护 `mir3-system-` 会话，普通 Harness 会话保持原权限，作用域无法验证时系统 AI 失败关闭。

## 0.2.0 - 2026-08-24

- 接入 996 项目激活消息，并同步 Harness Workspace 与 Session。
- 接管 Harness“添加工作区”入口，目录选择统一经过 Studio 的项目边界校验。
- 增加 MIR3 MCP 项目绑定和插件 ready 握手。
- 增加插件本地说明文档与设置页更新记录入口。
- 增加 Harness 服务端、客户端 loader 契约自动审计。

## 0.1.0 - 2026-08-23

- 建立第一方系统插件包及 Profile 本地安装方式。
- 按 Harness/Cordis 插件结构提供服务端 default export 与浏览器 ModuleLoader 入口。
