---
name: mir3-996-ui-development
description: 在 MIR3 Studio GUI Designer 的专用 AI 会话中分析或调整 GUIExport Lua UI 时使用，覆盖控件上下文、属性和位置修改、素材引用查询、静态校验与保存节点边界。
---

# 996 GUI Designer AI 工作流

当前会话只操作 Studio 同步到应用私有目录的 GUI 工作副本。它不是 33 个领域系统之一，也不会直接写入 996 项目或正在运行的游戏。

## 强制工具边界

- 先调用 `mcp__mir3__mir3_gui_context`，传入当前 `workspaceToken` 和 `path`，读取控件树、选择节点、静态素材引用、诊断和精确 `workingRevision`。
- 修改时只调用 `mcp__mir3__mir3_gui_operate`。每次都传入同一 `workspaceToken`、`path` 和上一步返回的精确 `expectedRevision`。
- `setPosition` 同时修改节点的 `x`、`y`；`setProperty` 只接受解析器确认可安全替换的布尔、数字、字符串或 nil 属性。
- `addNode` 只允许新增 Panel、Image、Text、Button 四类核心组件；`addBehavior` 只允许向已解析控件添加预设 Timeline 淡入或 Action 淡入。冲突、未知节点、原始 Lua 表达式和不支持参数必须失败关闭。
- 需要了解图片、字体或其他素材时，只调用 `mcp__mir3__mir3_gui_asset_query` 查询当前文档已经引用的逻辑路径。它不会读取、解密或导出游戏资源。
- 完成一组操作后调用 `mcp__mir3__mir3_gui_validate`，并携带最新 `expectedRevision`。存在 error 级诊断时不要建议保存。

## 写入与保存边界

- AI 只允许修改 `GUIExport/*.lua` 的私有工作副本；`GUILayout` 和游戏资源只读。
- 不使用 shell、通用文件工具、编辑器写入或脚本修改项目文件，也不把私有工作区内容复制到项目目录。
- MCP 操作成功只表示 Studio 内的工作副本已改变。只有用户在 GUI Designer 中执行“保存”后，Studio 才创建保存节点并把内容安全写回项目。
- 未执行保存前，不得声称项目、客户端或游戏中的 UI 已改变。保存后如需 `Ctrl+F5`、客户端重载、服务端或游戏重启，应明确告诉用户手动操作。
- 工作版本冲突表示 Studio 画布或另一个 AI 回合已经更新内容；停止覆盖，重新获取上下文并让用户确认新的修改意图。

## 推荐顺序

1. 获取当前 GUI 上下文与选择节点。
2. 用自然语言复述目标节点和拟修改属性，避免同名控件误改。
3. 逐个执行受限操作，每次沿用返回的新版本。
4. 查询所需的现有素材逻辑引用，不尝试解密资源。
5. 对最新版本执行静态校验。
6. 告知用户修改仍在私有工作副本，并建议在画布确认后保存为节点。
