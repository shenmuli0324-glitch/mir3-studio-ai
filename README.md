<p align="center">
  <img src="public/brand/mir3-studio-ai.svg" width="96" alt="MIR3 Studio AI" />
</p>

<h1 align="center">MIR3 Studio AI</h1>

<p align="center">面向 996 传奇3引擎开发者的 AI 桌面开发环境</p>

<p align="center"><samp><a href="./README.en.md">English</a> · <strong>中文</strong></samp></p>

> 当前版本为 `0.1.0`。本阶段完成独立品牌、数据与发行体系收敛，不包含新的 MIR3 专属功能；原有核心、插件、档案和更新能力保持可用。

## 产品能力

- Tauri 2 + React 原生桌面外壳，支持 Windows、macOS 和 Linux。
- 首次启动自动准备 Node.js 与 MIR3 AI Core。
- 多版本核心下载、切换、健康检查和进程生命周期管理。
- 隔离的 Profile、插件安装/升级/卸载及异常恢复。
- 仅提供 `mir3` 用户命令；插件所需 pnpm 位于应用私有工具目录。
- 应用自更新仅连接 MIR3 Studio AI 的 GitHub Releases。

## 产品接口

| 项目 | 值 |
| --- | --- |
| 应用名 | MIR3 Studio AI |
| 核心显示名 | MIR3 AI Core |
| 版本 | 0.1.0 |
| Tauri identifier | `ai.mir3.studio` |
| CLI | `mir3` |
| 数据目录 | `~/.mir3-studio-ai` |
| 开发数据目录 | `~/.mir3-studio-ai.dev` |
| 数据目录覆盖变量 | `MIR3_STUDIO_HOME` |

## 快速开始

从 [GitHub Releases](https://github.com/shenmuli0324-glitch/mir3-studio-ai/releases) 下载对应平台安装包并启动。首次运行需要联网准备运行环境，完成后核心服务在本机回环地址运行。

系统要求：Windows 10+（64 位）、macOS 10.15+，或支持 AppImage / DEB 的 Linux。

## 开发

```bash
corepack pnpm install
corepack pnpm tauri dev
```

详细说明见 [中文开发文档](./docs/DEVELOPMENT.zh.md)。

## 数据与隐私

MIR3 Studio AI 不读取、迁移或删除其他产品的数据目录。外部只接受 `MIR3_STUDIO_HOME`；核心协议需要的环境映射由桌面端在子进程内部完成。

AI Core 与插件具备本地文件和命令执行能力，请仅在可信项目和可信插件环境中使用。

## 第三方与许可

项目许可证见 [LICENSE](./LICENSE) 与 [LICENSE.details](./LICENSE.details)。第三方组件和上游归属集中记录在 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
