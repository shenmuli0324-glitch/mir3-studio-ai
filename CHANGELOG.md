# MIR3 Studio AI 更新记录

## 0.9.0 - 2026-08-26

- 建立 MIR3 System Kernel v1、Domain 数据库 schema v2 和固定通用 MCP 能力面。
- 将 33 个系统统一为独立版本化领域包，共享真实文件、资源、依赖、Draft、校验和能力契约。
- 新增左资源/文件/依赖、中领域视图/源码/Diff/校验、右归档 AI 会话的三栏开发工作区。
- MIR3 Core Plugin 升级为 protocol v2 Harness 兼容层，系统会话使用 cwd 创建并立即归档。
- MIR3 Core Plugin 成为唯一 Harness 兼容适配器；移除旧 Safe Files 插件和 iframe 无作用域文件命令转发，Studio 人工编辑改为系统版本绑定 Draft。
- 增加 Task Receipt、项目/个人/团队能力治理、任务作用域和领域包升级回滚基础设施。
- 地图改为统一领域包，移除旧地图专属前端状态、Tauri 命令和服务管线，同时复用已验证的无损地图算法。

## 0.2.1 - 2026-08-24

- 将 Safe Files 插件升级到 0.1.1，BIFF `.xls` 改为当前工作表完整读取和连续显示，不再使用分页交互。
- 增加工作簿解析缓存、源文件变化失效、有效区域裁剪和异常文件保护。
- XLS 前端改用虚拟滚动，只渲染可视行，同时保持完整工作表连续浏览。
- Harness、MIR3 Core、插件安装与原生编辑回退流程保持不变。

## 0.2.0 - 2026-08-24

- 新增可选 Harness 插件 `@mir3-studio/dsh-mir3-safe-files` 0.1.0；通过 Better Sidebar 公共扩展接口接管 TXT、Lua 和 BIFF XLS。
- 插件启用时，TXT/Lua 保存进入外置 Draft，保留 GB18030、UTF BOM 和原始换行；混合换行无法确定时拒绝写入。
- 新增 BIFF `.xls` 分页只读预览，并拒绝伪装为 `.xls` 的 OOXML/XLSX。
- Safe Files 插件通过 Harness 原生插件安装、挂载和卸载流程运行，不成为 MIR3 Core 的启动依赖；卸载后自然回退 Harness 原生查看器。
- MIR3 MCP 仍保持八项领域工具，并为 `mir3_draft_patch` 增加格式安全的 `text.replace`、`text.splice` 和 `lua.replace_function` 操作。
- Draft 原始字节写入、源哈希、revision、人工预览、快照和确认令牌继续共用同一安全链路。

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
