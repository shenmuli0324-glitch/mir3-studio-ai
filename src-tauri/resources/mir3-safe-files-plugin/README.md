# MIR3 Safe Files

`@mir3-studio/dsh-mir3-safe-files` 是可选的第一方 Harness 插件。安装并启用时，它通过 Better Sidebar 的公开 `registerFileViewer` 接口接管 MIR3 项目中的 `.txt`、`.lua` 和旧版 BIFF `.xls`。

## 0.1.1 能力

- TXT/Lua：识别 ASCII、UTF-8、GB18030 及带 BOM 的 UTF-8/16/32。
- TXT/Lua：保留 BOM、CRLF/LF/CR 和未修改区域的原始字节；混合换行不明确时拒绝写入。
- 保存只写 MIR3 Studio 外置 Draft；正式项目文件保持不变。
- XLS：只接受 OLE2/BIFF `.xls`，完整显示当前工作表；伪装成 `.xls` 的 XLSX 会被拒绝。
- XLS：按源文件状态缓存解析结果、裁剪纯空白尾部，并以虚拟滚动连续显示全部有效行。
- XLS：超过 20 MiB、20,000 行、256 列或 500,000 个有效范围单元格时失败关闭，避免异常文件拖垮工作台。
- MIR3 Session 切换为只读，并阻止 Harness Agent 对受保护格式的原生写入意图。

## 模式边界

卸载或禁用本插件后，查看器和写入门禁随 Harness 插件生命周期注销，TXT/Lua 回退 Better Sidebar 原生编辑器，XLS 回退下载查看。现有 Draft、快照和索引不会被迁移或删除。

本插件不修改 Harness、Better Sidebar 或 Core 源码，也不是 `@mir3-studio/dsh-mir3-core` 的启动依赖。

版本变化见 [CHANGELOG.md](./CHANGELOG.md)。
