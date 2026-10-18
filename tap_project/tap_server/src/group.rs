//! Groups: the `GROUP CREATE`, `GROUP INVITE`, `GROUP JOIN` and
//! `GROUP LEAVE` commands, RFC 5.3.
//!
//! Rules the RFC leaves open, chosen by the team:
//!
//! - a player belongs to one group at most;
//! - any member may invite; joining needs an invitation to that group;
//! - `GROUP JOIN` names the group by its current leader;
//! - when the leader leaves, the oldest member leads; the last member
//!   leaving dissolves the group and its pending invitations.
//!
//! Membership is stored once, in [`Group::members`]: a player's group is
//! found by looking, never kept in a second place that could disagree.
//!
//! - Owns: groups, leadership, invitations, the replies of the four
//!   commands.
//! - Does NOT own: the `EVT GROUP ...` events and `CHAT GROUP` delivery,
//!   which need the player streams of the server.

use std::collections::HashSet;

use crate::{GameState, Response};

// MARK: - Group

/// One group of players.
pub struct Group {
    /// Current leader, always one of `members`.
    pub leader: String,
    /// Members in joining order, leader included. The first one after the
    /// leader is the oldest, next in line to lead.
    pub members: Vec<String>,
    /// Players invited to this group who have not joined yet.
    pub invited: HashSet<String>,
}

/// Id of the group `username` belongs to, or `None`.
pub fn group_of(gs: &GameState, username: &str) -> Option<String> {
    for (id, group) in &gs.groups {
        if group.members.iter().any(|member| member == username) {
            return Some(id.clone());
        }
    }
    None
}

// MARK: - GROUP CREATE

/// Creates a group led by `username`, RFC 5.3.1.
///
/// The group id is `grp.<founder>` and never changes, even when the
/// leadership passes to another member. Reply body: `group=<group-id>`.
///
/// # Errors
///
/// - `404 Player not CONNECTED`: `username` is not in the game.
/// - `402 ALREADY_IN_GROUP`: `username` is already in a group.
pub fn create(gs: &mut GameState, username: &str) -> Response {
    if !gs.players.contains_key(username) {
        return not_connected();
    }
    if group_of(gs, username).is_some() {
        return already_in_group();
    }

    // A founder who left and founds again would reuse the same id: free
    // ids only, by adding a number when needed.
    let mut id = format!("grp.{username}");
    let mut suffix = 2;
    while gs.groups.contains_key(&id) {
        id = format!("grp.{username}.{suffix}");
        suffix += 1;
    }

    gs.groups.insert(id.clone(), Group {
        leader: username.to_string(),
        members: vec![username.to_string()],
        invited: HashSet::new(),
    });
    Response::Ok(format!("group={id}"))
}

// MARK: - GROUP INVITE

/// Invites `target` to the group of `username`, RFC 5.3.2.
///
/// Any member may invite. Inviting a player who is already in a group is
/// allowed: the invitation waits until that player leaves and joins.
///
/// # Errors
///
/// - `404 Player not CONNECTED`: `username` is not in the game.
/// - `401 NOT_IN_GROUP`: `username` is in no group.
/// - `404 PLAYER_NOT_FOUND`: no connected player is called `target`.
pub fn invite(gs: &mut GameState, username: &str, target: &str) -> Response {
    if !gs.players.contains_key(username) {
        return not_connected();
    }
    let Some(id) = group_of(gs, username) else {
        return not_in_group();
    };
    let target = target.trim();
    if !gs.players.contains_key(target) {
        return Response::Err {
            code: 404,
            message: "PLAYER_NOT_FOUND".to_string(),
        };
    }

    let group = gs.groups.get_mut(&id).unwrap();
    if !group.members.iter().any(|member| member == target) {
        group.invited.insert(target.to_string());
    }
    Response::Ok(String::new())
}

// MARK: - GROUP JOIN

/// Joins the group currently led by `leader`, RFC 5.3.3.
///
/// Needs a pending invitation to that group, which is used up. Reply body:
/// `group=<group-id>`.
///
/// # Errors
///
/// - `404 Player not CONNECTED`: `username` is not in the game.
/// - `402 ALREADY_IN_GROUP`: `username` is already in a group.
/// - `403 NOT_INVITED`: no group is led by `leader`, or it never invited
///   `username`. Not an RFC code: the RFC defines none for this case.
pub fn join(gs: &mut GameState, username: &str, leader: &str) -> Response {
    if !gs.players.contains_key(username) {
        return not_connected();
    }
    if group_of(gs, username).is_some() {
        return already_in_group();
    }

    let leader = leader.trim();
    let mut target_id: Option<String> = None;
    for (id, group) in &gs.groups {
        if group.leader == leader && group.invited.contains(username) {
            target_id = Some(id.clone());
        }
    }
    let Some(id) = target_id else {
        return Response::Err { code: 403, message: "NOT_INVITED".to_string() };
    };

    let group = gs.groups.get_mut(&id).unwrap();
    group.invited.remove(username);
    group.members.push(username.to_string());
    Response::Ok(format!("group={id}"))
}

// MARK: - GROUP LEAVE

