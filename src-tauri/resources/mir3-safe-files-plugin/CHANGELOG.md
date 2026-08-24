# Changelog

## 0.1.0

- 通过 Better Sidebar 公共扩展点注册高优先级 TXT、Lua、XLS 查看器。
- TXT/Lua 保存写入外置 Draft，保留 GB18030、BOM 与换行格式。
- 提供 BIFF `.xls` 分页只读预览并拒绝 OOXML 伪装文件。
- 在插件生命周期内为 MIR3 Session 启用只读策略，并拦截受保护文件的原生写入意图。
- 插件停用后不保留查看器、写入钩子或额外启动依赖。
