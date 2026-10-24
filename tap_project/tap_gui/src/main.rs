//! TAP graphical client: one window, one connection to the server.
//!
//! - `network`: connection thread, lines from the server to the window
//! - `protocol`: reading the server replies (RFC 42TAP)
//! - `events`: reading the `EVT` lines (RFC 6.2)
//! - `app`: window state, commands sent and replies received
//! - `replies`: what each server reply changes in the window
//! - `live`: what each `EVT` line changes in the window
//! - `images`: pictures of the rooms, built into the program
//! - `view`: drawing helpers (room, player, counters, buttons)
//! - `window`: the window layout, one panel per zone
//! - `log_panel`: the log, coloured by kind of line
//! - `chat`: the chat panel, apart from the log
//! - `map`: the mini-map, built from the exits of each LOOK
//! - `fade`: the fade-in of the room panel when the room changes

mod app;
mod chat;
mod events;
mod fade;
mod images;
mod live;
mod log_panel;
mod map;
mod network;
mod protocol;
mod replies;
mod view;
mod window;

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

    // Big enough for the room in the middle and a panel on each side.
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1200.0, 750.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "TAP",
        options,
        Box::new(move |cc| {
            // Lets egui decode the jpeg pictures of the rooms.
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(TapApp::new(&cc.egui_ctx, &host, port)))
        }),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("GUI error: {e}");
            ExitCode::FAILURE
        }
    }
}
