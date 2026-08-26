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
- 33/33 使用非通配引擎范围和证据门控自动泛化；版本未知、别名非法或范围不兼容时资源只读，Draft 写入与 Apply 双重拒绝。
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
- 远程分发：编译期 HTTPS/Ed25519 配置成对校验；后台只验签并暂存候选，激活继续要求用户确认。
- 能力治理：项目/个人/团队覆盖解析、跨系统工作流编译重放、Receipt/Memory 原子记录、版本迁移预演与失败快照恢复测试。
- MCP：所有可写官方操作真实编译为 scoped Draft；参数篡改、scope 提权、revision 伪造和未知步骤全部 fail closed。

## Harness 与 AI 证据

- 系统会话使用项目 `cwd`，执行 `create → archive → open → subscribe → prompt`，不创建普通 Workspace。
- 普通 Session canary 使用 Harness 公共 Runtime 执行 create/open/archive，并断言不是 managed Session。
- Core 候选 canary 在一次性项目和数据库中启动真实 `mir3-mcp` sidecar，调用系统列表与官方领域能力；任一步失败都不推进 LKG，并触发 Core 回滚、重启和 iframe 刷新。
- 系统转全局只传结构化任务总结、资源引用、Draft、权限范围和未完成计划，不复制原始聊天记录。
- 系统 AI 与全局 Harness 共享同一 MCP、Capability Registry 和外置 Draft；真实项目应用仍只由 Studio 在确认后执行。

2026-08-26 使用 `f4fc4d2` 原生 smoke 保留的一次性 profile，独立启动 Harness
`0.1.1-rc.2` 并进行浏览器级只读回归：主工作台、会话搜索、新建会话、工作区选择、
通用设置、模型、插件、Agent 预设页面均可渲染；插件清单显示 140 个运行项，
`mir3-core` 为“已挂载、已启用”。隔离 profile 内的 Core Plugin client SHA-256 与
仓库文件一致：
`d4d8281384d5ba45810835719595888f1da8efcefbca3826f66ecd672641e644`。
该记录证明当前 Harness Web 表面和兼容插件可加载，但没有工作区/API Key，且不是
最新原生窗口截图，因此不作为文件、编辑器、终端、Agent、MCP Client、隐藏会话或
物理设备回归的完成证据。

## G0 历史基线

G0 的可定位历史基线是 commit
`8ecb8c516126ce6a150fef9322256fa42d4265c4`。仓库只能证明该 Git 对象当前可读，
不声称任何仓库外 G0 备份仍然存在。可用以下命令恢复一份无 `.git`
元数据的只读快照，并直接比较历史基线与当前 `HEAD`：

```bash
MIR3_G0_COMMIT=8ecb8c516126ce6a150fef9322256fa42d4265c4
git cat-file -e "${MIR3_G0_COMMIT}^{commit}"
MIR3_G0_SNAPSHOT="$(mktemp -d)/mir3-g0"
mkdir -p "$MIR3_G0_SNAPSHOT"
git archive "$MIR3_G0_COMMIT" | tar -x -C "$MIR3_G0_SNAPSHOT"
chmod -R a-w "$MIR3_G0_SNAPSHOT"
git diff --stat "$MIR3_G0_COMMIT"..HEAD --
git diff --name-status "$MIR3_G0_COMMIT"..HEAD --
```

只有用户另行提供了外置 G0 路径时，才能把它与上述只读快照比较；
`git diff --no-index` 在发现差异时返回码为 1：

```bash
git diff --no-index --stat "$MIR3_G0_SNAPSHOT" /absolute/path/to/user-provided-g0-copy
```

## 三个真实项目副本

自动化只对一次性副本写入，原项目不参与写测试。仓库不保存真实项目副本，当前也
没有可复核的三项目验收报告；历史临时目录的规模不能作为最终证据。最终验收必须
由用户明确提供三个互不嵌套、可丢弃且结构显著不同的绝对路径，并由运行器生成带
内容哈希的仓库外报告。建议至少覆盖：

| 副本 | 目标结构 | 验收重点 |
|---|---|---|
| A | 中等目录、客户端与引擎混合结构 | 常规识别、共享文件和跨领域引用 |
| B | 极小或旧版本结构 | 缺失依赖、版本别名和未知文件只读诊断 |
| C | 大目录或显著不同版本 | 大规模索引、复杂依赖、批量修改和恢复 |

