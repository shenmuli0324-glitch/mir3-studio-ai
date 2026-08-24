# MIR3 Studio AI Harness 插件开发约定

MIR3 Studio AI 的插件能力必须建立在 Harness 公共插件机制之上。目标是让 Studio 扩展 996 传奇3领域能力，同时保留 Harness 原有启动、Profile、Workspace、Session、Agent、文件、编辑器、终端和 MCP Client 流程，并能跟随未来 Harness Core 更新。

## 强制结构

- 每个随包插件都是独立 npm 包，必须有稳定 SemVer、`README.md`、`CHANGELOG.md`、服务端入口和浏览器端入口。
- 服务端入口默认导出函数，或带 `apply` 方法的对象，交给 Harness/Cordis loader 管理生命周期。
- 浏览器入口使用 Harness 的 `window.__ModuleLoader__.load`，factory 必须显式返回 `module.exports`。
- 所需能力通过 `dsh.client.inject` 声明；只调用 Harness 注入的公开服务。
- 第一方插件由 Studio 复制到活动 Profile 的本地 `file:` 依赖源，并使用带边界标记的 `cordis.patch.yml` 块挂载；不能写进或修改 Harness 官方包。
- 插件卸载、Profile 切换、Core 退出时必须释放自身监听器和子进程。

## 禁止事项

- 修改 MIR3 AI Core/Harness 源码或发布包中的官方 `node_modules`。
- 复制实现第二套 Workspace、Session、文件工具、编辑器、终端、Agent、Profile 或 MCP Client。
- 依赖 Harness 未公开的 DOM 结构完成关键业务；界面拦截只能作为入口适配，真正的路径和权限校验必须由 Tauri/Rust 完成。
- 在插件参数中接受未经 Studio 校验的项目根目录，或绕过 Draft、预览、确认和备份门禁直接修改正式项目。

## 版本和更新记录

- 修复兼容问题递增 patch；增加向后兼容能力递增 minor；破坏性协议变化递增 major。
- 每次插件功能变化都必须先更新插件版本，并在 `CHANGELOG.md` 顶部增加同版本记录。
- 第一方插件的设置列表项必须能离线打开本地说明和更新记录，不跳转外部仓库。
- Studio 应用只要包含功能或插件变化，也必须同步递增产品版本。

## 验收门禁

```bash
pnpm version:check
pnpm brand:audit
pnpm plugin:audit
pnpm typecheck
pnpm lint
pnpm exec vitest run
cargo check --manifest-path src-tauri/Cargo.toml --workspace --locked
cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked
```

除插件自身功能外，还必须回归：Core 启动、Harness 工作台、原设置界面、Profile、插件列表、Workspace/Session、MCP Client、退出清理以及 Core 更新失败回滚。

开发任务完成后必须检查变更范围、提交 Git 并推送当前分支；本地产物和用户文件不得进入提交。
