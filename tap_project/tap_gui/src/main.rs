use std::env;
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui;

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

/// Starts a thread that forwards every line received from the server to the window.
/// `ctx` is used to wake the window up as soon as a line arrives.
fn spawn_reader(stream: TcpStream, ctx: egui::Context) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let text = match line {
                Ok(t) => t,
                Err(e) => {
                    let _ = sender.send(format!("Read error: {e}"));
                    break;
                }
            };
            // An error here means the window is closed: nobody is listening anymore.
            if sender.send(text).is_err() {
                return;
            }
            ctx.request_repaint();
        }
        let _ = sender.send("Connection closed".to_string());
        ctx.request_repaint();
    });
    receiver
}

/// State of the GUI window.
struct TapApp {
    /// Lines coming from the network thread, None if the connection failed.
    receiver: Option<Receiver<String>>,
    /// Every line shown in the log, oldest first.
    log: Vec<String>,
}

impl TapApp {
    /// Connects to the server and starts the network thread.
    fn new(ctx: &egui::Context, host: &str, port: u16) -> TapApp {
        match TcpStream::connect((host, port)) {
            Ok(stream) => TapApp {
                receiver: Some(spawn_reader(stream, ctx.clone())),
                log: vec![format!("Connected to {host}:{port}")],
            },
            Err(e) => TapApp {
                receiver: None,
                log: vec![format!("Connection to {host}:{port} failed: {e}")],
            },
        }
    }
}

impl eframe::App for TapApp {
    /// Called on every frame: draws the whole window from the current state.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Move every line received since the last frame into the log.
        if let Some(receiver) = &self.receiver {
            while let Ok(line) = receiver.try_recv() {
                self.log.push(line);
            }
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("TAP");
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log {
                        ui.label(line);
                    }
                });
        });
    }
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
