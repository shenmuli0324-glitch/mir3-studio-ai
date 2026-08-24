//! 996 GUI 隔离运行时协议与可嵌入执行入口。

mod engine;
mod mocks;
mod model;
mod runtime;

pub use model::*;
pub use runtime::{execute_json_line, execute_request, RuntimeServer};

#[cfg(test)]
mod tests;
