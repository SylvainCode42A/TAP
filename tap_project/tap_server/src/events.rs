//! Events: the asynchronous `EVT` lines of RFC 6.2, the delivery of
//! `CHAT`, RFC 5.2.1, and the cleanup of a player who leaves the game.
//!
//! One event is ours, not the RFC's: `EVT ROOM COMBAT`, because the
//! subject asks combat results to reach the players around and the RFC
//! defines no event for it.
//!
//! Rules the RFC leaves open, chosen by the team:
//!
//! - an event never goes back to the player who caused it: that player
//!   already has the `OK` reply, and a client shows its own chat line;
//! - `EVT STATS players=<n>` goes to every connected player, on each
//!   arrival and departure;
//! - `EVT GROUP INVITE <leader>` names the leader of the inviting group,
//!   the name `GROUP JOIN` expects;
//! - a chat message holding a control character is refused, RFC 9.2: it
//!   could drive the terminal of the players who receive it;
//! - a stream that fails to write is skipped: the reading thread of that
//!   player sees the broken connection and cleans up.
//!
//! Events are written while the caller holds the `GameState` lock, so two
//! events never interleave on one stream.
//!
//! - Owns: the text of every `EVT` line, who receives it, writing it, the
//!   replies of the three `CHAT` commands, the departure of a player.
//! - Does NOT own: the state changes behind an event, made by the command
//!   modules before the event is sent.

use std::io::Write;

use crate::{group, GameState, Response};

// MARK: - Delivery

/// Writes `EVT <event>` to each player of `recipients` who has a stream.
fn send(gs: &GameState, recipients: &[String], event: &str) {
    let line = format!("EVT {event}\n");
    for name in recipients {
        if let Some(mut stream) = gs.senders.get(name) {
            let _ = stream.write_all(line.as_bytes());
        }
    }
}

/// Players in `room`, `except` excluded.
fn in_room(gs: &GameState, room: &str, except: &str) -> Vec<String> {
    gs.players.values()
        .filter(|p| p.current_room == room && p.username != except)
        .map(|p| p.username.clone())
        .collect()
}

/// Members of the group of `username`, `username` excluded. Empty when
/// `username` is in no group. Read it before a departure, for
/// [`group_leave`].
pub fn group_mates(gs: &GameState, username: &str) -> Vec<String> {
    match group::group_of(gs, username) {
        None => vec![],
        Some(id) => gs.groups[&id].members.iter()
            .filter(|member| *member != username)
            .cloned()
            .collect(),
    }
}

// MARK: - ROOM PRESENCE

/// `EVT ROOM PRESENCE ENTER <username>` to the others in `room`.
pub fn enter(gs: &GameState, username: &str, room: &str) {
    let event = format!("ROOM PRESENCE ENTER {username}");
    send(gs, &in_room(gs, room, username), &event);
}

/// `EVT ROOM PRESENCE LEAVE <username>` to the others in `room`.
pub fn leave(gs: &GameState, username: &str, room: &str) {
    let event = format!("ROOM PRESENCE LEAVE {username}");
    send(gs, &in_room(gs, room, username), &event);
}

/// Presence events of a move from room `from` to the current room of
/// `username`. Nothing when the room did not change.
pub fn moved(gs: &GameState, username: &str, from: &str) {
    let Some(player) = gs.players.get(username) else {
        return;
    };
    if player.current_room != from {
        leave(gs, username, from);
        enter(gs, username, &player.current_room);
    }
}

// MARK: - STATS

/// `EVT STATS players=<n>` to every connected player.
pub fn players_count(gs: &GameState) {
    let everyone: Vec<String> = gs.senders.keys().cloned().collect();
    send(gs, &everyone, &format!("STATS players={}", gs.players.len()));
}

// MARK: - CHAT

