use std::net::TcpStream;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use crate::{Command, Response, GameState};
use crate::dispatch;
use crate::send_error_mess;

pub fn handle_(stream: &mut TcpStream, game_state: Arc<Mutex<GameState>>) {
    let mut current_username: Option<String> = None;
    stream.write_all("OK hello proto=1\n".as_bytes()).expect("Failed to send hello");

    loop {
        let mut reader = BufReader::new(&mut *stream);
        let mut line = String::new();

        match reader.read_line(&mut line) {

            Ok(0) => break,
            Ok(_) => {
                let mut matched = false;
                let mut should_quit = false;
                let mut cmd: Option<Command> = None;
                let actions: Vec<&str> = line.split_whitespace().collect();

                if actions.get(0).map(|s| s.to_uppercase()) == Some("CONNECT".to_string()) {
                    matched = true;
                    if current_username.is_some() {
                        let current = current_username.as_ref().unwrap();
                        send_error_mess(stream, "401", &format!("{} already connected, use QUIT first", current));
                    } else if actions.len() == 2 {
                        cmd = Some(Command::Connect { username: actions[1].to_string() });
                    } else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CONNECT");
                    }
                }

                if line.trim().to_uppercase() == "LOOK" {
                    matched = true;
                    cmd = Some(Command::Look);
                }

                if actions.get(0).map(|s| s.to_uppercase()) == Some("MOVE".to_string()) {
                    matched = true;
                    if actions.len() == 2 {
                        let upper = actions[1].trim().to_uppercase();
                        if upper == "NORTH" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                        }
                        else if upper == "SOUTH" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                        }
                        else if upper == "WEST" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                        }
                        else if upper == "EAST" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                        }
                        else {
                            send_error_mess(stream, "301", "NO_EXIT");
                        }
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for MOVE");
                    }
                }

                if actions.get(0).map(|s| s.to_uppercase()) == Some("TAKE".to_string()) {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Take { item: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TAKE");
                    }
                }

                if actions.get(0).map(|s| s.to_uppercase()) == Some("DROP".to_string()) {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Drop { item: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for DROP");
                    }
                }

                if line.trim().to_uppercase() == "INVENTORY" {
                    matched = true;
                    cmd = Some(Command::Inventory);
                }

                if actions.get(0).map(|s| s.to_uppercase()) == Some("TALK".to_string()) {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Talk { npc: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TALK");
                    }
                }

                if actions.get(0).map(|s| s.to_uppercase()) == Some("ATTACK".to_string()) {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Attack { target: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for ATTACK");
                    }
                }

                if line.trim().to_uppercase() == "STATUS" {
                    matched = true;
                    cmd = Some(Command::Status);
                }

                if actions.get(0).map(|s| s.to_uppercase()) == Some("QUEST".to_string()) {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Quest { quest_id: item.to_string() });
                    }
                    else {
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

                if actions.get(0).map(|s| s.to_uppercase()) == Some("GROUP".to_string()) && actions.get(1).map(|s| s.to_uppercase()) == Some("INVITE".to_string()) {
                    matched = true;
                    if actions.len() == 3 {
                        cmd = Some(Command::GroupInvite { username: actions[2].to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for GROUP INVITE");
                    }
                }

                if actions.get(0).map(|s| s.to_uppercase()) == Some("GROUP".to_string()) && actions.get(1).map(|s| s.to_uppercase()) == Some("JOIN".to_string()) {
                    matched = true;
                    if actions.len() == 3 {
                        cmd = Some(Command::GroupJoin { group_id: actions[2].to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for GROUP JOIN");
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

                if actions.get(0).map(|s| s.to_uppercase()) == Some("CHAT".to_string()) && actions.get(1).map(|s| s.to_uppercase()) == Some("GLOBAL".to_string()) {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatGlobal { message: rest.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT GLOBAL");
                    }
                }

                if actions.get(0).map(|s| s.to_uppercase()) == Some("CHAT".to_string()) && actions.get(1).map(|s| s.to_uppercase()) == Some("ROOM".to_string()) {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatRoom { message: rest.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT ROOM");
                    }
                }

                if actions.get(0).map(|s| s.to_uppercase()) == Some("CHAT".to_string()) && actions.get(1).map(|s| s.to_uppercase()) == Some("GROUP".to_string()) {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatGroup { message: rest.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT GROUP");
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
                            }
                            let message = format!("OK {}\n", msg);
                            println!("{}", message.trim());
                            stream.write_all(message.as_bytes()).expect("Failed to send response");
                        }
                        Response::Err { code, message } => {
                            send_error_mess(stream, &code.to_string(), &message);
                        }
                    }
                }

                if should_quit {
                    if let Some(username) = &current_username {
                        let mut gs = game_state.lock().unwrap();
                        gs.players.remove(username);
                    }
                    break;
                }
            },

            Err(e) => {
                println!("ERROR: {}", e);
                break;
            },
        }
    }
}