不能仅运行普通 `cargo test` 作为真实语料证据：两个 external corpus 测试使用
Rust harness 的 `#[ignore]`，普通测试输出必须把它们记为 `ignored`，而不是误导性的
`ok`。最终验收必须由专用运行器设置 `MIR3_DOMAIN_CORPUS_ROOTS` 并显式执行
ignored 测试，对三份明确的一次性副本运行：

```bash
node scripts/run-domain-corpus-acceptance.mjs \
  --confirm-disposable-copies \
  /absolute/copy-a /absolute/copy-b /absolute/copy-c
```

运行器拒绝重复、嵌套或缺少“客户端/引擎”的根目录，先记录每个文件的内容
SHA-256、总字节数、整树 SHA-256 和引擎/客户端版本标记，再执行静态覆盖审计及
`external_real_project_corpus` 两条矩阵测试。报告默认写入仓库外的系统临时目录：
`mir3-domain-corpus-acceptance/<corpus-identity>/report.json`。若显式指定 `--output`，
仓库内路径必须已被 Git ignore，且不能位于任何项目副本内。

报告状态为 `passed` 后，以下两项才具有可重复验收证据：

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

`pnpm package:mac` 只接受无 tracked 修改的已提交源码；不跟踪的用户文件不会参与
构建身份，也不会被删除或提交。构建时会把 Git commit/tree、产品/Core Plugin/33 包
版本和构建时间写入 App 内的 `resources/build-provenance.json`，签名和 DMG 校验后再生成：

```text
src-tauri/target/release/bundle/dmg/<DMG filename>.provenance.json
```

该 sidecar 记录签名模式、notarization/staple 状态、签名后 App 整树 SHA-256、DMG
SHA-256、文件数、字节数和验证时间。`node scripts/package-macos.mjs --verify-only`
会重新计算上述证据，并拒绝当前提交、内嵌身份或产物哈希不一致。

原生 smoke 还必须等到 `.mir3-core-canary.json` 落盘；该状态只会在 Bridge v2、普通
Session、归档系统 Session、真实 MCP sidecar 和官方领域能力 canary 全部通过后写入。
这仍不证明归档会话在 Workspace/Ungrouped/搜索中的可见性，后者保留为真实 UI 验收。

macOS 控制台未登录、锁屏或会话状态无法可靠读取时，原生 smoke 必须在创建临时目录和
启动应用前 fail-closed，并提示解锁后重新运行。全部 native canary 通过后，smoke 在 DMG
provenance 同目录原子覆盖写入 `<DMG filename>.smoke.json`。该证据绑定产品版本、buildId、源码
commit/tree、DMG 与 provenance SHA-256、Core tag/commit、Bridge protocol、必需 runtime
gates、33 包计数和通过时间；不记录临时数据目录、端口或用户名。证据写入或任一绑定校验
失败时，smoke 仍然失败。同版本重新打包会先删除旧 smoke sidecar，防止新 DMG 尚未
通过 canary 时残留旧 passed 结果。此 sidecar 证明本次自动化 native canary，不替代下列真实 UI 验收。

## 必须保留的外部验收项

以下项目不能由当前 macOS 自动化替代，完成前不得宣称物理验收全部通过：

- 在真实 Harness UI 中确认归档系统会话重启后不出现在 Workspace、Ungrouped 和普通搜索。
- 普通 Harness 的设置、Profile、Workspace、Session、文件、编辑器、终端、Agent、MCP Client 与插件生命周期回归。
- 系统 AI 的真实提问、审批、取消、恢复、系统转全局和深链返回。
- 地图、任务、活动、沙巴克、跨服在真实游戏运行环境中的专项验收。
- Windows debug/release 的端口、数据隔离、安装、启动和基础流程。
- 最新 macOS 原生包的可视 UI 与物理设备操作验收。

当前证据状态必须按事实更新，不能用单元测试、HTTP 健康检查或历史临时目录代替：

| 验收项 | 当前状态 | 完成所需证据 |
|---|---|---|
| 三个真实项目副本 | 待提供 | 三个明确绝对路径及 `run-domain-corpus-acceptance.mjs` 的 passed 报告 |
| macOS Core/WebView canary | 锁屏阻塞 | 解锁 console 会话后 `pnpm smoke:mac` 通过 |
| macOS 可视 UI 与隐藏会话 | 待执行 | 正控制普通会话可见、负断言归档系统会话在三个入口均不可见 |
| Windows 基础流程 | 待执行 | Windows debug/release 物理机日志与验收记录 |
| 真实游戏专项 | 待执行 | 地图、任务、活动、沙巴克、跨服加载及运行结果 |
| 领域包在线更新 | 待配置 | 生产 HTTPS 索引、公钥、签名候选及失败回滚证据 |