/// `401 INVALID_MESSAGE` when `message` holds a control character.
fn check_message(message: &str) -> Result<(), Response> {
    if message.chars().any(char::is_control) {
        return Err(Response::Err { code: 401, message: "INVALID_MESSAGE".to_string() });
    }
    Ok(())
}

/// `CHAT GLOBAL <message>`: `EVT GLOBAL CHAT <username> <message>` to
/// every other connected player. Reply body: empty.
///
/// # Errors
///
/// - `401 INVALID_MESSAGE`: `message` holds a control character.
pub fn chat_global(gs: &GameState, username: &str, message: &str) -> Response {
    if let Err(reply) = check_message(message) {
        return reply;
    }
    let others: Vec<String> = gs.senders.keys()
        .filter(|name| *name != username)
        .cloned()
        .collect();
    send(gs, &others, &format!("GLOBAL CHAT {username} {message}"));
    Response::Ok(String::new())
}

/// `CHAT ROOM <message>`: `EVT ROOM CHAT <username> <message>` to the
/// others in the room of `username`. Reply body: empty.
///
/// # Errors
///
/// - `404 Player not CONNECTED`: `username` is not in the game.
/// - `401 INVALID_MESSAGE`: `message` holds a control character.
pub fn chat_room(gs: &GameState, username: &str, message: &str) -> Response {
    if let Err(reply) = check_message(message) {
        return reply;
    }
    let Some(player) = gs.players.get(username) else {
        return Response::Err { code: 404, message: "Player not CONNECTED".to_string() };
    };
    let others = in_room(gs, &player.current_room, username);
    send(gs, &others, &format!("ROOM CHAT {username} {message}"));
    Response::Ok(String::new())
}

/// `CHAT GROUP <message>`: `EVT GROUP CHAT <username> <message>` to the
/// other members of the group of `username`. Reply body: empty.
///
/// # Errors
///
/// - `401 NOT_IN_GROUP`: `username` is in no group, same code as the
///   `GROUP` commands.
/// - `401 INVALID_MESSAGE`: `message` holds a control character.
pub fn chat_group(gs: &GameState, username: &str, message: &str) -> Response {
    if let Err(reply) = check_message(message) {
        return reply;
    }
    if group::group_of(gs, username).is_none() {
        return Response::Err { code: 401, message: "NOT_IN_GROUP".to_string() };
    }
    send(gs, &group_mates(gs, username), &format!("GROUP CHAT {username} {message}"));
    Response::Ok(String::new())
}

// MARK: - GROUP

/// `EVT GROUP INVITE <leader>` to `target`, invited by `username`.
pub fn group_invite(gs: &GameState, username: &str, target: &str) {
    if let Some(id) = group::group_of(gs, username) {
        let event = format!("GROUP INVITE {}", gs.groups[&id].leader);
        send(gs, &[target.trim().to_string()], &event);
    }
}

/// `EVT GROUP JOIN <username>` to the other members, after the join.
pub fn group_join(gs: &GameState, username: &str) {
    send(gs, &group_mates(gs, username), &format!("GROUP JOIN {username}"));
}

/// `EVT GROUP LEAVE <username>` to `mates`, the members left behind,
/// read before the departure.
pub fn group_leave(gs: &GameState, username: &str, mates: &[String]) {
    send(gs, mates, &format!("GROUP LEAVE {username}"));
}

// MARK: - COMBAT

/// `EVT ROOM COMBAT <username> <npc-id> damage=<n> target_hp=<n>
/// status=<status>` to the others in `room`, where the round was fought.
/// `reply` is the `ATTACK` reply: nothing is sent for an error.
///
/// Not an RFC event, see the module header. Its data follows
/// `EVT STATS players=<n>`: `key=value` pairs, one word each.
pub fn combat(gs: &GameState, username: &str, room: &str, npc_id: &str, reply: &Response) {
    let Response::Ok(body) = reply else {
        return;
    };
    let Ok(round) = serde_json::from_str::<serde_json::Value>(body) else {
        return;
    };
    let event = format!(
        "ROOM COMBAT {username} {npc_id} damage={} target_hp={} status={}",
        round["damage"], round["target_hp"], round["status"].as_str().unwrap_or("unknown")
    );
    send(gs, &in_room(gs, room, username), &event);
}

