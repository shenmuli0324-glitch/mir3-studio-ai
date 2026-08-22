<p align="center">
  <img src="public/brand/mir3-studio-ai.svg" width="96" alt="MIR3 Studio AI" />
</p>

<h1 align="center">MIR3 Studio AI</h1>

<p align="center">
  面向 996 传奇3引擎开发者的 AI 桌面开发环境 ——<br />
  基于 <a href="https://github.com/deepseek-ai/deepseek-harness">DeepSeek Harness</a>，保留完整 Harness、插件与更新能力。
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Windows%20%7C%20macOS%20%7C%20Linux-black?style=flat-square" alt="Windows | macOS | Linux" />
</p>

<p align="center">
  <samp><a href="./README.en.md">English</a> · <strong>中文</strong></samp>
</p>

<p align="center">
  <img src="./docs/images/hero-zh.png" width="100%" alt="MIR3 Studio AI 中文界面预览" />
</p>

> [更多浏览图片](/docs/PREVIEW.md)

> 当前阶段仅完成品牌化改造，不包含新的 MIR3 专属功能；原有桌面端能力保持不变。

## 功能

- ⚡️ **零环境** — 首次启动自动装配内置 Node 运行时与 Harness 内核；本机已有兼容 Node / Pnpm 时直接复用，不修改已有的系统环境。
- 🔄 **内核更新** — 每次启动同步上游最新 Harness 版本，上游更新无需重装即生效；支持多版本核心的下载、切换与卸载，切换后重启服务。
- 🖥️ **应用配置** — 统一的配置对话框（调试 / 档案 / 插件 / 核心），界面按钮中英双语本地化，并适配暗色模式。
- 🗂️ **档案隔离** — 在应用配置中新建 / 切换 / 删除彼此隔离的档案，插件、补丁与设置各自独立，互不干扰。
- 🧩 **插件管理** — 插件面板只读展示已安装插件，出现异常时提供升级 / 卸载入口，错误详情实时同步。
- 🪶 **原生轻量** — Tauri 2 外壳（非 Electron）：更小的安装包、更低的内存占用、原生窗口。Windows / macOS / Linux，中英双语界面。
- ⌨️ **命令行集成** — 安装后自动注册 `dsh` 命令（`*/bin`），新开终端即用；不覆盖你已有的 shell 配置。
- 🧭 **启动引导** — 首次启动可选装推荐插件并实时查看安装日志；随时跳过，之后也能从侧边栏重新打开。
- 🚀 **自更新** — 应用独立检查 GitHub 最新版并下载安装包；开发 / 生产构建的端口与数据目录彼此隔离。

## 预设插件

首次启动引导中提供的插件，按需勾选安装：

- [DSH Win Terminal Inspector](https://github.com/clearkurt/dsh-win-terminal-inspector) — Windows 极简模式修复
- [DSH Tauri](https://github.com/hairyf/dsh-tauri) — 桌面端消息桥：提供与 Tauri 2 外壳的通信通道（推荐）
- [DSH Market](https://github.com/dsh-market/dsh-market) — 可视化插件市场：浏览、搜索并一键安装社区插件（推荐）
- [DSH Better Sidebar](https://github.com/omdsh-dev/DSH-better-sidebar) — 类 VSCode 右侧栏（资源管理器/编辑器/终端/Git/浏览器），按会话隔离（推荐）
- [DSH Notification](https://github.com/omdsh-dev/dsh-notification) — 回合完成时桌面通知：按结果分别开关，支持包含/排除关键词规则
- [DSH Session Context Menu](https://github.com/baihejiangnan/dsh-session-context-menu) — DSH 封装端右键菜单：为会话、工作区、输入框和链接补充常用操作

> 你想收录新的插件作为预设？修改 [preset-plugins.json](https://github.com/hairyf/deepseek-harness-desktop/blob/main/src-tauri/resources/preset-plugins.json) 并提交 PR，审核通过后将在将来版本新增为预设插件。

## 快速开始

从本项目发布页面下载对应平台的 MIR3 Studio AI 安装包，安装后启动即可。

首次运行会下载 Node 运行时与 Harness 内核，随后直接进入 `http://127.0.0.1:3080` 的 Harness 界面；此后完全本地运行，无需联网。

**系统要求：** Windows 10+（64 位）· macOS 10.15+ · Linux（AppImage / `.deb`，基于 Ubuntu 22.04 构建，兼容 22.04 及更新版本）· 首次运行需要网络

## 开发

想参与开发？参见 [docs/DEVELOPMENT.zh.md](./docs/DEVELOPMENT.zh.md)。

## 工作原理

```text
┌──────────────────────────────────────────────┐
│ Tauri WebView (React)                        │
│   安装状态机 → 下载进度 → iframe              │
│   加载 dsh Web 界面 + 侧边栏控制              │
└──────────────────────┬───────────────────────┘
                       │ invoke 命令 + 事件
┌──────────────────────┴───────────────────────┐
│ Tauri Rust 后端                              │
│   service/download  安装器 + 解压            │
│   service/core      Harness 核心多版本管理   │
│   service/profile   dsh 档案管理             │
│   service/plugin    插件卸载 / 升级          │
│   service/cli       dsh 命令 shim + PATH     │
│   service/update    桌面端自更新             │
│   service/workflow  dsh 进程生命周期         │
│   task              dsh 健康检查             │
└──────┬───────────────────────────┬───────────┘
       │                           │
  runtime/ (Node.js v22.22.0)   dependencies/dsh/ (发行版)
       └─────────────┬─────────────┘
                     ▼
   dsh --profile <档案> --host 127.0.0.1 --port 3080
                     │  DSH_HOME=~/.dsh
                     ▼
        http://127.0.0.1:3080/  ← 内嵌界面
```

Harness 发行版由 [deepseek-harness-pkg](https://github.com/hairyf/deepseek-harness-pkg) 构建发布。每次启动都会对比最新发行版，本地过期时自动重新下载；GitHub 不可达时保留本地安装。通过 CLI 全局安装的本地核心会被优先使用。

## 说明

> [!WARNING]
> **开发预览** — 上游 `dsh` 仍在快速迭代，存在破坏性变更；本项目同步跟随。

> [!IMPORTANT]
> **macOS Gatekeeper** — 应用未公证，首次启动需在系统设置 → 隐私与安全性 → 仍要打开 放行一次。

> [!NOTE]
> **安全声明** — `dsh` 具备本地代码执行能力。仅供学习 / 研究 / 测试，请在可信、隔离的环境中使用。

## 相关项目

- [deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) — 上游 `dsh` agent 平台
- [deepseek-harness-pkg](https://github.com/hairyf/deepseek-harness-pkg) — 预打包 Harness 发行版（本应用下载源）
- [deepseek-harness-desktop](https://github.com/hairyf/deepseek-harness-desktop) — MIR3 Studio AI 的上游桌面端基础
- [n8n-desktop](https://github.com/tangtao646/n8n-desktop) — 参考实现

## License

[MIT](./LICENSE)，附加[非商用条款](./LICENSE.details)。保留 deepseek-harness-desktop 上游版权声明。
