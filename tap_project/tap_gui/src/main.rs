//! TAP graphical client: one window, one connection to the server.
//!
//! - `network`: connection thread, lines from the server to the window
//! - `protocol`: reading the server replies (RFC 42TAP)
//! - `app`: window state, commands sent and replies received
//! - `view`: drawing the window

mod app;
mod network;
mod protocol;
mod view;

use std::env;
use std::process::ExitCode;

use app::TapApp;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 7878;

/// Reads `[host] [port]` from the command line, with defaults.
fn parse_args() -> Result<(String, u16), String> {
    let args: Vec<String> = env::args().collect();

    if args.len() > 3 {
        return Err("too many arguments".to_string());
    }

    let host = match args.get(1) {
        Some(h) => h.clone(),
        None => DEFAULT_HOST.to_string(),
    };

    let port = match args.get(2) {
        Some(p) => match p.parse::<u16>() {
            Ok(n) => n,
            Err(_) => return Err(format!("invalid port: {p}")),
        },
        None => DEFAULT_PORT,
    };

    Ok((host, port))
}

fn main() -> ExitCode {
    let (host, port) = match parse_args() {
        Ok(hp) => hp,
        Err(e) => {
            eprintln!("Error: {e}");
            eprintln!("Usage: tap_gui [host] [port]");
            return ExitCode::FAILURE;
        }
    };

    let options = eframe::NativeOptions::default();
    let result = eframe::run_native(
        "TAP",
        options,
        Box::new(move |cc| Ok(Box::new(TapApp::new(&cc.egui_ctx, &host, port)))),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("GUI error: {e}");
            ExitCode::FAILURE
        }
    }
}
