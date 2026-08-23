## [P1] 验证安装后在各平台生成mir3 shim并注册PATH
[测试类型] 功能
[前置条件] release 构建；分别准备好 Windows 与 macOS/Linux 测试环境；两平台安装均已完成且 `cli_link_enabled` 为 true
[测试步骤] 1. Windows 平台检查 `%LOCALAPPDATA%\mir3-studio-ai\bin` 目录及注册表 `HKCU\Environment\Path`。2. Unix 平台检查 `~/.local/bin` 目录及 `~/.zshrc`/`~/.bashrc` 中的注入块。3. 两平台均重新打开终端执行 `mir3 --version`。
[预期结果] 1. Windows：`mir3.cmd` 与 `mir3.ps1` 已生成，`HKCU\Environment\Path` 已追加 `%LOCALAPPDATA%\mir3-studio-ai\bin`。2. Unix：`~/.local/bin/mir3` shim 已生成且权限为 `-rwxr-xr-x`，rc 文件已写入 `# >>> MIR3 Studio AI mir3 >>>` 开头、`# <<< MIR3 Studio AI mir3 <<<` 结尾的注入块。3. 两平台终端均输出 dsh 版本号、退出码为 0。

## [P4] 验证shim文本为纯英文且路径转义正确
[测试类型] 兼容性
[前置条件] release 构建；应用数据目录含特殊字符（Windows 用户名含 `%`，如 `C:\Users\100%test\...`；Unix 用户名含 `'`，如 `/home/o'brien/...`）
[测试步骤] 1. 读取 `mir3.cmd`，检查 `set "APP_DIR=..."`、`set "DSH_HOME=..."` 行的 `%` 转义。2. 读取 `mir3.ps1`，检查 `$appDir = '...'`、`$dshHome = '...'` 行的 `'` 转义。3. 读取 Unix `mir3` shim，检查 `APP_DIR='...'` 行的 `'` 转义。4. 检查三个 shim 文件全文是否只含 ASCII 字符，并新开终端执行 `mir3 --version`。
[预期结果] 1. `mir3.cmd` 中含 `%` 的目录已写成 `%%`（如 `set "APP_DIR=C:\Users\100%%test\..."`），不存在未转义的单独 `%`。2. `mir3.ps1` 中含 `'` 的目录已写成 `''`（如 `$appDir = 'C:\Users\o''brien\...'`）。3. Unix `mir3` shim 中含 `'` 的目录已写成 `'\''`（如 `APP_DIR='/home/o'\''brien/...'`）。4. 三文件全文不含中文或非 ASCII 字符、可由英文代码页正确解析，`mir3 --version` 输出版本号无乱码、退出码为 0。

## [P2] 验证shim优先使用本机兼容Node并在不兼容时回退内置Node
[测试类型] 功能
[前置条件] release 构建；本机 PATH 前置 Node v22.22.0；内置 Node 已随应用安装至运行时目录；`cli_link_enabled` 为 true
[测试步骤] 1. 在 PATH 前置本机 Node 目录后新开终端执行 `mir3 --version`。2. 将本机 Node 切换为不兼容版本 v21.7.0（PATH 中仅有 v21.7.0）后新开终端执行 `mir3 --version`。
[预期结果] 1. shim 解析到本机 `node`（v22.22.0 满足 v22.15+ 条件），`mir3 --version` 输出版本号、退出码为 0。2. 本机 Node v21.7.0 不满足兼容条件，shim 回退使用内置 `%APP_DIR%\runtime\node.exe`，`mir3 --version` 仍输出版本号、退出码为 0。

## [P2] 验证用户已安装pnpm时pnpm shim优先转发用户pnpm
[测试类型] 功能
[前置条件] release 构建；用户自行安装的 pnpm 已在 PATH 中（如 `C:\Program Files\nodejs\pnpm.cmd` 的版本为 v9.15.0）；捆绑 pnpm 已随应用安装至 `dependencies/pnpm/bin/pnpm.cjs`
[测试步骤] 1. 新开终端执行 `pnpm --version`。2. 确认 shim 未覆盖用户 pnpm，执行 `pnpm --version` 前后用户 pnpm 的全局配置目录保持不变。
[预期结果] 1. `pnpm --version` 输出用户 pnpm 的版本号 v9.15.0、退出码为 0，而非捆绑 `dependencies/pnpm/bin/pnpm.cjs` 的版本。2. pnpm shim 转发到用户 pnpm（`where pnpm` 命中用户路径、排除 `%LOCALAPPDATA%\mir3-studio-ai\bin`），用户 pnpm 配置与环境未变。

## [P3] 验证本机已有pnpm或内置pnpm已安装时跳过捆绑安装
[测试类型] 兼容性
[前置条件] release 构建；安装前用户 PATH 已存在 pnpm（`pnpm --version` 可输出 v9.15.0）或应用数据目录 `dependencies/pnpm` 已为已安装状态
[测试步骤] 1. 在满足跳过条件的环境下完成应用安装，观察安装流程是否仍下载或安装捆绑 pnpm。2. 新开终端执行 `pnpm --version`。
[预期结果] 1. `Pnpm::check_installed` 判定用户 pnpm 已安装或捆绑 pnpm 已就绪，安装流程跳过捆绑 pnpm 的重复安装（安装日志出现跳过或复用提示）。2. `pnpm --version` 正常输出 v9.15.0、退出码为 0，确认复用已有 pnpm、命令可用。
