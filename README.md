<p align="center">
  <img src="public/brand/mir3-studio-ai.svg" width="96" alt="MIR3 Studio AI" />
</p>

<h1 align="center">MIR3 Studio AI</h1>

<p align="center">面向 996 传奇3引擎开发者的 AI 桌面开发环境</p>

<p align="center"><samp><a href="./README.en.md">English</a> · <strong>中文</strong></samp></p>

> 当前版本为 `0.9.9`。MIR3 Studio 通过统一系统开发内核、33 个领域包、归档系统会话和安全 Draft 工作流开发由 996 项目管理器创建的项目。

MIR3 Studio AI 使用 DeepSeek Harness 作为开源 Agent 基础设施之一。在其插件化架构基础上，我们独立开发了面向 996 传奇3的项目模型、知识体系、开发工具链和 AI 工作流。

## 产品能力

- Tauri 2 + React 原生桌面外壳，支持 Windows、macOS 和 Linux。
- 安装包携带目标平台已锁定的 Node.js、pnpm 与 MIR3 AI Core 基线，首次启动无需从 GitHub 下载核心。
- 多版本核心下载、切换、健康检查和进程生命周期管理。
- 隔离的 Profile、插件安装/升级/卸载及异常恢复。
- 996项目识别、外置索引、真实文件与领域资源映射、知识治理、Draft预览与版本快照。
- 统一三栏系统工作区：左侧真实文件/资源/依赖，中间领域视图/Diff/校验，右侧归档 AI 会话。
- 一个系统内核和 33 个独立版本化领域包；领域包可单独审计、升级、禁用和回滚，不争用 Harness UI 生命周期。
- 十二项固定通用 MCP 工具向系统 AI 和全局 Harness 暴露同一份资源、依赖、Draft Diff、校验和能力目录。
- 成功任务生成 Task Receipt，并可经预览和校验提升为项目、个人或团队的版本化安全能力。
- Studio 领域源码编辑统一先创建并绑定当前系统版本的外置 Draft，再执行保留 GB18030/BOM/换行的 TXT、Lua 修改和 BIFF XLS 查看；Harness AI 写入只走带任务作用域的通用 MCP。
- 应用自更新仅连接 MIR3 Studio AI 的 GitHub Releases。

## 产品接口

| 项目 | 值 |
| --- | --- |
| 应用名 | MIR3 Studio AI |
| 核心显示名 | MIR3 AI Core |
| 版本 | 0.9.9 |
| Tauri identifier | `ai.mir3.studio` |
| 数据目录 | `~/.mir3-studio-ai` |
| 开发数据目录 | `~/.mir3-studio-ai.dev` |
| 数据目录覆盖变量 | `MIR3_STUDIO_HOME` |

## 快速开始

从 [GitHub Releases](https://github.com/shenmuli0324-glitch/mir3-studio-ai/releases) 下载对应平台安装包并启动。首次运行从安装包内校验并安装运行时基线，核心服务在本机回环地址运行；联网仅用于后续主动检查更新。

系统要求：Windows 10+（64 位）、macOS 10.15+，或支持 AppImage / DEB 的 Linux。

## 开发

```bash
corepack pnpm install
corepack pnpm tauri dev
```

Apple Silicon macOS 的固定交付命令为 `pnpm package:mac`。该命令一次完成 `.app`、`.dmg`、签名结构、镜像校验和 SHA-256 输出；未配置 Developer ID 时默认使用可真机测试的 ad-hoc 签名。

正式发布若启用领域包远程候选，必须在编译时同时注入 HTTPS 索引地址 `MIR3_DOMAIN_PACK_INDEX_URL` 和 Base64 编码的 32 字节 Ed25519 公钥 `MIR3_DOMAIN_PACK_ED25519_PUBLIC_KEY`；构建脚本会拒绝单边配置、非 HTTPS/带凭据地址和明显无效的公钥。启用后桌面端在启动 60 秒后及每 6 小时后台检查并验签、暂存候选，但绝不自动激活；激活仍需用户确认。未配置时后台任务安静关闭，手动远程检查以 `DOMAIN_PACK_UPDATE_NOT_CONFIGURED` 失败关闭，本地随包候选、确认激活与回滚仍正常可用；仓库不内置虚构的发布源或密钥。

33 个领域包使用证据门控的引擎自动泛化：只归一化 SemVer、`v` 前缀 SemVer 与 `major.minor` 三类明确别名，并同时要求真实 996 项目目录、领域选择器/内容指纹与资源 Schema 校验。未识别或不兼容的引擎只能查看和诊断，Draft 写入与最终 Apply 都会拒绝。

详细说明见 [中文开发文档](./docs/DEVELOPMENT.zh.md)。运行时基线的升级和按平台验收规则见 [基线发布约定](./docs/runtime-baseline-policy.md)，版本更新见 [CHANGELOG](./CHANGELOG.md)。

## 数据与隐私

MIR3 Studio AI 不读取、迁移或删除其他产品的数据目录。外部只接受 `MIR3_STUDIO_HOME`；核心协议需要的环境映射由桌面端在子进程内部完成。

AI Core 与插件具备本地文件和命令执行能力，请仅在可信项目和可信插件环境中使用。

## 第三方与许可

项目许可证见 [LICENSE](./LICENSE) 与 [LICENSE.details](./LICENSE.details)。第三方组件和上游归属集中记录在 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
