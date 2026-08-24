//! 996 Lua GUI 的静态解析、统一 DOM 与源码级编辑原语。
//!
//! 本 crate 不执行 Lua，也不访问文件系统。调用方负责解码文件、验证路径、
//! 保存 Draft 与应用 Patch。

mod adapter;
mod model;
mod parser;
mod source_edit;

pub use adapter::*;
pub use model::*;
pub use parser::parse_document;
pub use source_edit::{
    apply_source_edits, generate_template, insert_core_node, replace_bound_property,
};
