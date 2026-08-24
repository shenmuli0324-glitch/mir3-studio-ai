# MIR3 Studio AI 运行时基线发布约定

MIR3 Studio AI 的各平台安装包必须携带一套已经锁定的运行时基线，使新安装在不访问 GitHub 的情况下也能安装并启动 MIR3 AI Core。

## 基线晋级规则

1. `runtime-baseline.lock.json` 是安装包运行时基线的唯一事实来源。
2. 正常发行构建只接受目标平台的 `validation` 为 `approved`。
3. `testing` 候选仅可用于开发测试安装包，并且构建时必须显式设置 `MIR3_BASELINE_ALLOW_UNVALIDATED=1`。
4. 修改 Core tag/commit、Node.js 或 pnpm 版本、下载地址、SHA-256 中的任何一项，都视为新的基线候选；受影响平台必须恢复为 `testing` 并重新验收。
5. 只有安装包在对应真实平台完成下述验收后，才允许把该平台晋级为 `approved`。晋级不需要改动已锁定的组件版本，只修改验收状态和记录。
6. 一个系统或 CPU 架构的验收结果不能批准另一个系统或架构；Windows、macOS Apple Silicon、macOS Intel 和 Linux 分别维护状态。
7. 应用运行后下载的 Core 更新候选不会改写当前安装包内的基线。只有经过代码评审的锁文件变更和新一轮真实平台验收，才会让后续安装包采用新基线。
8. 更新候选只有在 Harness 工作台和第一方 MIR3 Core Plugin 均成功就绪后才提交为 Last Known Good；启动或插件加载失败时自动恢复更新前的 Core。

## 真实平台验收证据

- 在没有既有 MIR3 Studio 运行时数据的环境中全新安装。
- 下载完成安装包后断网，首次启动仍可完成 Node.js、pnpm 与 Core 安装。
- Core 进程成功启动且 HTTP 健康检查通过。
- Harness 工作台和原有设置页面正常加载。
- MIR3 Core Plugin 按 Harness 插件协议注册，不出现 loader 错误。
- MIR3 MCP sidecar 与 MIR3 996 Skill 均存在并可用。
- 正常退出后，应用拥有的 Core/MCP 进程被清理。
- 从上一个 `approved` 基线执行升级和失败回滚测试。

发布工作流不得绕过本约定。未验收开关只用于生成收集上述证据的开发测试安装包，不能用于正式 tag 发布。
