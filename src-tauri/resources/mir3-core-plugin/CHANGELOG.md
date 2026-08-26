# MIR3 Core Plugin 更新记录

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
