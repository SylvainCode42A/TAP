//! Reading side of one client connection: the greeting, each line into a
//! `Command`, the reply written back, the departure.
//!
//! - Owns: RFC 4 framing (one `\n`-terminated line per command), keyword
//!   matching ignoring case, argument counts, writing `OK` and `ERR`,
//!   session logs and flood detection.
//! - Does NOT own: what a command does (`dispatch.rs`), events
//!   (`events.rs`).

use crate::send_error_mess;
use crate::{Command, GameState, Response};
use crate::{dispatch, events, logging};
use serde_json::json;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Longest command line accepted, end of line included, RFC 9.4.
const MAX_LINE: usize = 1024;

/// Serves one client, on its own thread, until QUIT or disconnection.
///
/// Sends `OK hello proto=1`, then for each line: parse it into a
/// `Command`, run it through `dispatch`, write the reply. A malformed line
/// gets its `ERR` here and never reaches dispatch. After a successful
/// CONNECT the stream joins `GameState::senders`, for events. However the
/// session ends, `events::disconnect` then removes the player.
///
/// One `BufReader` serves the whole session: rebuilt for each line, it
/// dropped the commands that arrived in the same TCP packet (issue #36).
pub fn handle_(stream: &mut TcpStream, game_state: Arc<Mutex<GameState>>) {
    let mut current_username: Option<String> = None;
    let mut reader = match stream.try_clone() {
        Ok(read_half) => BufReader::new(read_half),
        Err(_) => return,
    };
    let ip = stream
        .peer_addr()
        .map_or("unknown".to_string(), |peer| peer.to_string());
    logging::info("connection_open", json!({"ip": ip}));
    if stream.write_all("OK hello proto=1\n".as_bytes()).is_err() {
        logging::info(
            "connection_close",
            json!({"ip": ip, "player": null, "reason": "write_failed"}),
        );
        return;
    }

    let mut flood = logging::RateWatch::new(logging::FLOOD_LIMIT, logging::FLOOD_WINDOW);
    // Why the loop ended, for the log: end of file unless said otherwise.
    let mut reason = "eof";

    loop {
        let mut line = String::new();

        match reader.read_line(&mut line) {
            Ok(0) => break,
            // RFC 9.4: 1024 bytes per line at most.
            Ok(n) if n > MAX_LINE => {
                logging::warn(
                    "abuse_oversized_line",
                    json!({
                        "ip": ip, "player": current_username, "bytes": n,
                    }),
                );
                send_error_mess(stream, "401", "LINE_TOO_LONG");
            }
            Ok(_) => {
                logging::info(
                    "command",
                    json!({
                        "ip": ip, "player": current_username, "line": line.trim_end(),
                    }),
                );
                if flood.hit(Instant::now()) {
                    logging::warn(
                        "abuse_command_flood",
                        json!({
                            "ip": ip,
                            "player": current_username,
                            "limit": logging::FLOOD_LIMIT,
                            "window_ms": logging::FLOOD_WINDOW.as_millis() as u64,
                        }),
                    );
                }
                let mut matched = false;
                let mut should_quit = false;
                let mut cmd: Option<Command> = None;
                let actions: Vec<&str> = line.split_whitespace().collect();

                if actions.first().map(|s| s.to_uppercase()) == Some("CONNECT".to_string()) {
                    matched = true;
                    if let Some(current) = &current_username {
                        send_error_mess(
                            stream,
                            "401",
                            &format!("{} already connected, use QUIT first", current),
                        );
                    } else if actions.len() == 2 {
                        cmd = Some(Command::Connect {
                            username: actions[1].to_string(),
                        });
                    } else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CONNECT");
                    }
                }

                if line.trim().to_uppercase() == "LOOK" {
                    matched = true;
                    cmd = Some(Command::Look);
                }

                if actions.first().map(|s| s.to_uppercase()) == Some("MOVE".to_string()) {
                    matched = true;
                    // Any word: dispatch checks it against the exits of the
                    // room, which world.yaml may name freely (south_alt).
                    if actions.len() == 2 {
                        cmd = Some(Command::Move {
                            direction: actions[1].to_string(),
                        });
                    } else {
                        send_error_mess(stream, "401", "Invalid number of arguments for MOVE");
                    }
                }

                if actions.first().map(|s| s.to_uppercase()) == Some("TAKE".to_string()) {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Take {
                            item: item.to_string(),
                        });
                    } else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TAKE");
                    }
                }

                if actions.first().map(|s| s.to_uppercase()) == Some("DROP".to_string()) {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Drop {
                            item: item.to_string(),
                        });
                    } else {
                        send_error_mess(stream, "401", "Invalid number of arguments for DROP");
                    }
                }

                if line.trim().to_uppercase() == "INVENTORY" {
                    matched = true;
                    cmd = Some(Command::Inventory);
                }

                if actions.first().map(|s| s.to_uppercase()) == Some("TALK".to_string()) {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Talk {
                            npc: item.to_string(),
                        });
                    } else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TALK");
                    }
                }

                if actions.first().map(|s| s.to_uppercase()) == Some("ATTACK".to_string()) {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Attack {
                            target: item.to_string(),
                        });
                    } else {
                        send_error_mess(stream, "401", "Invalid number of arguments for ATTACK");
                    }
                }

                if line.trim().to_uppercase() == "STATUS" {
                    matched = true;
                    cmd = Some(Command::Status);
                }

                if actions.first().map(|s| s.to_uppercase()) == Some("QUEST".to_string()) {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Quest {
                            quest_id: item.to_string(),
                        });
                    } else {
                        send_error_mess(stream, "401", "Invalid number of arguments for QUEST");
                    }
                }

                if line.trim().to_uppercase() == "QUESTS" {
                    matched = true;
                    cmd = Some(Command::Quests);
                }

                if line.trim().to_uppercase() == "WHO" {
                    matched = true;
                    cmd = Some(Command::Who);
                }

                if actions.first().map(|s| s.to_uppercase()) == Some("GROUP".to_string())
                    && actions.get(1).map(|s| s.to_uppercase()) == Some("INVITE".to_string())
                {
                    matched = true;
                    if actions.len() == 3 {
                        cmd = Some(Command::GroupInvite {
                            username: actions[2].to_string(),
                        });
                    } else {
                        send_error_mess(
                            stream,
                            "401",
                            "Invalid number of arguments for GROUP INVITE",
                        );
                    }
                }

                if actions.first().map(|s| s.to_uppercase()) == Some("GROUP".to_string())
                    && actions.get(1).map(|s| s.to_uppercase()) == Some("JOIN".to_string())
                {
                    matched = true;
                    if actions.len() == 3 {
                        cmd = Some(Command::GroupJoin {
                            group_id: actions[2].to_string(),
                        });
                    } else {
                        send_error_mess(
                            stream,
                            "401",
                            "Invalid number of arguments for GROUP JOIN",
                        );
                    }
                }

                if line.trim().to_uppercase() == "GROUP CREATE" {
                    matched = true;
                    cmd = Some(Command::GroupCreate);
                }

                if line.trim().to_uppercase() == "GROUP LEAVE" {
                    matched = true;
                    cmd = Some(Command::GroupLeave);
                }

                if actions.first().map(|s| s.to_uppercase()) == Some("CHAT".to_string())
                    && actions.get(1).map(|s| s.to_uppercase()) == Some("GLOBAL".to_string())
                {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatGlobal {
                            message: rest.to_string(),
                        });
                    } else {
                        send_error_mess(
                            stream,
                            "401",
                            "Invalid number of arguments for CHAT GLOBAL",
                        );
                    }
                }

                if actions.first().map(|s| s.to_uppercase()) == Some("CHAT".to_string())
                    && actions.get(1).map(|s| s.to_uppercase()) == Some("ROOM".to_string())
                {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatRoom {
                            message: rest.to_string(),
                        });
                    } else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT ROOM");
                    }
                }

                if actions.first().map(|s| s.to_uppercase()) == Some("CHAT".to_string())
                    && actions.get(1).map(|s| s.to_uppercase()) == Some("GROUP".to_string())
                {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatGroup {
                            message: rest.to_string(),
                        });
                    } else {
                        send_error_mess(
                            stream,
                            "401",
                            "Invalid number of arguments for CHAT GROUP",
                        );
                    }
                }

                if line.trim().to_uppercase() == "QUIT" {
                    matched = true;
                    cmd = Some(Command::Quit);
                    should_quit = true;
                }

                if !matched {
                    send_error_mess(stream, "401", "Unknown command");
                }

                if let Some(c) = cmd {
                    let is_connect = matches!(c, Command::Connect { .. });
                    let resp = dispatch::dispatch(c, &game_state, &current_username);
                    match resp {
                        Response::Ok(msg) => {
                            if is_connect {
                                current_username = Some(actions[1].to_string());
                                if let Ok(cloned) = stream.try_clone() {
                                    let mut gs = game_state.lock().unwrap();
                                    gs.senders.insert(actions[1].to_string(), cloned);
                                }
                            }
                            // RFC 4.3: a reply with no data is a bare "OK".
                            let message = if msg.is_empty() {
                                "OK\n".to_string()
                            } else {
                                format!("OK {}\n", msg)
                            };
                            logging::info(
                                "reply",
                                json!({
                                    "ip": ip, "player": current_username, "reply": message.trim_end(),
                                }),
                            );
                            if stream.write_all(message.as_bytes()).is_err() {
                                reason = "write_failed";
                                break;
                            }
                        }
                        Response::Err { code, message } => {
                            send_error_mess(stream, &code.to_string(), &message);
                        }
                    }
                }

                if should_quit {
                    reason = "quit";
                    break;
                }
            }

            // A client that vanishes resets the connection: an ordinary
            // departure, not a server error.
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
                ) =>
            {
                reason = "reset";
                break;
            }
            Err(e) => {
                logging::error(
                    "read_error",
                    json!({
                        "ip": ip, "player": current_username, "error": e.to_string(),
                    }),
                );
                reason = "read_error";
                break;
            }
        }
    }

    // QUIT, end of file or read error: the player leaves the game alike.
    if let Some(username) = &current_username {
        events::disconnect(&mut game_state.lock().unwrap(), username);
    }
    logging::info(
        "connection_close",
        json!({"ip": ip, "player": current_username, "reason": reason}),
    );
}
