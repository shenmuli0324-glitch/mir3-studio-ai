//! MIR3 Studio 的 996 项目领域层。
//!
//! 本 crate 不依赖 Tauri 或 Harness；桌面命令和 MCP sidecar 共享同一实现，
//! 避免项目索引、知识与 Draft 出现两套状态。

mod draft;
mod governance;
mod knowledge;
mod map_adapter;
mod project;
mod resources;
mod safe_files;
mod scan;
mod store;
mod systems;

pub use draft::*;
pub use governance::*;
pub use knowledge::*;
pub use map_adapter::*;
pub use project::*;
pub use resources::*;
pub use safe_files::*;
pub use scan::*;
pub use store::*;
pub use systems::*;
