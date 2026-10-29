*This project has been created as part of the 42 curriculum by jferone, slidriss, srezzaq.*

<h1 align="center">TAP</h1>

<p align="center">
  The Answer Protocol<br>
  <i>a shared-world text adventure over TCP, in Rust</i>
</p>
<p align="center">
  <a href="#instructions"><img alt="Rust 2024" src="https://img.shields.io/badge/Rust-2024-4A6CF7?labelColor=0A0D12"></a>
  <a href="#protocol-implementation"><img alt="RFC 42TAP" src="https://img.shields.io/badge/protocol-RFC_42TAP-4A6CF7?labelColor=0A0D12"></a>
  <a href="#building-and-running"><img alt="GUI egui" src="https://img.shields.io/badge/GUI-egui-4A6CF7?labelColor=0A0D12"></a>
  <a href="#server-logging"><img alt="logs JSON Lines" src="https://img.shields.io/badge/logs-JSON_Lines-4A6CF7?labelColor=0A0D12"></a>
</p>

---

<h2 align="center" id="description">DESCRIPTION</h2>

### Quick read

A shared-world text adventure: several players explore one dungeon, talk
to NPCs, pick up items, fight a monster and complete quests together. The
goal of a game: defeat the monster and take its treasure.

### What is behind

- One world, many clients acting at once: it must stay consistent.
- One written protocol: RFC 42TAP over TCP, where any client talks to any server that follows it.
- Protocol usage: one UTF-8 line per message, commands from the client,
`OK` or `ERR` replies and asynchronous `EVT` events from the server.

It is played on a terminal (CLI) or in a window (GUI), and by any client
another group wrote against the same RFC.

| Rust programs | Path | Role |
|---------------|------|------|
| Server | `tap_project/tap_server` | world state, rules, protocol, logs |
| CLI client | `tap_project/tap_client` | type protocol commands, see every line live |
| GUI client | `tap_project/tap_gui` | egui window: room, inventory, buttons, chat |

---

<h2 align="center" id="instructions">INSTRUCTIONS</h2>

Requirements: a Rust toolchain (`rustup`, edition 2024, with `clippy` and
`rustfmt`) and `make`. The GUI needs the usual X11 or Wayland libraries of
a Linux desktop.

| I want to | Run |
|-----------|-----|
| fetch the dependencies | `make install` |
| start the server | `make run-server` |
| play in a terminal | `make run-client` |
| play in a window | `make run-client-gui` |
| play with no client at all | `nc 127.0.0.1 7878` |
| join a server on another machine | `make run-client HOST=<ip>` or `make run-client-gui HOST=<ip>` |
| check the code | `make test`, then `make lint` |

