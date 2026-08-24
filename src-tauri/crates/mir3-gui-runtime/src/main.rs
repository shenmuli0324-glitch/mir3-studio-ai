#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{self, BufRead, Write};

use mir3_gui_runtime::{execute_json_line, RuntimeServer};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut server = RuntimeServer::new();
    for line in stdin.lock().lines() {
        let response = match line {
            Ok(line) => execute_json_line(&mut server, &line),
            Err(error) => {
                eprintln!("RUNTIME_STDIN_READ: {error}");
                break;
            }
        };
        match serde_json::to_string(&response) {
            Ok(line) => {
                if writeln!(stdout, "{line}").is_err() || stdout.flush().is_err() {
                    break;
                }
            }
            Err(error) => {
                eprintln!("RUNTIME_RESPONSE_SERIALIZE: {error}");
                break;
            }
        }
    }
}
