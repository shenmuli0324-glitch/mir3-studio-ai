# MIR3 Studio AI 更新记录

## 0.9.9 - 2026-08-27

- 修复 macOS 原生包中 Tauri 自定义协议父窗口的精确 origin 规范化，使 MIR3 Core Plugin 1.0.5 能回传 ready、Bridge 描述并执行完整兼容 canary；非可信不透明来源仍保持拒绝。

## 0.9.8 - 2026-08-27

- 完成旧版真机 Profile 的安全迁移：移除已退役的 Safe Files bundle，同时保留 `@mir3-studio/dsh-mir3-core` 的 Profile 本地依赖与受管理 Cordis patch 挂载，避免把非 bundle 插件错误注册到 Harness bundle 层。
- macOS 原生烟测覆盖旧 Profile 启动迁移，并兼容系统仅报告根控制台锁定状态的情形；真实锁屏仍会严格拒绝运行 UI 验收。

## 0.9.7 - 2026-08-27

- 修复从旧版真机 Profile 升级时仍加载已合并的 `@mir3-studio/dsh-mir3-safe-files` bundle，导致 MIR3 AI Core 在监听端口前退出的问题；启动迁移现在同时清理正式的 `dsh.profile.bundles` 与早期兼容结构。

## 0.9.6 - 2026-08-27

- 33 个领域包统一升级至 1.3.1：字段别名参与真实读取、Schema、引用校验与安全写入，引用依赖由字段契约闭包生成，全部 33 条运行时规则由共享 fail-closed 执行器实际运行。
- 组合 Draft 增加跨进程锁、崩溃 Journal、原子恢复和 Studio 联合审查；能力、Memory 与领域包指针升级共享可恢复治理快照。
- Harness Core Plugin 升级至 1.0.4，补齐系统及全局归档会话恢复、重订阅、任务持久化、作用域撤销重试和失败补偿。
- 任务、天赋、活动、沙巴克和跨服增加专用领域摘要；依赖图、Snapshot 恢复及跨系统深链统一进入三栏工作区。
- 真实项目验收器升级为 schema v2，只有三份一次性副本合计完成只读校验和 Draft 应用/恢复各 33/33 才能生成通过报告。
- Draft Apply 与 Task Receipt/Memory 共享可恢复的崩溃 Journal；Kernel 签发不可伪造的 Apply Receipt，Snapshot 回滚同步撤销 Receipt、Memory 和衍生能力。
- 系统转全局使用凭证脱敏的结构化语义摘要，原始聊天与短期 scope token 不再进入 Receipt、Memory 或任务本地存储。
- 155 个官方写操作完成统一生命周期分类，33/33 系统各有代表能力通过真实 Diff、Apply、字节变化与 Snapshot 恢复；全局 3/8/33 组合工作流使用紧凑交接摘要保持 MCP 预算内。

## 0.9.1 - 2026-08-26

- 将领域资源升级为记录级模型，执行 Manifest 映射、稳定资源 ID、来源定位和跨领域引用诊断，并在左侧资源区直接呈现。
- 补齐全部官方写操作的安全编译、Draft 覆盖校验、组合 Draft 原子应用、并发 revision 冲突和只读故障降级。
- 增加表格、曲线、日历、排行、关系、流程、时间线、空间和跨服拓扑等差异化中央视图；地图领域包升级至 1.0.1。
- MIR3 Core Plugin 升级至 1.0.1，支持多系统全局任务、结构化 Draft 回传、可信深链、会话序列校验和短期作用域续租。
- 用户能力改由成功回执、固定领域包和服务端操作证据编译，并在隔离 Draft 中真实重放后方可确认激活。
- 增加 33 包启停/升级/回滚、3/8/33 系统组合、万文件索引、MCP 上下文预算和损坏包隔离门禁。

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
