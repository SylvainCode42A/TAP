use std::sync::{Arc, Mutex};
use crate::{Command, Response, GameState, Player};
use crate::{combat, events, group, items, logging, quest};
use crate::RoomInfo;
use crate::LookResponse;

/// Runs `cmd` for `current_username` and logs the world change it made,
/// if any.
pub fn dispatch(cmd: Command, game_state: &Arc<Mutex<GameState>>, current_username: &Option<String>) -> Response {
    let world_event = logging::world_event(&cmd);
    let reply = run(cmd, game_state, current_username);
    if let (Some((event, arg)), Response::Ok(body)) = (world_event, &reply) {
        logging::world(event, current_username.as_deref(), &arg, body);
    }
    reply
}

fn run(cmd: Command, game_state: &Arc<Mutex<GameState>>, current_username: &Option<String>) -> Response {
    match cmd {
        Command::Connect { username } => {
            let mut gs = game_state.lock().unwrap();
            if gs.players.contains_key(&username) {
                Response::Err { code: 201, message: "NAME_IN_USE".to_string() }
            } else {
                let player = Player::new(&username);
                let room = player.current_room.clone();
                gs.players.insert(username.clone(), player);
                events::enter(&gs, &username, &room);
                events::players_count(&gs);
                // RFC 5.1.1: the reply is exactly "OK connected".
                Response::Ok("connected".to_string())
            }
        },

        Command::Look => {
            match current_username {
                Some(username) => {
                    let gs = game_state.lock().unwrap();
                    match gs.players.get(username) {
                        Some(player) => match gs.world.world.locations.get(&player.current_room) {
                            Some(room) => {
                                let room_id = player.current_room.clone();

                                let players_for_init = gs.players.values()
                                    .filter(|p| p.current_room == room_id)
                                    .map(|p| p.username.clone())
                                    .collect();

                                let npcs_for_init = room.spawns.iter()
                                    .map(|spawn| spawn.npc_type.clone())
                                    .collect();

                                let look_resp = LookResponse {
                                    room: RoomInfo {
                                        id: room_id,
                                        name: room.name.clone(),
                                        description: room.description.clone(),
                                        exits: room.exits.clone(),
                                    },
                                    players: players_for_init,
                                    items: room.items.clone(),
                                    npcs: npcs_for_init,
                                };

                                let look_from_js = serde_json::to_string(&look_resp).unwrap();
                                Response::Ok(look_from_js)
                            }
                            None => Response::Err { code: 404, message: "Current room not found".to_string() }
                        }
                        None => Response::Err { code: 404, message: "Current player not found".to_string() }
                    }
                }
                None => {
                    Response::Err { code: 404, message: "Try with first CONNECTED".to_string() }
                }
            }
        },

        Command::Move { direction } => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let mut gs =  game_state.lock().unwrap();
                    match gs.players.get(username) {
                        None => Response::Err { code: 404, message: "Current room not found".to_string() },
                        Some(player) => {
                            match gs.world.world.locations.get(&player.current_room) {
                                None => Response::Err { code: 404, message: "Current room not found".to_string() },
                                Some(room) => {
                                    match room.exits.get(&direction.to_lowercase()).cloned() {
                                        None => Response::Err { code: 301, message: "NO_EXIT".to_string() },
                                        Some(new_room) => {
                                            let from = gs.players.get_mut(username).map(|p| {
                                                p.in_combat = None;
                                                std::mem::replace(&mut p.current_room, new_room.clone())
                                            });
                                            if let Some(from) = from {
                                                events::moved(&gs, username, &from);
                                            }
                                            Response::Ok(format!("room={}", new_room))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },

        // RFC 8.3: an item is named by its id or its display name.
        Command::Take { item } => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => items::take_item(&mut game_state.lock().unwrap(), username, &item),
        },

        Command::Drop { item } => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => items::drop_item(&mut game_state.lock().unwrap(), username, &item),
        },

        Command::Inventory => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let gs = game_state.lock().unwrap();
                    match gs.players.get(username) {
                        None => Response::Err { code: 404, message: "Player not CONNECTED".to_string() },
                        Some(player) => Response::Ok(serde_json::to_string(&player.inventory).unwrap()),
                    }
                }
            }
        },

        Command::Talk { npc } => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let mut gs = game_state.lock().unwrap();
                    let Some(room) = gs.players.get(username).map(|p| p.current_room.clone()) else {
                        return Response::Err { code: 404, message: "Player not CONNECTED".to_string() };
                    };
                    // RFC 8.2: NPC_NOT_FOUND when the NPC is not in this room.
                    let Some(npc_id) = combat::find_npc_in_room(&gs, &room, &npc) else {
                        return combat::npc_not_found();
                    };
                    // A spawn naming an NPC missing from world.yaml, or an
                    // NPC with nothing to say: 404, never a panic.
                    let dialogue = match gs.world.world.npcs.get(&npc_id) {
                        Some(npc_data) if !npc_data.dialogue.is_empty() => npc_data.dialogue.clone(),
                        _ => return combat::npc_not_found(),
                    };
                    let player = gs.players.get_mut(username).unwrap();
                    let index = player.talk_progress.entry(npc_id).or_insert(0);
                    let phrase = dialogue[*index % dialogue.len()].clone();
                    *index += 1;
                    Response::Ok(phrase)
                }
            }
        },

        Command::Attack { target } => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => {
                // The round is told to the room it was fought in; a defeat
                // then sends the player back to the entrance.
                let mut gs = game_state.lock().unwrap();
                let from = gs.players.get(username).map(|p| p.current_room.clone());
                let reply = combat::attack(&mut gs, username, &target);
                if let Some(from) = from {
                    if let Some(npc_id) = combat::find_npc_in_room(&gs, &from, &target) {
                        events::combat(&gs, username, &from, &npc_id, &reply);
                    }
                    events::moved(&gs, username, &from);
                }
                reply
            }
        },

        Command::Status => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => combat::status(&game_state.lock().unwrap(), username),
        },

        Command::Quest { quest_id } => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => quest::quest(&mut game_state.lock().unwrap(), username, &quest_id),
        },

        Command::Quests => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => quest::quests(&game_state.lock().unwrap(), username),
        },
        Command::Who => {
            let gs = game_state.lock().unwrap();
            Response::Ok(format!("players={}", gs.players.len()))
        },

        Command::GroupCreate => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => group::create(&mut game_state.lock().unwrap(), username),
        },

        Command::GroupInvite { username: target } => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => {
                let mut gs = game_state.lock().unwrap();
                let reply = group::invite(&mut gs, username, &target);
                if let Response::Ok(_) = reply {
                    events::group_invite(&gs, username, &target);
                }
                reply
            }
        },

        // RFC 5.3.3: the argument of GROUP JOIN is the leader's name.
        Command::GroupJoin { group_id: leader } => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => {
                let mut gs = game_state.lock().unwrap();
                let reply = group::join(&mut gs, username, &leader);
                if let Response::Ok(_) = reply {
                    events::group_join(&gs, username);
                }
                reply
            }
        },

        Command::GroupLeave => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => {
                let mut gs = game_state.lock().unwrap();
                let mates = events::group_mates(&gs, username);
                let reply = group::leave(&mut gs, username);
                if let Response::Ok(_) = reply {
                    events::group_leave(&gs, username, &mates);
                }
                reply
            }
        },

        Command::ChatGlobal { message } => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => events::chat_global(&game_state.lock().unwrap(), username, &message),
        },

        Command::ChatRoom { message } => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => events::chat_room(&game_state.lock().unwrap(), username, &message),
        },

        Command::ChatGroup { message } => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => events::chat_group(&game_state.lock().unwrap(), username, &message),
        },

        Command::Quit => Response::Ok("bye".to_string()),
    }
}