Details in [Building and Running](#building-and-running).

---

<h2 align="center" id="architecture">ARCHITECTURE</h2>

```mermaid
flowchart LR
    C[client socket] --> T[one thread per client<br>parsing.rs]
    T -->|Command| D[dispatch.rs]
    D --> M[combat / quest / group / items]
    M --> S[(GameState<br>Arc Mutex)]
    D --> E[events.rs]
    E -->|EVT lines| O[other clients]
    D -->|Response| T
    T -->|OK / ERR| C
    T & D & E -.-> L[logging.rs<br>writer thread]
```

- **Concurrency model**: one OS thread per connection, all sharing one
  `GameState` behind an `Arc<Mutex<...>>`. A command takes the lock once,
  changes the world and sends its events, then releases it. Commands are
  therefore applied one at a time, in arrival order, which is also the
  turn order of combat. The world is small, so one lock is simpler than
  fine-grained locking and cannot deadlock.
- **Dispatcher, not inline handling**: `parsing.rs` turns a line into a
  typed `enum Command`; `dispatch.rs` has one arm per command and returns a
  `Response` (`Ok(body)` or `Err { code, message }`). Game rules live in
  their own modules (`combat.rs`, `quest.rs`, `group.rs`, `items.rs`),
  each owning one part of the state and tested without any socket.
- **Events**: `events.rs` writes every `EVT` line to the streams kept in
  `GameState.senders`, while the lock is held, so two events never
  interleave on one socket. A stream that fails to write is skipped: its
  own thread sees the broken connection and cleans up.
- **Disconnection**: `QUIT`, end of stream and read errors all run the
  same cleanup: leave the group, leave the room, drop the stream, update
  the player count, each step sending its event.
- **Startup**: `world_check.rs` validates `world.yaml` (subject minimums,
  exits, references, one connected map with a loop) and refuses to start on a broken world, listing every
  problem at once.

### Life of a command

1. `parsing.rs` reads one line (1024 bytes at most) and logs it as `command`.
2. It turns the line into a `Command`, or answers `ERR 401` at once: a
   malformed line never reaches the world.
3. `dispatch.rs` takes the world lock; the rule module changes `GameState`.
4. Still under the lock, `events.rs` sends the `EVT` lines to the other
   players concerned; then the lock is released.
5. `parsing.rs` writes the `OK` or `ERR` reply and logs it as `reply`.

### Design rules

- **The server is the only authority.** Clients send intents; damage,
  item ownership and quest conditions are computed on the server.
- **One lock, one command at a time.** Arrival order is the turn order.
- **No player hears their own event.** The `OK` reply already told them.
- **No broken world starts.** Every problem of `world.yaml` is listed at once.
- **Abuse is logged, not blocked.** See [Server Logging](#server-logging).
- **Every departure from the RFC is written down.** See below.

### Repository layout

```txt
Makefile                     build tool: install, build, run-*, test, lint, clean
tap_project/
  tap_server/
    data/world.yaml          the world: rooms, items, NPCs, quests
    logs/server.jsonl        written at run time, git-ignored
    src/
      main.rs                shared types, accept loop, startup
      world_check.rs         validates world.yaml before the server starts
      parsing.rs             one client: framing, keywords, OK and ERR, flood watch
      dispatch.rs            one arm per command, under the world lock
      combat.rs              ATTACK rounds, death and respawn
      quest.rs               QUEST and QUESTS, conditions read from the player
      group.rs               GROUP commands, leader succession
      items.rs               TAKE, DROP, lookup by id or name
      events.rs              EVT lines, disconnection cleanup
      logging.rs             JSON Lines, writer thread, abuse counters
      game_stress.rs         random campaigns checking invariants, tests only
  tap_client/
    src/main.rs              CLI: reader thread and keyboard loop
    assets/                  room and item pictures, shown by the GUI
  tap_gui/src/               egui client: network thread, reply parsing, room view, map, log
```

---

<h2 align="center" id="protocol-implementation">PROTOCOL IMPLEMENTATION</h2>

The server follows RFC 42TAP. Where the RFC is silent or inconsistent, the
choices below were made.

| Topic | RFC | Our choice | Why |
|---|---|---|---|
| Combat broadcast | no event defined | `EVT ROOM COMBAT <user> <npc-id> damage=<n> target_hp=<n> status=<s>` to the others in the room | the subject asks combat results to reach the players around; `key=value` data as in `EVT STATS players=<n>` |
| Event syntax | 4.1 ABNF: `event-type = 1*ALPHA`; 7.1 tables: `EVT <category> <type> <data>` | the 7.1 tables (`EVT ROOM PRESENCE ENTER bob`) | the tables define every event; the ABNF is too narrow for them |
| Own events | silent | an event never goes back to the player who caused it | that player already has the `OK` reply |
| `EVT GROUP INVITE` | `<leader>` | the leader of the inviting group, whoever invited | it is the name `GROUP JOIN` expects |
| Unknown command, wrong arguments | no code | `ERR 401 Unknown command`, `ERR 401 Invalid number of arguments for <CMD>` | 4xx is non-fatal for every client (RFC 7.3) |
| Command before `CONNECT` | no code | `ERR 404 Try with first CONNECTED` | same reason |
| Second `CONNECT` | no code | `ERR 401 <name> already connected, use QUIT first` | same reason |
| Name in use | `ERR 201 NAME_IN_USE`, case not specified | case-insensitive: `Alice` is refused while `alice` plays | two players must stay distinguishable in `WHO`, chat and events |
| Line over 1024 bytes | 9.4 recommends a limit | `ERR 401 LINE_TOO_LONG` | RFC 9.4 |
| Control character in a chat message | 9.2: reject or neutralise | `ERR 401 INVALID_MESSAGE` | relayed as is, an escape sequence would drive other players' terminals |
| Fixed item (`obtainable: false`) | no code | `ERR 403 ITEM_NOT_OBTAINABLE` | the item stays visible in its room |
| `GROUP JOIN` without invitation | no code | `ERR 403 NOT_INVITED` | joining needs an invitation to that group |
| `GROUP INVITE` of an unknown player | no code | `ERR 404 PLAYER_NOT_FOUND` | same family as the other 404 lookups |
| Identifiers | examples use `loc.`, `item.`, `npc.` prefixes | bare ids from `world.yaml` (`cell_key`, `monster`) | the RFC shows the prefixes in examples only; every reply uses the same ids as LOOK |
| `QUEST` reply status | example shows `available` | first `QUEST` accepts at once: `active` | one command both accepts and turns in, see [Quest System](#quest-system) |

### Example session

Lines from a recorded run, `C:` sent by alice's client, `S:` sent by the
server. `...` marks lines left out.

```txt
S: OK hello proto=1
C: CONNECT alice
S: OK connected
S: EVT ROOM PRESENCE ENTER bob
S: EVT STATS players=2
C: MOVE east
S: OK room=guard_room
C: TAKE Cell Key
S: OK taken=cell_key
...
C: ATTACK monster
S: OK {"attacker_hp": 88, "target_hp": 5, "damage": 15, "status": "combat"}
C: ATTACK monster
S: OK {"attacker_hp": 88, "target_hp": 0, "damage": 15, "status": "victory"}
```

---

<h2 align="center" id="combat-system">COMBAT SYSTEM</h2>

One `ATTACK <npc>` is one full round: the player strikes, then the NPC
strikes back if it is still standing.

| Rule | Value |
|---|---|
| Player health | 100 HP at start, never above 100 |
| Player damage | 5, plus 10 with the `sword` in the inventory |
| NPC counter-attack | its `attack` stat in `world.yaml` (5 if missing) |
| Monster | 20 HP, attack 12 |
| Initiative | the attacker always strikes first; there is no roll |
| Turn order | arrival order of commands, serialised by the world lock |
| Who can be attacked | NPCs marked `hostile: true` only, else `ERR 405 NPC_NOT_HOSTILE` |
| NPC health | shared by every player: two players hitting the monster add up |
| NPC death | the kill goes to the player who lands the last hit; the NPC rises again at full health at once, so every player can finish the treasure quest |
| Player death | at 0 HP the player respawns at the entrance with 50 HP |

Combat states, as reported by `STATUS` and `ATTACK`:

```mermaid
stateDiagram-v2
    direction LR
    healthy --> combat: ATTACK
    wounded --> combat: ATTACK
    combat --> combat: ATTACK
    combat --> wounded: fight ends
    wounded --> healthy: quest done
```

| From | When | To |
|---|---|---|
| `healthy` or `wounded` | `ATTACK`, both sides still standing | `combat` |
| `combat` | `ATTACK`, both sides still standing | `combat` |
| `combat` | victory, `MOVE` away, or defeat (respawn at the entrance with 50 HP) | `wounded` |
| `wounded` | a quest turned in (full heal) | `healthy` |

`STATUS` answers `healthy` (full HP), `wounded` (below 100 HP) or `combat`
(the last round left both sides standing). Leaving the room ends a fight:
`MOVE` is the way to flee, so no `FLEE` command is needed. There is no
`DEFEND`, critical hit or status effect: with one enemy and four items,
the sword is the one tactical choice. With it the monster falls in two
rounds and the player takes 12 damage; bare-handed it takes four rounds
and 36 damage.

Every round is broadcast to the room as `EVT ROOM COMBAT`.

---

<h2 align="center" id="quest-system">QUEST SYSTEM</h2>

A quest is given by one NPC (`giver`) and is done when its conditions
hold: an item in the inventory (`requires_item`), an NPC beaten
(`requires_kill`), or both. The server checks them, never the client.

```mermaid
stateDiagram-v2
    direction LR
    [*] --> active: QUEST npc
    active --> active: not met
    active --> completed: met
    completed --> [*]
```

| From | When | To |
|---|---|---|
| no quest | first `QUEST <npc>` | `active` |
| `active` | `QUEST <npc>`, conditions not met: progress shown | `active` |
| `active` | `QUEST <npc>`, conditions met | `completed` |

- **Progression**: conditions are read from the player's state when asked,
  so nothing can drift: `QUESTS` shows `progress: "met/needed"` for each
  active quest.
- **Completion**: manual, by asking the giver again with the same `QUEST
  <npc>`. One command accepts and turns in, so no `COMPLETE_QUEST` is
  needed; there is no `ABANDON_QUEST` either, a quest never blocks another.
- **Rewards**: turning in a quest fully heals the player. A quest never
  hands out an item: items are unique in the world.
- **Dependencies**: none between the two quests; the treasure quest needs
  the monster beaten and the treasure taken, in any order.
- **When nothing is left**: `ERR 406 NO_QUEST_AVAILABLE`.

| Quest | Giver | Conditions |
|---|---|---|
| `free_prisoner` - Free the Prisoner | prisoner (secret cell) | hold the `cell_key` |
| `treasure_quest` - The Treasure | merchant (entrance) | beat the `monster`, hold the `treasure` |

---

<h2 align="center" id="world-design">WORLD DESIGN</h2>

<!-- review: slidriss -->

Eight rooms: a loop of six, plus two dead ends. Room pictures for the
GUI are in `tap_project/tap_client/assets`.

```mermaid
flowchart TB
    treasure["Treasure Room<br>treasure"]
    monster["Monster Lair<br>monster, enemy"]
    library["Library<br>shield, fixed"]
    armory["Armory<br>sword"]
    corridor["Corridor<br>torch, fixed"]
    guard["Guard Room<br>cell key"]
    entrance["Dungeon Entrance<br>merchant, start"]
    cell["Secret Cell<br>prisoner"]
    monster -- east / west --- treasure
    library -- north / south --- monster
    armory -- north / south_alt --- monster
    corridor -- north / south --- library
    guard -- east / west --- armory
    entrance -- north / south --- corridor
    entrance -- east / west --- guard
    entrance -- south / north --- cell
```

| Room | NPC | Item |
|---|---|---|
| Dungeon Entrance | merchant (dialogue, quest giver) | |
| Corridor | | torch (fixed) |
| Guard Room | | cell key |
| Library | | shield (fixed) |
| Armory | | sword |
| Monster Lair | monster (enemy) | |
| Treasure Room | | treasure |
| Secret Cell | prisoner (dialogue, quest giver) | |

- NPC roles: the merchant gives advice and the treasure quest, the
  prisoner gives the rescue quest, the monster is the one enemy.
- Items: five, three of them obtainable (sword, cell key, treasure); the
  torch and the shield are scenery that LOOK shows but TAKE refuses.
- The loop entrance - corridor - library - monster lair - armory - guard
  room - entrance lets players reach the monster by two paths.

---

<h2 align="center" id="server-logging">SERVER LOGGING</h2>

Every log line is one JSON object (JSON Lines), written to the console and
appended to `tap_project/tap_server/logs/server.jsonl`:

```json
{"ts":"2026-10-22T14:23:20.512Z","level":"INFO","event":"item_taken","arg":"Cell Key","player":"alice","result":"taken=cell_key"}
```

`ts` (UTC, milliseconds), `level` and `event` always come first. Lines go
through a channel to one writer thread, so a slow disk never holds a
client thread or the world lock.

| Level | Events |
|---|---|
| INFO | `server_start`, `connection_open`, `connection_close` (with `reason`: quit, eof, reset, write_failed, read_error), `command` (every line received), `reply` (every `OK` sent) |
| INFO | world changes: `player_joined`, `player_moved`, `item_taken`, `item_dropped`, `npc_talk`, `combat_round`, `quest_update`, `quest_completed`, `group_created`, `group_invite`, `group_joined`, `group_left`, `chat_global`, `chat_room`, `chat_group` |
| WARN | `reply` for every `ERR` sent; abuse: `abuse_command_flood` (over 20 commands in 1 s on one connection), `abuse_rapid_connections` (over 10 connections in 10 s from one IP), `abuse_oversized_line`; `log_file_unavailable` |
| ERROR | `read_error`, `accept_failed` |

Connection lines carry `ip` as `address:port`, which identifies one
connection; `command` and `reply` lines also carry `player` once known.
An abuse burst is logged once, not once per command. Abuse is logged, not
blocked.

Monitoring with `jq`:

```sh
tail -f tap_project/tap_server/logs/server.jsonl | jq -c 'select(.level != "INFO")'    # problems only
jq -c 'select(.event | startswith("abuse_"))' tap_project/tap_server/logs/server.jsonl # abuse
jq -c 'select(.player == "alice")' tap_project/tap_server/logs/server.jsonl            # one player
```

---

<h2 align="center" id="group-contributions">GROUP CONTRIBUTIONS</h2>

<!-- review: each member checks their own line -->

| Member | Area | Parts |
|---|---|---|
| **slidriss** | network | TCP server, one thread per client, greeting, player streams for broadcast, disconnection, pipelined commands fix |
| | protocol | parsing into `Command`, dispatcher, case-insensitive commands, RFC error codes, LOOK JSON |
| | world | world data, room pictures |
| **srezzaq** | CLI client | the whole client |
| | GUI client | egui: room view with pictures and icons, inventory, action buttons, items by name or id, counters, NPC dialogue, health bar, quests, minimap, live updates from `EVT`, chat view, group invitations, reconnection |
| **jferone** | game engine | combat, quests, groups, items, world validation, WHO |
| | events | CHAT and every `EVT` |
| | operations | server logging, RFC conformance fixes, Makefile, README assembly |

---

<h2 align="center" id="building-and-running">BUILDING & RUNNING</h2>

The root `Makefile` drives `cargo` in the three crates:

| Target | Effect |
|---|---|
| `make install` | fetch every dependency (`cargo fetch`) |
| `make build` | build the three programs |
| `make run-server` | start the server on port 7878 of every interface, from `tap_project/tap_server` where `data/world.yaml` and `logs/` live |
| `make run-client` | start the CLI client; `make run-client HOST=10.0.0.5 PORT=7878` for another server |
| `make run-client-gui` | start the GUI client; same `HOST` and `PORT` |
| `make test` | run every unit test |
| `make lint` | `cargo fmt --check`, then `cargo clippy` with warnings as errors |
| `make format` | format the three crates |
| `make clean` | remove build outputs and server logs |

Without `make`: `cd tap_project/tap_server && cargo run`, and
`cargo run -- [host] [port]` in `tap_client` or `tap_gui`.

**CLI client**: commands are typed exactly as the protocol spells them
(`MOVE north`, `CHAT ROOM hello`); an empty line is not sent. A reader
thread prints every server line as it arrives, so events show up while
the player is typing. When the server closes the connection (after
`QUIT`, for example), the client prints `Connection closed` and exits;
Ctrl-D closes it from the client side.

**GUI client**: enter a name and press Connect; if the server cannot be
reached, nothing is sent and Connect can be pressed again. The room
picture, the exits, the items, the NPCs and the inventory are shown with
buttons, and each item and NPC of our world has its icon; an item can
also be typed by its name or its id for `TAKE` and `DROP`. A mini-map
draws every room seen so far, from the exits of each `LOOK`. The `EVT`
lines update the window live: players in the room, player count, group,
chat. A group invitation shows Join and Ignore buttons. The health bar
and the quests come from the `STATUS` and `QUESTS` replies. The log and
the chat (global, room, group) are shown apart. After `QUIT` or a lost
connection, the window goes back to the start screen, ready to connect
again.

---

<h2 align="center" id="testing">TESTING</h2>

**Unit tests**: `make test`. The server tests cover every command module
without sockets, events with real local sockets, and random campaigns of
thousands of actions that check world invariants after each step (the
heaviest are `#[ignore]`: `cargo test --release -- --ignored` in
`tap_project/tap_server`).

**Multiplayer**: start the server, then two clients (CLI, GUI or
`nc 127.0.0.1 7878`):

| Client A | Client B sees |
|----------|---------------|
| `CONNECT alice` | `EVT ROOM PRESENCE ENTER alice`, `EVT STATS players=2` |
| `CHAT GLOBAL hi` | `EVT GLOBAL CHAT alice hi` |
| `MOVE north` | `EVT ROOM PRESENCE LEAVE alice` |
| `GROUP CREATE`, `GROUP INVITE bob` | `EVT GROUP INVITE alice`; B answers `GROUP JOIN alice` |
| close the client | `EVT GROUP LEAVE alice`, `EVT STATS players=1` |

**Combat**: from the entrance, `MOVE east`, `MOVE east`, `TAKE sword`,
`MOVE north`, then `ATTACK monster` twice: `status` goes `combat`, then
`victory`; `STATUS` shows 88 HP. A second player in the lair sees each
round as `EVT ROOM COMBAT`. Without the sword, keep attacking to see a
defeat and the respawn at the entrance with 50 HP.

**Quests**:

1. Rescue: `MOVE east`, `TAKE Cell Key`, `MOVE west`, `MOVE south`,
   `QUEST prisoner` (active), `QUEST prisoner` again (completed, full
   heal).
2. Treasure: `QUEST merchant` at the entrance (active, `QUESTS` shows
   `0/2`), beat the monster as above, `MOVE east`, `TAKE treasure`, walk
   back to the entrance, `QUEST merchant` (completed).

**Logs**: run the scenarios above while watching
`tail -f tap_project/tap_server/logs/server.jsonl | jq -c .`.

### What was measured

One review run, 2026-10-24, on a fresh clone of `main` with #76 and #78 merged:

| Check | Result |
|-------|--------|
| `make lint` | clean: `rustfmt`, `clippy` with warnings as errors |
| `make install`, `make build` | clean |
| `make test` | server 64 passed, GUI 28 passed |
| 3 `#[ignore]` campaigns, `--release` | 3 passed |
| scenarios above, 3 scripted clients | every reply and event as written here, defeat and respawn excepted: unit tests only |
| logs of that run | 200 lines, each one valid JSON for `jq` |
| another group's CLI client on our server | every command answered, its events seen by our client |
| our commands on that group's server | connected; its `LOOK`, `WHO` and `INVENTORY` replies differ from RFC 5.1.2, 5.2.2 and 5.4.3; their real `LOOK` reply, fed to our GUI parser, is rejected, while an RFC-shaped one is read: our GUI cannot draw their rooms |

Not claimed: the GUI window was checked by hand only, never on the other
group's server, and no load beyond a burst of 30 commands was tried.

---

<h2 align="center" id="known-limits">KNOWN LIMITS</h2>

- **No persistence.** A restart resets the world and every player.
- **One lock.** Right for 8 rooms and a few players; a large world would
  queue its players behind it.
- **A name is the only identity.** No password: a free name is anyone's.
- **Abuse is logged, not blocked.** A flooding client keeps being served.
- **Fixed address.** The server always listens on port 7878.

---

<h2 align="center" id="resources">RESOURCES</h2>

- RFC 42TAP, attached to the subject: the protocol of record.
- [The Rust Programming Language](https://doc.rust-lang.org/book/), chapters
  on threads and shared state.
- [std::net](https://doc.rust-lang.org/std/net/) and
  [std::sync](https://doc.rust-lang.org/std/sync/) documentation.
- [serde](https://serde.rs/), [serde_yaml](https://docs.rs/serde_yaml),
  [serde_json](https://docs.rs/serde_json).
- [egui and eframe](https://docs.rs/eframe) documentation.
- [JSON Lines](https://jsonlines.org/) format.
- Howard Hinnant, [chrono-Compatible Low-Level Date Algorithms](https://howardhinnant.github.io/date_algorithms.html),
  for the UTC timestamps of the logs.


### AI USAGE

#### By `slidriss`

<!-- OWED: slidriss add their own part below. -->


#### By `srezzaq`

I used AI to learn Rust, to understand the concepts of the
subject and to test my code.


#### By `jferone`

Self-written or agent-generated, my code carries the same design value, stress tests and the same check against my [DRAFT tools](https://github.com/sayca-jferone/DRAFT).

| Task | AI use |
|------|--------|
| Requirements | Claude extracted `subject v1.4` and `RFC 42TAP` into my `DRAFT FMBOA grid` (conditions, specifications, requirements), then challenged hard both the soundness of the subject and our group positions |
| Game engine | `Claude Hexis` (my own harness enhancement project) helped to stress-test every piece of code produced, whether written by hand or generated. It helped on: combat, quests, groups, items, world validation, WHO, CHAT and `EVT`, server logging, RFC fixes, Makefile, final README conformity check |
| Final review, 2026-10-24 | `Claude Hexis` replayed the README scenarios against `main`, found that #62 and #63 never reached it and found four defects; it wrote the PR restoring them and the four fixes of #78, under my own review |

**Human-native + AI enhancement:** Nothing an agent returned was merged before I reviewed it. Every fix came with its test and a replayed scenario.

**Attribution:** Every commit is authored by myself. No commit on GitHub carries an AI trailer, because of my strict supervision.

---

*Built at 42 by jferone, slidriss and srezzaq.*
