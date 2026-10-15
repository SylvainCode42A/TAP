use std::net::{TcpListener, TcpStream};
use std::io::{BufRead, BufReader, Write};


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
                if line.starts_with("CONNECT") {
                    let actions: Vec<&str> = line.split_whitespace().collect();
                    
                    if actions.len() == 2 {
                        println!("OK '{}' connected", actions[1]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CONNECT");
                    }
                }
                if line.trim() == "LOOK" {
                    println!("OK json() all info of room, players, next room");
                }
                if line.starts_with("MOOV") {
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 2 {
                        println!("OK room = next_is_{}", actions[1]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for MOOV");
                    }
                }
                if line.starts_with("TAKE") {
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 2 {
                        println!("OK taken={}", actions[1]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TAKE");
                    }
                }
                if line.starts_with("DROP") {
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 2 {
                        println!("OK dropped={}", actions[1]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for DROP");
                    }
                }
                if line.trim() == "INVENTORY" {
                    println!("OK json() of player INVENTORY");
                }
                if line.starts_with("TALK") {
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 2 {
                        println!("OK json() of TALK to '{}'", actions[1]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for TALK");
                    }
                }
                if line.starts_with("ATTACK") {
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 2 {
                        println!("OK json() of ATTACK to player '{}'", actions[1]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for ATTACK");
                    }
                }
                if line.trim() == "STATUS" {
                    println!("OK json() of health player")
                }
                if line.starts_with("QUEST") {
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 2 {
                        println!("OK json() of quest '{}'", actions[1]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for QUEST");
                    }
                }
                if line.trim() == "QUESTS" {
                    println!("OK [all_quests]");
                }
                if line.trim() == "WHO" {
                    println!("OK json() of all players and count");
                }
                if line.starts_with("GROUP INVITE") {
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 3 {
                        println!("OK group_invite=pending for player '{}'", actions[2]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for GROUP INVITE");
                    }
                }
                if line.starts_with("GROUP JOIN") {
                    let actions : Vec<&str> = line.split_whitespace().collect();
                    if actions.len() == 3 {
                        println!("OK joined={}", actions[2]);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for GROUP JOIN");
                    }
                }
                if line.trim() == "GROUP LEAVE" {
                    println!("OK left_group");
                }
                if line.starts_with("CHAT GLOBAL") {
                    let actions: Vec<&str> = line.split_whitespace().collect();
                    if actions.len() > 2 {
                        let rest = actions.into_iter().skip(2).collect::<Vec<&str>>().join(" ");
                        println!("OK say '{}' in GLOBAL", rest);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT GLOBAL");
                    }
                }
                if line.starts_with("CHAT ROOM") {
                    let actions: Vec<&str> = line.split_whitespace().collect();
                    if actions.len() > 2 {
                        let rest = actions.into_iter().skip(2).collect::<Vec<&str>>().join(" ");
                        println!("OK say '{}' in ROOM", rest);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT ROOM");
                    }
                }
                if line.starts_with("CHAT GROUP") {
                    let actions: Vec<&str> = line.split_whitespace().collect();
                    if actions.len() > 2 {
                        let rest = actions.into_iter().skip(2).collect::<Vec<&str>>().join(" ");
                        println!("OK say '{}' in GROUP", rest);
                    }
                    else {
                        send_error_mess(stream, "401", "Invalid number of arguments for CHAT GROUP");
                    }
                }
                if line.trim() == "QUIT" {
                    println!("OK goodbye");
                    break;
                }
            }
            Err(e) => {
                println!("ERR: {}", e);
                break;
            }
        }
    }
}

fn send_error_mess(stream: &mut TcpStream, t_error: &str, error: &str) {
    let message = format!("ERR {} {}\n", t_error, error);
    stream.write_all(message.as_bytes()).expect("Failed to send message error");
}
