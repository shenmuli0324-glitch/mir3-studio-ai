<p align="center">
  <img src="public/brand/mir3-studio-ai.svg" width="96" alt="MIR3 Studio AI" />
</p>

<h1 align="center">MIR3 Studio AI</h1>

<p align="center">面向 996 传奇3引擎开发者的 AI 桌面开发环境</p>

<p align="center"><samp><a href="./README.en.md">English</a> · <strong>中文</strong></samp></p>

> 当前版本为 `0.7.1`。MIR3 Studio 直接打开由 996 项目管理器创建的项目，并通过领域索引、Skill、MCP 与安全 Draft 工作流辅助开发。

MIR3 Studio AI 使用 DeepSeek Harness 作为开源 Agent 基础设施之一。在其插件化架构基础上，我们独立开发了面向 996 传奇3的项目模型、知识体系、开发工具链和 AI 工作流。

## 产品能力

- Tauri 2 + React 原生桌面外壳，支持 Windows、macOS 和 Linux。
- 安装包携带目标平台已锁定的 Node.js、pnpm 与 MIR3 AI Core 基线，首次启动无需从 GitHub 下载核心。
- 多版本核心下载、切换、健康检查和进程生命周期管理。
- 隔离的 Profile、插件安装/升级/卸载及异常恢复。
- 996项目识别、外置索引、知识治理、Draft预览与版本快照。
- MIR3 Skill 与八项领域 MCP 工具复用现有 Harness 文件、编辑和会话能力。
- 可选 MIR3 Safe Files 插件在启用时提供保留 GB18030/BOM/换行的 TXT、Lua Draft 编辑和 BIFF XLS 只读预览；卸载后回退 Harness 原生编辑模式。
- 应用自更新仅连接 MIR3 Studio AI 的 GitHub Releases。

## 产品接口

| 项目 | 值 |
| --- | --- |
| 应用名 | MIR3 Studio AI |
| 核心显示名 | MIR3 AI Core |
| 版本 | 0.7.1 |
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

详细说明见 [中文开发文档](./docs/DEVELOPMENT.zh.md)。运行时基线的升级和按平台验收规则见 [基线发布约定](./docs/runtime-baseline-policy.md)，版本更新见 [CHANGELOG](./CHANGELOG.md)。

## 数据与隐私

MIR3 Studio AI 不读取、迁移或删除其他产品的数据目录。外部只接受 `MIR3_STUDIO_HOME`；核心协议需要的环境映射由桌面端在子进程内部完成。

AI Core 与插件具备本地文件和命令执行能力，请仅在可信项目和可信插件环境中使用。

## 第三方与许可

项目许可证见 [LICENSE](./LICENSE) 与 [LICENSE.details](./LICENSE.details)。第三方组件和上游归属集中记录在 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
