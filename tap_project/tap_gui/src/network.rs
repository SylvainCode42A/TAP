//! Connection thread: forwards every line from the server to the window.

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui;

/// Line sent to the window when the server closes the connection.
pub const CONNECTION_CLOSED: &str = "Connection closed";

/// Starts a thread that forwards every line received from the server to the window.
/// `ctx` is used to wake the window up as soon as a line arrives.
pub fn spawn_reader(stream: TcpStream, ctx: egui::Context) -> Receiver<String> {
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
        let _ = sender.send(CONNECTION_CLOSED.to_string());
        ctx.request_repaint();
    });
    receiver
}
