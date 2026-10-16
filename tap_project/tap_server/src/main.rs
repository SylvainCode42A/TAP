use std::net::{TcpListener, TcpStream};
use std::io::{BufRead, BufReader, Write};

enum Command {
    Connect { username: String },
    Look,
    Move { direction: String },
    Take { item: String },
    Drop { item: String },
    Inventory,
    Talk { npc: String },
    Attack { target: String },
    Status,
    Quest { quest_id: String },
    Quests,
    Who,
    GroupInvite { username: String },
    GroupJoin { group_id: String },
    GroupLeave,
    ChatGlobal { message: String },
    ChatRoom { message: String },
    ChatGroup { message: String },
    Quit,
}

enum Response {
    Ok(String),
    Err { code: u16, message: String },
}

//SREZZAQ
fn dispatch(cmd: Command) -> Response {
    match cmd {
        Command::Connect { username } => Response::Ok(format!("{} connected", username)),
        _ => Response::Err { code: 404, message: "Not implemented yet".to_string() },
    }
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:7878").expect("Erreur de bind");

    for stream in listener.incoming() {
        let mut stream = stream.expect("Erreur lors de la récupération du stream");
        handle_(&mut stream);
    }
}

fn handle_(stream: &mut TcpStream) {

    loop {
        let mut reader = BufReader::new(&mut *stream);
        let mut line = String::new();

        match reader.read_line(&mut line) {

            Ok(0) => break,
            Ok(_) => {
                let mut matched = false;
                let mut should_quit = false;
                let mut cmd: Option<Command> = None;

                if line.starts_with("CONNECT") {
                    matched = true;
                    let actions: Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 2 {
                        cmd = Some(Command::Connect { username: actions[1].to_string() });
                        //println!("OK '{}' connected", actions[1]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CONNECT");
                    }
                }

                if line.trim() == "LOOK" {
                    matched = true;
                    cmd = Some(Command::Look);
                    println!("OK json() all info of room, players, next room");
                }
                else if line.trim().starts_with("LOOK") {
                    send_error_mess(stream, "401", "Invalid number of arguments for LOOK");
                }

                if line.starts_with("MOVE") {
                    matched = true;
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 2 {
                        let upper = actions[1].trim().to_uppercase();
                        if upper == "NORTH" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                            println!("OK room = next_is_north");
                        }
                        else if upper == "SOUTH" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                            println!("OK room = next_is_south");
                        }
                        else if upper == "WEST" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                            println!("OK room = next_is_west");
                        }
                        else if upper == "EAST" {
                            cmd = Some(Command::Move { direction: actions[1].to_string() });
                            println!("OK room = next_is_east");
                        }
                        else {
                            send_error_mess(stream, "406", "Invalid direction for MOVE");
                        }
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for MOVE");
                    }
                }

                if line.starts_with("TAKE") {
                    matched = true;
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() >= 2 {
                        let item = actions.into_iter().skip(1).collect::<Vec<&str>>().join(" ");
                        cmd = Some(Command::Take { item: item.to_string() });
                        println!("OK taken={}", item);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TAKE");
                    }
                }

                if line.starts_with("DROP") {
                    matched = true;
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() >= 2 {
                        let item = actions.into_iter().skip(1).collect::<Vec<&str>>().join(" ");
                        cmd = Some(Command::Drop { item: item.to_string() });
                        println!("OK dropped={}", item);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for DROP");
                    }
                }

                if line.trim() == "INVENTORY" {
                    matched = true;
                    cmd = Some(Command::Inventory);
                    println!("OK json() of player INVENTORY");
                }
                else if line.trim().starts_with("INVENTORY") {
                    send_error_mess(stream, "401", "Invalid number of arguments for INVENTORY");
                }

                if line.starts_with("TALK") {
                    matched = true;
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() >= 2 {
                        let item = actions.into_iter().skip(1).collect::<Vec<&str>>().join(" ");
                        cmd = Some(Command::Talk { npc: item.to_string() });
                        println!("OK json() of TALK to '{}'", item);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TALK");
                    }
                }

                if line.starts_with("ATTACK") {
                    matched = true;
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() >= 2 {
                        let item = actions.into_iter().skip(1).collect::<Vec<&str>>().join(" ");
                        cmd = Some(Command::Attack { target: item.to_string() });
                        println!("OK json() of ATTACK to player '{}'", item);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for ATTACK");
                    }
                }

                if line.trim() == "STATUS" {
                    matched = true;
                    cmd = Some(Command::Status);
                    println!("OK json() of health player")
                }
                else if line.trim().starts_with("STATUS") {
                    send_error_mess(stream, "401", "Invalid number of arguments for STATUS");
                }

                if line.starts_with("QUEST ") {
                    matched = true;
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 2 {
                        cmd = Some(Command::Quest { quest_id: actions[1].to_string() });
                        println!("OK json() of quest '{}'", actions[1]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for QUEST");
                    }
                }

                if line.trim() == "QUESTS" {
                    matched = true;
                    cmd = Some(Command::Quests);
                    println!("OK [all_quests]");
                }
                else if line.trim().starts_with("QUESTS") {
                    send_error_mess(stream, "401", "Invalid number of arguments for QUESTS");
                }

                if line.trim() == "WHO" {
                    matched = true;
                    cmd = Some(Command::Who);
                    println!("OK json() of all players and count");
                }
                else if line.trim().starts_with("WHO") {
                    send_error_mess(stream, "401", "Invalid number of arguments for WHO");
                }

                if line.starts_with("GROUP INVITE") {
                    matched = true;
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 3 {
                        cmd = Some(Command::GroupInvite { username: actions[2].to_string() });
                        println!("OK group_invite=pending for player '{}'", actions[2]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for GROUP INVITE");
                    }
                }

                if line.starts_with("GROUP JOIN") {
                    matched = true;
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 3 {
                        cmd = Some(Command::GroupJoin { group_id: actions[2].to_string() });
                        println!("OK joined={}", actions[2]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for GROUP JOIN");
                    }
                }

                if line.trim() == "GROUP LEAVE" {
                    matched = true;
                    cmd = Some(Command::GroupLeave);
                    println!("OK left_group");
                }
                else if line.trim().starts_with("GROUP LEAVE") {
                    send_error_mess(stream, "401", "Invalid number of arguments for GROUP LEAVE");
                }

                if line.starts_with("CHAT GLOBAL") {
                    matched = true;
                    let actions: Vec<&str> = line.split_whitespace().collect();
                    if actions.len() > 2 {
                        let rest = actions.into_iter().skip(2).collect::<Vec<&str>>().join(" ");
                        cmd = Some(Command::ChatGlobal { message: rest.to_string() });
                        println!("OK say '{}' in GLOBAL", rest);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT GLOBAL");
                    }
                }

                if line.starts_with("CHAT ROOM") {
                    matched = true;
                    let actions: Vec<&str> = line.split_whitespace().collect();
                    if actions.len() > 2 {
                        let rest = actions.into_iter().skip(2).collect::<Vec<&str>>().join(" ");
                        cmd = Some(Command::ChatRoom { message: rest.to_string() });
                        println!("OK say '{}' in ROOM", rest);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT ROOM");
                    }
                }

                if line.starts_with("CHAT GROUP") {
                    matched = true;
                    let actions: Vec<&str> = line.split_whitespace().collect();
                    if actions.len() > 2 {
                        let rest = actions.into_iter().skip(2).collect::<Vec<&str>>().join(" ");
                        cmd = Some(Command::ChatGroup { message: rest.to_string() });
                        println!("OK say '{}' in GROUP", rest);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT GROUP");
                    }
                }

                if line.trim() == "QUIT" {
                    matched = true;
                    cmd = Some(Command::Quit);
                    should_quit = true;
                    println!("OK goodbye");
                }
                else if line.trim().starts_with("QUIT") {
                    send_error_mess(stream, "401", "Invalid number of arguments for QUIT");
                }

                if !matched {
                    send_error_mess(stream, "401", "Unknown command");
                }

                if let Some(c) = cmd {
                    let resp = dispatch(c);
                    match resp {
                        Response::Ok(msg) => {
                            let message = format!("OK: {}", msg);
                            println!("{}", msg);
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
                println!("ERRoR: {}", e);
                break;
            },
        }
    }
}

fn send_error_mess(stream: &mut TcpStream, t_error: &str, error: &str) {
    let message = format!("ERR {} {}\n", t_error, error);
    println!("{}", message.trim());
    stream.write_all(message.as_bytes()).expect("Failed to send message error");
}