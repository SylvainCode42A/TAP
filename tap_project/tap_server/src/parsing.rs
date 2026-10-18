use std::net::TcpStream;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use crate::{Command, Response, GameState};
use crate::dispatch;
use crate::send_error_mess;

pub fn handle_(stream: &mut TcpStream, game_state: Arc<Mutex<GameState>>) {
    let mut current_username: Option<String> = None;

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

                if actions.get(0) == Some(&"CONNECT") {
                    matched = true;
                    if actions.len() == 2 {
                        current_username = Some(actions[1].to_string());
                        cmd = Some(Command::Connect { username: actions[1].to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CONNECT");
                    }
                }

                if line == "LOOK\n" {
                    matched = true;
                    cmd = Some(Command::Look);
                }

                if actions.get(0) == Some(&"MOVE") {
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
                            send_error_mess(stream, "406", "Invalid direction for MOVE");
                        }
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for MOVE");
                    }
                }

                if actions.get(0) == Some(&"TAKE") {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Take { item: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TAKE");
                    }
                }

                if actions.get(0) == Some(&"DROP") {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Drop { item: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for DROP");
                    }
                }

                if line == "INVENTORY\n" {
                    matched = true;
                    cmd = Some(Command::Inventory);
                }

                if actions.get(0) == Some(&"TALK") {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Talk { npc: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TALK");
                    }
                }

                if actions.get(0) == Some(&"ATTACK") {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Attack { target: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for ATTACK");
                    }
                }

                if line == "STATUS\n" {
                    matched = true;
                    cmd = Some(Command::Status);
                }

                if actions.get(0) == Some(&"QUEST") {
                    matched = true;
                    if actions.len() >= 2 {
                        let item = actions[1..].join(" ");
                        cmd = Some(Command::Quest { quest_id: item.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for QUEST");
                    }
                }

                if line == "QUESTS\n" {
                    matched = true;
                    cmd = Some(Command::Quests);
                }

                if line == "WHO\n" {
                    matched = true;
                    cmd = Some(Command::Who);
                }

                if actions.get(0) == Some(&"GROUP") && actions.get(1) == Some(&"INVITE") {
                    matched = true;
                    if actions.len() == 3 {
                        cmd = Some(Command::GroupInvite { username: actions[2].to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for GROUP INVITE");
                    }
                }

                if actions.get(0) == Some(&"GROUP") && actions.get(1) == Some(&"JOIN") {
                    matched = true;
                    if actions.len() == 3 {
                        cmd = Some(Command::GroupJoin { group_id: actions[2].to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for GROUP JOIN");
                    }
                }

                if line == "GROUP LEAVE\n" {
                    matched = true;
                    cmd = Some(Command::GroupLeave);
                }

                if actions.get(0) == Some(&"CHAT") && actions.get(1) == Some(&"GLOBAL") {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatGlobal { message: rest.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT GLOBAL");
                    }
                }

                if actions.get(0) == Some(&"CHAT") && actions.get(1) == Some(&"ROOM") {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatRoom { message: rest.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT ROOM");
                    }
                }

                if actions.get(0) == Some(&"CHAT") && actions.get(1) == Some(&"GROUP") {
                    matched = true;
                    if actions.len() > 2 {
                        let rest = actions[2..].join(" ");
                        cmd = Some(Command::ChatGroup { message: rest.to_string() });
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT GROUP");
                    }
                }

                if line == "QUIT\n" {
                    matched = true;
                    cmd = Some(Command::Quit);
                    should_quit = true;
                }

                if !matched {
                    send_error_mess(stream, "401", "Unknown command");
                }

                if let Some(c) = cmd {
                    let resp = dispatch::dispatch(c, &game_state, &current_username);
                    match resp {
                        Response::Ok(msg) => {
                            let message = format!("OK: {}", msg);
                            println!("{}", message);
                            stream.write_all(message.as_bytes()).expect("Failed to send response");
                        }
                        Response::Err { code, message } => {
                            send_error_mess(stream, &code.to_string(), &message);
                        }
                    }
                }

                if should_quit {
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