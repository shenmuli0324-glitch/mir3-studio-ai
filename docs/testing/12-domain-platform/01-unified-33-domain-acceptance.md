# 统一 33 领域平台验收记录

本文记录 MIR3 Studio 统一领域内核、Harness Compatibility Adapter、通用 MCP 与 33 个领域包的可重复验收证据。自动化通过不替代真实 Harness UI、Windows 物理设备和游戏运行环境验收。

## 交付边界

- Kernel API：v1。
- Domain 数据库：schema v2，迁移前备份，失败恢复，只读降级。
- Studio–Harness Bridge：protocol v2，精确 origin、source、DTO 与 sequence 校验。
- Harness Core Plugin：单一兼容适配器，不修改 Harness/Core、官方插件或 `node_modules`。
- 领域包：33 个独立 `kind=domain` 包，均具备 current、previous、LKG、禁用和候选激活状态。
- 通用 MCP：只提供领域查询、Draft、Diff、结构化操作、能力与校验工具，不提供任意 Shell 或任意路径写入。

## 33 包契约矩阵

`pnpm domain:audit` 必须逐包输出证据行并满足以下条件：

- 33/33 Manifest、package、README、CHANGELOG、SemVer 与兼容声明完整。
- 33/33 具备真实文件投影、内容指纹、路径别名、资源 Schema、稳定资源 ID 与依赖声明。
- 33/33 具备 create、clone、batch-update、replace-reference 四类安全操作。
- 所有写操作都要求闭合参数 Schema、expectedRevision、Draft 绑定、预览、校验和确认。
- 33/33 具备 syntax/schema、uniqueness、range、reference、client-engine、runtime 与 unknown-format 校验。
- fixture canary 对 valid、invalid、expected diagnostics 做精确匹配，并 dry-run 全部官方操作。
- 地图使用 `map-canvas-v1` 和通用 Draft；跟踪源码中不存在旧地图页面、`MapSystemService` 或旧 `map_*` Tauri 命令。

当前注册表包含 194 个官方能力，其中 155 个为 Draft 写能力。依赖审计报告的两个强连通分量属于资源交叉引用，不是插件加载顺序或执行依赖；Kernel 对缺失或损坏包仍按单包故障隔离处理。

## Kernel 与治理证据

主要自动化测试：

- 数据迁移：`schema_v2_migration_creates_recoverable_backups`、`failed_schema_migration_restores_backup_without_partial_ddl`、`newer_registry_schema_starts_readonly_without_hiding_projects`。
- Draft：`draft_never_changes_project_until_apply_and_snapshot_restores`、`scoped_drafts_reject_foreign_files_and_composite_apply_is_atomic`、并发 revision 与组合提交 CAS 测试。
- 规模：`ten_thousand_file_index_has_bounded_queries_and_stable_pagination`、`ten_thousand_row_xls_opens_with_bounded_dimensions_and_cache_reuse`、`maximum_supported_map_opens_by_chunk_without_expanding_the_whole_grid`。
- 包生命周期：`all_33_domain_packs_support_disable_upgrade_and_rollback`、`corrupt_or_disabled_pack_is_isolated_and_reported_as_a_missing_dependency`、`semantically_tampered_candidate_fixture_never_changes_current`。
- 能力治理：项目/个人/团队覆盖解析、跨系统工作流编译重放、Receipt/Memory 原子记录、版本迁移预演与失败快照恢复测试。
- MCP：所有可写官方操作真实编译为 scoped Draft；参数篡改、scope 提权、revision 伪造和未知步骤全部 fail closed。

## Harness 与 AI 证据

- 系统会话使用项目 `cwd`，执行 `create → archive → open → subscribe → prompt`，不创建普通 Workspace。
- 普通 Session canary 使用 Harness 公共 Runtime 执行 create/open/archive，并断言不是 managed Session。
- Core 候选 canary 在一次性项目和数据库中启动真实 `mir3-mcp` sidecar，调用系统列表与官方领域能力；任一步失败都不推进 LKG，并触发 Core 回滚、重启和 iframe 刷新。
- 系统转全局只传结构化任务总结、资源引用、Draft、权限范围和未完成计划，不复制原始聊天记录。
- 系统 AI 与全局 Harness 共享同一 MCP、Capability Registry 和外置 Draft；真实项目应用仍只由 Studio 在确认后执行。

## 三个真实项目副本

自动化只对一次性副本写入，原项目不参与写测试。当前三份显著不同结构的副本规模为：

| 副本 | 文件数 | 自动识别领域数 | 用途 |
|---|---:|---:|---|
| A | 673 | 16 | 中等目录、客户端与引擎混合结构 |
| B | 13 | 6 | 极小结构、缺失依赖与未知文件诊断 |
| C | 4510 | 20 | 大目录、共享文件与复杂依赖 |

使用 `MIR3_DOMAIN_CORPUS_ROOTS` 显式传入三个副本后：

- `external_real_project_corpus_runs_the_full_readonly_domain_matrix` 验证每份副本中所有实际识别领域的文件、资源、依赖与校验链。
- `external_real_project_corpus_applies_and_restores_verified_drafts` 对每份副本执行 Draft、Diff、领域校验、应用和 Snapshot 字节级恢复。
- 33 个领域的逐包结构化写入由 bundled fixture 与 155 个 MCP 编译用例覆盖；真实副本没有覆盖到的领域不得据此宣称已完成真实游戏格式专项验收。

## 最终自动化门禁

```text
pnpm typecheck
pnpm test -- --run
pnpm lint
pnpm knip
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace -- --test-threads=1
pnpm version:check
pnpm brand:audit
pnpm plugin:audit
pnpm domain:audit
pnpm release:check
git diff --check
pnpm package:mac
pnpm smoke:mac
```

`pnpm smoke:mac` 使用隔离的 `MIR3_STUDIO_HOME` 启动打包后的 `.app`，验证真实 Harness HTTP、启动进程归属、33 个 current/LKG 包状态，并只清理该次 smoke 拥有的进程和临时目录。

## 必须保留的外部验收项

以下项目不能由当前 macOS 自动化替代，完成前不得宣称物理验收全部通过：

- 在真实 Harness UI 中确认归档系统会话重启后不出现在 Workspace、Ungrouped 和普通搜索。
- 普通 Harness 的设置、Profile、Workspace、Session、文件、编辑器、终端、Agent、MCP Client 与插件生命周期回归。
- 系统 AI 的真实提问、审批、取消、恢复、系统转全局和深链返回。
- 地图、任务、活动、沙巴克、跨服在真实游戏运行环境中的专项验收。
- Windows debug/release 的端口、数据隔离、安装、启动和基础流程。
- 最新 macOS 原生包的可视 UI 与物理设备操作验收。