// MARK: - Departure

/// Removes `username` from the game, by `QUIT` or a lost connection: it
/// leaves its group and its room, loses its stream, and the player count
/// drops. Each step sends its event.
pub fn disconnect(gs: &mut GameState, username: &str) {
    let Some(room) = gs.players.get(username).map(|p| p.current_room.clone()) else {
        return;
    };
    let mates = group_mates(gs, username);
    if let Response::Ok(_) = group::leave(gs, username) {
        group_leave(gs, username, &mates);
    }
    gs.senders.remove(username);
    leave(gs, username, &room);
    gs.players.remove(username);
    players_count(gs);
}

// MARK: - Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::game_with_player;
    use crate::Player;
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};

    /// Gives `name` a real stream in `gs` and returns the client end.
    fn plug(gs: &mut GameState, name: &str) -> BufReader<TcpStream> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server_end, _) = listener.accept().unwrap();
        gs.senders.insert(name.to_string(), server_end);
        BufReader::new(client)
    }

    /// A game with `names` connected and plugged, all at the entrance.
    fn game(names: &[&str]) -> (GameState, Vec<BufReader<TcpStream>>) {
        let mut gs = game_with_player(names[0], "entrance");
        for name in &names[1..] {
            gs.players.insert(name.to_string(), Player::new(name));
        }
        let clients = names.iter().map(|name| plug(&mut gs, name)).collect();
        (gs, clients)
    }

    /// Every line a client got. Drop the game first: closing the server
    /// ends turns the reads into end of file.
    fn inbox(client: BufReader<TcpStream>) -> Vec<String> {
        client.lines().map(|line| line.unwrap()).collect()
    }

    fn group_of_two(gs: &mut GameState) {
        group::create(gs, "alice");
        group::invite(gs, "alice", "bob");
        group::join(gs, "bob", "alice");
    }

    #[test]
    fn chat_global_reaches_everyone_but_the_sender() {
        let (gs, clients) = game(&["alice", "bob", "carol"]);
        chat_global(&gs, "alice", "hello all");
        drop(gs);
        let mut inboxes = clients.into_iter().map(inbox);
        assert!(inboxes.next().unwrap().is_empty());
        assert_eq!(inboxes.next().unwrap(), ["EVT GLOBAL CHAT alice hello all"]);
        assert_eq!(inboxes.next().unwrap(), ["EVT GLOBAL CHAT alice hello all"]);
    }

    #[test]
    fn chat_room_stays_in_the_room() {
        let (mut gs, clients) = game(&["alice", "bob", "carol"]);
        gs.players.get_mut("carol").unwrap().current_room = "corridor".to_string();
        chat_room(&gs, "alice", "psst");
        drop(gs);
        let inboxes: Vec<Vec<String>> = clients.into_iter().map(inbox).collect();
        assert_eq!(inboxes[1], ["EVT ROOM CHAT alice psst"]);
        assert!(inboxes[2].is_empty());
    }

    #[test]
    fn chat_group_needs_a_group() {
        let (mut gs, clients) = game(&["alice", "bob", "carol"]);
        assert!(matches!(
            chat_group(&gs, "alice", "hi"),
            Response::Err { code: 401, .. }
        ));
        group_of_two(&mut gs);
        assert!(matches!(chat_group(&gs, "bob", "hi"), Response::Ok(_)));
        drop(gs);
        let inboxes: Vec<Vec<String>> = clients.into_iter().map(inbox).collect();
        assert_eq!(inboxes[0], ["EVT GROUP CHAT bob hi"]);
        assert!(inboxes[2].is_empty());
    }

    #[test]
    fn chat_refuses_control_characters_keeps_unicode() {
        let (gs, clients) = game(&["alice", "bob"]);
        let escape = "\u{1b}[2Jgotcha";
        assert!(matches!(chat_global(&gs, "alice", escape), Response::Err { code: 401, .. }));
        assert!(matches!(chat_room(&gs, "alice", escape), Response::Err { code: 401, .. }));
        assert!(matches!(chat_global(&gs, "alice", "héllo ✓"), Response::Ok(_)));
        drop(gs);
        let inboxes: Vec<Vec<String>> = clients.into_iter().map(inbox).collect();
        assert_eq!(inboxes[1], ["EVT GLOBAL CHAT alice héllo ✓"]);
    }

    #[test]
    fn move_tells_both_rooms() {
        let (mut gs, clients) = game(&["alice", "bob", "carol"]);
        gs.players.get_mut("carol").unwrap().current_room = "corridor".to_string();
        gs.players.get_mut("alice").unwrap().current_room = "corridor".to_string();
        moved(&gs, "alice", "entrance");
        moved(&gs, "alice", "corridor");
        drop(gs);
        let inboxes: Vec<Vec<String>> = clients.into_iter().map(inbox).collect();
        assert_eq!(inboxes[1], ["EVT ROOM PRESENCE LEAVE alice"]);
        assert_eq!(inboxes[2], ["EVT ROOM PRESENCE ENTER alice"]);
    }

    #[test]
    fn combat_round_reaches_the_room() {
        let (mut gs, clients) = game(&["alice", "bob", "carol"]);
        for name in ["alice", "bob"] {
            gs.players.get_mut(name).unwrap().current_room = "monster_room".to_string();
        }
        let reply = crate::combat::attack(&mut gs, "alice", "Monster");
        combat(&gs, "alice", "monster_room", "monster", &reply);
        let refused = crate::combat::attack(&mut gs, "carol", "monster");
        combat(&gs, "carol", "entrance", "monster", &refused);
        drop(gs);
        let inboxes: Vec<Vec<String>> = clients.into_iter().map(inbox).collect();
        assert!(inboxes[0].is_empty());
        assert_eq!(inboxes[1], ["EVT ROOM COMBAT alice monster damage=5 target_hp=15 status=combat"]);
        assert!(inboxes[2].is_empty());
    }

    #[test]
    fn group_events_follow_the_cycle() {
        let (mut gs, clients) = game(&["alice", "bob"]);
        group::create(&mut gs, "alice");
        group::invite(&mut gs, "alice", "bob");
        group_invite(&gs, "alice", "bob");
        group::join(&mut gs, "bob", "alice");
        group_join(&gs, "bob");
        let mates = group_mates(&gs, "bob");
        group::leave(&mut gs, "bob");
        group_leave(&gs, "bob", &mates);
        drop(gs);
        let inboxes: Vec<Vec<String>> = clients.into_iter().map(inbox).collect();
        assert_eq!(inboxes[0], ["EVT GROUP JOIN bob", "EVT GROUP LEAVE bob"]);
        assert_eq!(inboxes[1], ["EVT GROUP INVITE alice"]);
    }

    #[test]
    fn disconnect_cleans_up_and_tells_everyone() {
        let (mut gs, clients) = game(&["alice", "bob"]);
        group_of_two(&mut gs);
        disconnect(&mut gs, "bob");
        assert!(!gs.players.contains_key("bob"));
        assert!(!gs.senders.contains_key("bob"));
        assert_eq!(group::group_of(&gs, "bob"), None);
        drop(gs);
        let inboxes: Vec<Vec<String>> = clients.into_iter().map(inbox).collect();
        assert_eq!(inboxes[0], [
            "EVT GROUP LEAVE bob",
            "EVT ROOM PRESENCE LEAVE bob",
            "EVT STATS players=1",
        ]);
        assert!(inboxes[1].is_empty());
    }
}
