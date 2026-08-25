//! bridge：Tauri 命令的对外出口。
//!
//! 按领域把命令拆成若干子模块（生命周期 / 插件 / 核心 / 档案 / 应用配置 /
//! 系统集成 / 更新），此处统一声明并重导出为「干净 API」，供 `generate_handler!`
//! 以 `crate::bridge::<cmd>` 形式注册。

pub mod app_config;
pub mod clipboard;
pub mod core;
pub mod gui_designer;
pub mod lifecycle;
pub mod plugin;
pub mod profile;
pub mod project;
pub mod system_os;
pub mod updater;

pub use app_config::*;
pub use clipboard::*;
pub use core::*;
pub use gui_designer::*;
pub use lifecycle::*;
pub use plugin::*;
pub use profile::*;
pub use project::*;
pub use system_os::*;
pub use updater::*;
