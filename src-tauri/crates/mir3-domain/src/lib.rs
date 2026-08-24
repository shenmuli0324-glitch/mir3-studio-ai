//! MIR3 Studio 的 996 项目领域层。
//!
//! 本 crate 不依赖 Tauri 或 Harness；桌面命令和 MCP sidecar 共享同一实现，
//! 避免项目索引、知识与 Draft 出现两套状态。

mod draft;
mod knowledge;
mod project;
mod scan;
mod store;

pub use draft::*;
pub use knowledge::*;
pub use project::*;
pub use scan::*;
pub use store::*;
