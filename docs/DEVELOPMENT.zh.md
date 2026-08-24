# 开发

MIR3 Studio AI 是 **Tauri 2 + React 18** 应用：前端位于 `src/`，Rust 后端位于 `src-tauri/`。

## 环境要求

| 工具 | 版本 |
| --- | --- |
| Node.js | 20+ |
| Rust | 1.77.2+ |
| pnpm | 9+ |

以及平台编译工具链：

- **Windows** — MSVC 构建工具 + WebView2
- **macOS** — Xcode Command Line Tools
- **Linux** — WebKit2GTK

## 常用命令

```bash
pnpm install      # 安装依赖
pnpm dev          # 前端开发服务器（Vite）
pnpm typecheck    # 前端 TypeScript 检查
pnpm tauri dev    # 调试模式运行桌面端
pnpm tauri build  # 构建安装包
```

`pnpm tauri build` 会根据 `runtime-baseline.lock.json` 下载、校验并嵌入当前目标平台的运行时基线。正式构建只接受 `approved` 平台；`testing` 候选只能在收集真实平台验收证据时显式设置 `MIR3_BASELINE_ALLOW_UNVALIDATED=1`。任何 Core、Node、pnpm、下载地址或 SHA-256 变化都必须重新进行该平台验收，详见 [运行时基线发布约定](./runtime-baseline-policy.md)。

功能开发完成后先运行 `pnpm version:bump -- patch` 自动同步应用版本，再执行 `pnpm release:check`。内置插件还必须独立升级 SemVer、维护本地更新记录并遵守 [Harness 插件开发约定](./harness-plugin-development-policy.md)。所有测试完成后提交并推送当前 Git 分支。

后端检查（在 `src-tauri/` 下执行）：

```bash
cargo check
cargo test
```

## 小贴士

- 调试模式使用 **3081** 端口，正式版使用 **3080** —— 两者互不冲突，可以同时运行已安装版本与开发构建。