/// Removes `username` from its group, RFC 5.3.4.
///
/// A leaving leader hands over to the oldest member. The last member
/// leaving dissolves the group, its invitations included. Also meant to
/// be called when a player disconnects.
///
/// # Errors
///
/// - `404 Player not CONNECTED`: `username` is not in the game.
/// - `401 NOT_IN_GROUP`: `username` is in no group.
pub fn leave(gs: &mut GameState, username: &str) -> Response {
    if !gs.players.contains_key(username) {
        return not_connected();
    }
    let Some(id) = group_of(gs, username) else {
        return not_in_group();
    };

    let group = gs.groups.get_mut(&id).unwrap();
    group.members.retain(|member| member != username);
    if group.members.is_empty() {
        gs.groups.remove(&id);
    } else if group.leader == username {
        group.leader = group.members[0].clone();
    }
    Response::Ok(String::new())
}

// MARK: - Error replies

fn not_connected() -> Response {
    Response::Err { code: 404, message: "Player not CONNECTED".to_string() }
}

fn already_in_group() -> Response {
    Response::Err { code: 402, message: "ALREADY_IN_GROUP".to_string() }
}

fn not_in_group() -> Response {
    Response::Err { code: 401, message: "NOT_IN_GROUP".to_string() }
}

// MARK: - Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{game_with_player, ok_body};
    use crate::Player;

    /// A game with `names` connected, all at the entrance.
    fn game(names: &[&str]) -> GameState {
        let mut gs = game_with_player(names[0], "entrance");
        for name in &names[1..] {
            gs.players.insert(name.to_string(), Player::new(name));
        }
        gs
    }

    fn code(reply: Response) -> u16 {
        match reply {
            Response::Ok(_) => 0,
            Response::Err { code, .. } => code,
        }
    }

    #[test]
    fn create_then_create_again_is_402() {
        let mut gs = game(&["alice"]);
        assert_eq!(ok_body(create(&mut gs, "alice")), "group=grp.alice");
        assert_eq!(code(create(&mut gs, "alice")), 402);
    }

    #[test]
    fn invite_join_leave_cycle() {
        let mut gs = game(&["alice", "bob"]);
        create(&mut gs, "alice");
        ok_body(invite(&mut gs, "alice", "bob"));
        assert_eq!(ok_body(join(&mut gs, "bob", "alice")), "group=grp.alice");
        assert_eq!(group_of(&gs, "bob"), Some("grp.alice".to_string()));

        ok_body(leave(&mut gs, "bob"));
        assert_eq!(group_of(&gs, "bob"), None);
        assert_eq!(gs.groups["grp.alice"].members, vec!["alice"]);
    }

    #[test]
    fn join_needs_an_invitation_used_once() {
        let mut gs = game(&["alice", "bob"]);
        create(&mut gs, "alice");
        assert_eq!(code(join(&mut gs, "bob", "alice")), 403);

        invite(&mut gs, "alice", "bob");
        join(&mut gs, "bob", "alice");
        leave(&mut gs, "bob");
        assert_eq!(code(join(&mut gs, "bob", "alice")), 403);
    }

    #[test]
    fn join_unknown_leader_is_403() {
        let mut gs = game(&["alice"]);
        assert_eq!(code(join(&mut gs, "alice", "nobody")), 403);
    }

    #[test]
    fn invite_errors() {
        let mut gs = game(&["alice", "bob"]);
        assert_eq!(code(invite(&mut gs, "alice", "bob")), 401);
        create(&mut gs, "alice");
        assert_eq!(code(invite(&mut gs, "alice", "ghost")), 404);
    }

    #[test]
    fn any_member_may_invite() {
        let mut gs = game(&["alice", "bob", "carol"]);
        create(&mut gs, "alice");
        invite(&mut gs, "alice", "bob");
        join(&mut gs, "bob", "alice");
        ok_body(invite(&mut gs, "bob", "carol"));
        let joined = ok_body(join(&mut gs, "carol", "alice"));
        assert_eq!(joined, "group=grp.alice");
    }

    #[test]
    fn leaving_leader_hands_over_to_oldest_member() {
        let mut gs = game(&["alice", "bob", "carol"]);
        create(&mut gs, "alice");
        for name in ["bob", "carol"] {
            invite(&mut gs, "alice", name);
            join(&mut gs, name, "alice");
        }
        leave(&mut gs, "alice");
        assert_eq!(gs.groups["grp.alice"].leader, "bob");

        // JOIN now names the group by its new leader.
        gs.players.insert("dave".to_string(), Player::new("dave"));
        invite(&mut gs, "carol", "dave");
        assert_eq!(code(join(&mut gs, "dave", "alice")), 403);
        assert_eq!(ok_body(join(&mut gs, "dave", "bob")), "group=grp.alice");
    }

    #[test]
    fn last_member_leaving_dissolves_group() {
        let mut gs = game(&["alice", "bob"]);
        create(&mut gs, "alice");
        invite(&mut gs, "alice", "bob");
        leave(&mut gs, "alice");
        assert!(gs.groups.is_empty());
        assert_eq!(code(join(&mut gs, "bob", "alice")), 403);
    }

    #[test]
    fn leave_outside_group_is_401() {
        let mut gs = game(&["alice"]);
        assert_eq!(code(leave(&mut gs, "alice")), 401);
    }

    #[test]
    fn refounding_gets_a_fresh_id() {
        let mut gs = game(&["alice", "bob"]);
        create(&mut gs, "alice");
        invite(&mut gs, "alice", "bob");
        join(&mut gs, "bob", "alice");
        leave(&mut gs, "alice");
        assert_eq!(ok_body(create(&mut gs, "alice")), "group=grp.alice.2");
    }
}
