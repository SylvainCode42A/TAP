//! TAP CLI client: lines typed on stdin go to the server as is, and every
//! server line is printed as it arrives.
//!
//! Commands are raw protocol (`MOVE north`, `CHAT ROOM hi`), with no
//! translation layer, so the client works with any RFC 42TAP server. A
//! reader thread prints server lines, so events show up while the user
//! is typing.

use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream};
use std::process::{self, ExitCode};
use std::thread;

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

/// Starts a thread that prints every line received from the server.
/// When the connection closes, the whole program exits.
fn spawn_reader(stream: TcpStream) {
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            match line {
                Ok(text) => println!("{text}"),
                Err(e) => {
                    eprintln!("Read error: {e}");
                    break;
                }
            }
        }
        println!("Connection closed");
        process::exit(0);
    });
}

/// Sends every non-empty line typed on the keyboard to the server, ending with `\n`.
/// Returns when the keyboard is closed (Ctrl-D) or when sending fails.
fn send_loop(stream: &mut TcpStream) {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let text = match line {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Keyboard read error: {e}");
                break;
            }
        };
        if text.is_empty() {
            continue;
        }
        if let Err(e) = stream.write_all(format!("{text}\n").as_bytes()) {
            eprintln!("Send error: {e}");
            break;
        }
    }
}

fn main() -> ExitCode {
    let (host, port) = match parse_args() {
        Ok(hp) => hp,
        Err(e) => {
            eprintln!("Error: {e}");
            eprintln!("Usage: tap_client [host] [port]");
            return ExitCode::FAILURE;
        }
    };

    let mut stream = match TcpStream::connect((host.as_str(), port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Connection to {host}:{port} failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!("Connected to {host}:{port}");

    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Cannot clone connection: {e}");
            return ExitCode::FAILURE;
        }
    };

    spawn_reader(reader_stream);
    send_loop(&mut stream);

    // The keyboard is closed: close the connection too.
    // An error here only means it was already closed, so it is ignored.
    let _ = stream.shutdown(Shutdown::Both);
    ExitCode::SUCCESS
}