// MARK: - Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{game_with_player, ok_body};
    use crate::Spawn;

    /// Runs `cmd` as `username` on `gs`.
    fn run(gs: &Arc<Mutex<GameState>>, username: &str, cmd: Command) -> Response {
        dispatch(cmd, gs, &Some(username.to_string()))
    }

    fn talk(npc: &str) -> Command {
        Command::Talk { npc: npc.to_string() }
    }

    #[test]
    fn talk_needs_the_npc_in_the_room() {
        // The merchant waits at the entrance, alice is in the corridor.
        let gs = Arc::new(Mutex::new(game_with_player("alice", "corridor")));
        assert!(matches!(run(&gs, "alice", talk("merchant")), Response::Err { code: 404, .. }));
    }

    #[test]
    fn talk_takes_id_or_name_and_cycles() {
        let gs = Arc::new(Mutex::new(game_with_player("alice", "entrance")));
        let first = ok_body(run(&gs, "alice", talk("merchant")));
        let second = ok_body(run(&gs, "alice", talk("Merchant")));
        assert_ne!(first, second);
        assert_eq!(ok_body(run(&gs, "alice", talk("merchant"))), first);
    }

    #[test]
    fn talk_to_undefined_npc_is_404_not_panic() {
        let mut game = game_with_player("alice", "corridor");
        let corridor = game.world.world.locations.get_mut("corridor").unwrap();
        corridor.spawns.push(Spawn { npc_type: "phantom".to_string(), count: 1 });
        let gs = Arc::new(Mutex::new(game));
        assert!(matches!(run(&gs, "alice", talk("phantom")), Response::Err { code: 404, .. }));
    }

    #[test]
    fn inventory_is_a_json_list() {
        let mut game = game_with_player("alice", "entrance");
        let inventory = &mut game.players.get_mut("alice").unwrap().inventory;
        inventory.push("sword".to_string());
        inventory.push("say \"hi\"".to_string());
        let gs = Arc::new(Mutex::new(game));
        let body = ok_body(run(&gs, "alice", Command::Inventory));
        assert_eq!(body, r#"["sword","say \"hi\""]"#);
    }

    #[test]
    fn move_replies_the_room_id() {
        let gs = Arc::new(Mutex::new(game_with_player("alice", "entrance")));
        let reply = run(&gs, "alice", Command::Move { direction: "NORTH".to_string() });
        assert_eq!(ok_body(reply), "room=corridor");
    }
}
