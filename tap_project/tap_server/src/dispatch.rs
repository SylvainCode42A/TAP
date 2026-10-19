use std::sync::{Arc, Mutex};
use crate::{Command, Response, GameState, Player};
use crate::{combat, group, quest};
use crate::RoomInfo;
use crate::LookResponse;

pub fn dispatch(cmd: Command, game_state: &Arc<Mutex<GameState>>, current_username: &Option<String>) -> Response {
    match cmd {
        Command::Connect { username } => {
            let mut gs = game_state.lock().unwrap();
            if gs.players.contains_key(&username) {
                Response::Err { code: 201, message: "NAME_IN_USE".to_string() }
            } else {
                gs.players.insert(username.clone(), Player::new(&username));
                Response::Ok(format!("{} connected", username))
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
                                Response::Ok(format!("{}", look_from_js))
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
                                            if let Some(p) = gs.players.get_mut(username) {
                                                p.current_room = new_room.clone();
                                                p.in_combat = None;
                                            }
                                            Response::Ok(format!("room=next_is_{}", new_room))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },

        Command::Take { item } => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let mut gs = game_state.lock().unwrap();
                    let current_room = match gs.players.get(username) {
                        None => return Response::Err { code: 404, message: "Player not connected".to_string() },
                        Some(player) => player.current_room.clone(),
                    };
                    match gs.world.world.locations.get_mut(&current_room) {
                        None => Response::Err { code: 404, message: "Current room not found".to_string() },
                        Some(room) => {
                            if !room.items.contains(&item) {
                                Response::Err { code: 404, message: "ITEM_NOT_FOUND".to_string() }
                            } else {
                                room.items.retain(|i| i != &item);
                                if let Some(p) = gs.players.get_mut(username) {
                                    p.inventory.push(item.clone());
                                }
                                Response::Ok(format!("taken={}", item))
                            }
                        }
                    }
                }
            }
        },

        Command::Drop { item } => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let mut gs = game_state.lock().unwrap();
                    let current_room = match gs.players.get_mut(username) {
                        None => return Response::Err { code: 404, message: "Player not connected".to_string() },
                        Some(player) => player.current_room.clone(),
                    };
                    let has_item = match gs.players.get(username) {
                        None => return Response::Err { code: 404, message: "Player not in room".to_string() },
                        Some(player) => player.inventory.contains(&item)
                    };
                    if !has_item {
                        Response::Err { code: 404, message: "ITEM_NOT_IN_INVENTORY".to_string() }
                    } else {
                        if let Some(player) = gs.players.get_mut(username) {
                            player.inventory.retain(|i| i != &item)
                        }
                        if let Some(room) = gs.world.world.locations.get_mut(&current_room) {
                            room.items.push(item.clone());
                        }
                        Response::Ok(format!("dropped={}", item))
                    }
                }
            }
        },

        Command::Inventory => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let gs = game_state.lock().unwrap();
                    match gs.players.get(username) {
                        None => Response::Err { code: 404, message: "Player not CONNECTED".to_string() },
                        Some(player) => Response::Ok(format!("{:?}", player.inventory)),
                    }
                }
            }
        },

        Command::Talk { npc } => {
            match current_username {
                None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
                Some(username) => {
                    let mut gs = game_state.lock().unwrap();
                    match gs.world.world.npcs.get(&npc) {
                        None => Response::Err { code: 404, message: "NPC_NOT_FOUND".to_string() },
                        Some(npc_data) => {
                            let dialogue = npc_data.dialogue.clone();
                            match gs.players.get_mut(username) {
                                None => Response::Err { code: 404, message: "Player not CONNECTED".to_string() },
                                Some(player) => {
                                    let index = player.talk_progress.entry(npc.clone()).or_insert(0);
                                    let phrase = dialogue[*index % dialogue.len()].clone();
                                    *index += 1;
                                    Response::Ok(phrase)
                                }
                            }
                        }
                    }
                }
            }
        },

        Command::Attack { target } => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => combat::attack(&mut game_state.lock().unwrap(), username, &target),
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
            Some(username) => group::invite(&mut game_state.lock().unwrap(), username, &target),
        },

        // RFC 5.3.3: the argument of GROUP JOIN is the leader's name.
        Command::GroupJoin { group_id: leader } => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => group::join(&mut game_state.lock().unwrap(), username, &leader),
        },

        Command::GroupLeave => match current_username {
            None => Response::Err { code: 404, message: "Try with first CONNECTED".to_string() },
            Some(username) => group::leave(&mut game_state.lock().unwrap(), username),
        },
        Command::ChatGlobal { message } => Response::Err { code: 404, message: format!("ChatGlobal not found to send {}", message) },
        Command::ChatRoom { message } => Response::Err { code: 404, message: format!("ChatRoom not found to send {}", message) },
        Command::ChatGroup { message } => Response::Err { code: 404, message: format!("ChatGroup not found to send {}", message) },
        Command::Quit => Response::Ok("bye".to_string()),
    }
}