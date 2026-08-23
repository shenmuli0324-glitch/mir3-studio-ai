//! 命令行集成：安装后只在用户 PATH 中注册 `mir3` 命令。
//!
//! MIR3 AI Core 与捆绑的 pnpm 都是 Node 脚本，因此由包装脚本启动：
//!
//! - Windows：`%LOCALAPPDATA%\mir3-studio-ai\bin\mir3.cmd` / `mir3.ps1`，
//!   通过 `HKCU\Environment\Path` 注册并广播
//!   `WM_SETTINGCHANGE`；
//! - macOS/Linux：`~/.local/bin/mir3`，必要时向
//!   `~/.zshrc` / `~/.bashrc` 幂等更新 PATH 导出块（只动自身标记块、保留
//!   用户其余配置；写入前备份临时文件 + rename，失败自动回滚）。
//!
//! shim 运行时优先使用本地版本兼容的 node（校验规则与
//! [`crate::config::is_supported_node_version`] 一致），否则回退到捆绑运行时；
//! 插件安装使用的 pnpm shim 放在应用私有目录，仅对子进程 PATH 可见。
//! shim 不重定向 stdin/stdout、不修改工作目录，保证交互式命令可用，
//! 并透传全部参数与退出码。
//!
//! 模块划分（参考 `service/download/`）：
//! - [`shim`]：shim 脚本内容生成与落盘
//! - [`path`]：bin 目录定位、PATH 注册（注册表 / shell rc）、用户 pnpm 探测
//! - [`core`]：对外接口（状态 / 启用 / 清理）

mod core;
mod path;
mod shim;

pub use core::{ensure, ensure_shims, get_status, remove, CliLinkStatus};
pub use path::{find_user_pnpm, get_bin_dir, get_internal_bin_dir};
