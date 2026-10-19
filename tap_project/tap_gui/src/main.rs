use std::env;
use std::io::{BufRead, BufReader, Write};
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
    /// Write side of the connection, None if the connection failed.
    writer: Option<TcpStream>,
    /// Every line shown in the log, oldest first.
    log: Vec<String>,
    /// Player name typed in the name field.
    name: String,
}

impl TapApp {
    /// Connects to the server and starts the network thread.
    fn new(ctx: &egui::Context, host: &str, port: u16) -> TapApp {
        let mut app = TapApp {
            receiver: None,
            writer: None,
            log: vec![],
            name: String::new(),
        };
        let stream = match TcpStream::connect((host, port)) {
            Ok(s) => s,
            Err(e) => {
                app.log
                    .push(format!("Connection to {host}:{port} failed: {e}"));
                return app;
            }
        };
        match stream.try_clone() {
            Ok(reader_stream) => {
                app.receiver = Some(spawn_reader(reader_stream, ctx.clone()));
                app.writer = Some(stream);
                app.log.push(format!("Connected to {host}:{port}"));
            }
            Err(e) => app.log.push(format!("Cannot clone connection: {e}")),
        }
        app
    }

    /// Sends one command to the server, ending with `\n`, and shows it in the log.
    fn send(&mut self, command: &str) {
        let Some(writer) = &mut self.writer else {
            self.log.push("Not connected".to_string());
            return;
        };
        match writer.write_all(format!("{command}\n").as_bytes()) {
            Ok(()) => self.log.push(format!("> {command}")),
            Err(e) => self.log.push(format!("Send error: {e}")),
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

        // Buttons only record the click here: commands are sent after drawing,
        // because the drawing code already borrows `self.name` and `self.log`.
        let mut connect_clicked = false;

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("TAP");
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.name);
                connect_clicked = ui.button("Connect").clicked();
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log {
                        ui.label(line);
                    }
                });
        });

        if connect_clicked {
            let name = self.name.trim().to_string();
            self.send(&format!("CONNECT {name}"));
            self.send("LOOK");
        }
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
