# MIR3 Studio AI 更新记录

## 0.1.1 - 2026-08-24

MIR3 Studio AI 使用 DeepSeek Harness 作为开源 Agent 基础设施之一。在其插件化架构基础上，我们独立开发了面向 996 传奇3的项目模型、知识体系、开发工具链和 AI 工作流。

- 引入 MIR3 Studio 产品外壳，并将 Harness 作为完整开发工作台接入。
- 增加 996 项目识别、领域扫描与外置索引。
- 增加 MIR3 Skill、八项 MIR3 MCP 领域工具及 Draft 安全工作流。
- 增加第一方 MIR3 Core Plugin，并按 Harness 插件生命周期管理 Workspace、Session 和 MCP 绑定。
- 安装包内置按平台锁定的 Core、Node.js 与 pnpm 运行时基线，首次启动无需下载 Core。
- 增加运行时更新的 Last Known Good 验收与失败自动回滚。
- 增加产品版本同步、插件协议审计、插件独立版本和本地更新记录门禁。

## 0.1.0 - 2026-08-23

- 完成 MIR3 Studio AI 独立品牌、应用标识、数据目录和发行仓库初始化。
- 保留 Tauri 桌面架构、Harness 核心、插件、Profile 和更新能力。
