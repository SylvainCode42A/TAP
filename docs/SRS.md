# TAP - SRS document

- SRS for **Software Requirements Specification**

- Using [DRAFT formal method](https://github.com/sayca-jferone/DRAFT) simplified SRS-FMBOA.

| Mark | Meaning |
|---|---|
| `F` | Format: language, wire format, deliverables |
| `M` | Mandatory: the project fails without it |
| `B` | Bonus: RFC SHOULD/MAY the subject does not make mandatory. Subject v1.4 has no bonus part |
| Flag `O` | Open point: the subject leaves the choice to us. Answered in [Decisions](#decisions) |
| Flag `A` | Ambiguity: unclear or self-contradicting source. Settled in [Decisions](#decisions) |

## Sources

- Subject TAP v1.4
	> called `S p.N` : N for page number.

- RFC 42TAP
	> called `R N.N` : R numbers as printed: R reuses 8.x for errors and for resources.

## F - Formats

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | F1 | | Rust only. Python anywhere is an automatic failure | `git ls-files '*.py'` is empty | S p.6 |
| [ ] | F2 | | TCP sockets | `nc <host> <port>` connects | S p.6, R 2.1 |
| [ ] | F3 | | UTF-8 on every message, Unicode names and text | `CONNECT élodie`, `CHAT GLOBAL ça va` arrive intact | S p.6, R 9.2 |
| [ ] | F4 | A | Lines end with `\n`; a trailing `\r` from a client is stripped | server lines end with `\n`; `LOOK\r\n` works | S p.6, R 2, 3.1 |
| [ ] | F5 | | Reply: `OK [data]` or `ERR <3 digits> <TOKEN>` | no other reply shape on the wire (`OK:` is wrong) | R 4.1, 8.1 |
| [ ] | F6 | A | Event: `EVT <category> <type> <data>`, as in the R 7.1 tables | every push starts with `EVT ` and matches a R 7.1 row or F19 | R 4.1, 7.1 |
| [ ] | F7 | A | Keywords case-insensitive: commands, sub-commands, scopes. Free arguments keep their case | `chat global Hi` = `CHAT GLOBAL Hi` | R 4.2 |
| [ ] | F8 | A | Error codes from R 8.2, plus the additions listed in F19 | every emitted code and token is in R 8.2 or F19 | S p.6, R 8.2 |
| [ ] | F9 | A | IDs `room.<key>`, `item.<key>`, `npc.<key>` | LOOK, MOVE, TAKE replies use these prefixes | R 5.1.2, 5.1.3, 5.4.1 |
| [ ] | F10 | | World data in YAML or JSON, loaded at start | server reads the world file at boot | S p.5, 7 |
| [ ] | F11 | | Rust standard lint and type annotations | `cargo fmt --check` and `cargo clippy` report nothing | S p.6 |
| [ ] | F12 | O | Makefile over one cargo workspace (server, CLI, GUI), targets: install, compile, run-server, run-client, run-client-gui, lint, clean | the 7 `make` targets work from a fresh clone | S p.5, 15 |
| [ ] | F13 | | Repo holds server, CLI client, GUI client, world data, build tool, README | all 6 present at submission | S p.15 |
| [ ] | F14 | | `README.md` at repo root, in English | | S p.13, 14 |
| [ ] | F15 | | README line 1, italic, exact: `*This project has been created as part of the 42 curriculum by <login1>, <login2>, <login3>.*` | line 1 matches, 3 logins | S p.13 |
| [ ] | F16 | | README: Description, Instructions | present, not empty | S p.13 |
| [ ] | F17 | | README: Resources, plus AI use per task and per part | each part says if and how AI was used | S p.13 |
| [ ] | F18 | | README: Architecture (dispatcher or inline, concurrency model) | matches the code | S p.13 |
| [ ] | F19 | | README: Protocol Implementation, every deviation from R justified | lists: DEFEND, FLEE, USE (M40); `EVT ROOM COMBAT` (M42); `400 UNKNOWN_COMMAND`, `400 BAD_ARGUMENTS` (F8); `400 NOT_CONNECTED`, `400 ALREADY_CONNECTED` (M4); `400 NOT_IN_COMBAT` (M40); `405 ITEM_NOT_OBTAINABLE` (M27); `301 EXIT_BLOCKED` (M39); `401 NOT_LEADER` (M51); `404 PLAYER_NOT_FOUND` (M54); R 4.1 ABNF vs R 7.1 (F6) | S p.6, 13 |
| [ ] | F20 | | README: Combat System, every R 6.1.1 aspect decided | matches the code and M39 to M42 | S p.8, 13, R 6.1.1 |
| [ ] | F21 | | README: Quest System, every R 6.1.2 aspect decided | matches the code and M45 | S p.7, 13, R 6.1.2 |
| [ ] | F22 | | README: World Design (layout, connections, NPC roles, items) | matches the world file | S p.14 |
| [ ] | F23 | | README: Server Logging (format, event types, outputs, abuse monitoring) | matches the code | S p.14 |
| [ ] | F24 | | README: Group Contributions | each of the 3 members and their parts named | S p.14, 15 |
| [ ] | F25 | | README: Building and Running (build tool, server, CLI, GUI) | a newcomer runs all 3 from it alone | S p.5, 14 |
| [ ] | F26 | | README: Testing (multiplayer, combat, quests) | each has a reproducible procedure | S p.14 |
| [ ] | F27 | O | Logs in JSON Lines: one object per line, `ts`, `level`, `event`, then event fields | every log line parses with `jq` | S p.8 |
| [ ] | F28 | | Log levels INFO, WARN, ERROR | every line carries one | S p.8 |
| [ ] | F29 | A | RFC 3339 UTC timestamp with milliseconds; INFO to stdout, WARN and ERROR to stderr | `2>/dev/null` leaves only INFO lines | S p.8 |

</details>

## M - Mandatory

### Connection

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | M1 | | Greeting `OK hello proto=1` on TCP accept | first line `nc` shows | R 3.2 |
| [ ] | M2 | A | `CONNECT <username>` -> `OK connected`. Username: 1 to 16 chars, one word, Unicode letters and digits, `_`, `-`; else `ERR 400 BAD_ARGUMENTS` | `CONNECT élodie` -> OK; `CONNECT a b` -> 400 | R 3.1, 3.3, 5.1.1 |
| [ ] | M3 | A | Name unique among active players, else `ERR 201 NAME_IN_USE` | 2nd `CONNECT alice` or `CONNECT Alice` -> 201; free again once alice left | R 3.3 |
| [ ] | M4 | A | Before CONNECT: `ERR 400 NOT_CONNECTED`. Second CONNECT: `ERR 400 ALREADY_CONNECTED` | `LOOK` first -> 400 NOT_CONNECTED | R 2.2 |
| [ ] | M5 | | Partial line buffered until `\n` | send `LOO`, then `K\n` -> one LOOK reply | R 9.2 |
| [ ] | M6 | | Several lines in one packet, each handled | `LOOK\nWHO\n` in one send -> 2 replies | R 9.2 |
| [ ] | M7 | A | Unknown command -> `ERR 400 UNKNOWN_COMMAND`; bad arguments -> `ERR 400 BAD_ARGUMENTS`; session kept. Empty line ignored | `FOO` -> 400 UNKNOWN_COMMAND; `MOVE` -> 400 BAD_ARGUMENTS; empty line -> no reply | S p.7, R 9.3 |
| [ ] | M8 | | `QUIT` -> `OK bye`, then close | | R 5.1.4 |
| [ ] | M9 | | Disconnect (QUIT or abrupt): player state removed BEFORE leave events go out | kill a client: others get LEAVE and no longer see the name | S p.6, R 3.3 |
| [ ] | M10 | | Broadcast still reaches everyone when a client drops mid-send | kill a client during a chat burst: others miss nothing | S p.7 |
| [ ] | M11 | | Several players at once, real time | 3 clients play together | S p.4 |
| [ ] | M12 | | `EVT STATS players=<count>` when the server count changes | connect or quit -> others receive it | R 6.2.4 |
| [ ] | M13 | A | Clients show `900 CONNECTION_FAILED` and `901 SEND_FAILED`, then retry; the server logs both tokens at ERROR | server down -> client shows 900 | R 7.3, 8.2 |

</details>

### World

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | M14 | | Every exit and reference checked at load; broken world refused | exit to an unknown room -> server will not start, says why | S p.7 |
| [ ] | M15 | | 8 rooms or more, all connected | all reachable from start | S p.7 |
| [ ] | M16 | | At least one loop | | S p.7 |
| [ ] | M17 | | At least one optional branch | | S p.7 |
| [ ] | M18 | | Full circuit possible, map not line-only | walk a cycle back to start | S p.7 |
| [ ] | M19 | O | 3 distinct NPC roles: merchant = quest-giver, prisoner = dialogue (rescue target), monster = enemy | | S p.7 |
| [ ] | M20 | | 4 distinct items or more, 2 obtainable or more | | S p.7 |
| [ ] | M21 | | 2 quests or more | | S p.7 |

</details>

### Room and movement

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | M22 | | `LOOK` -> `OK <json>`: `room{id,name,description,exits}`, `players`, `items`, `npcs` | reply parses as JSON with these keys | R 5.1.2 |
| [ ] | M23 | O | `MOVE <direction>`, direction in north, south, east, west -> `OK room=<room-id>`; else `ERR 301 NO_EXIT` | `MOVE up` -> 301 | R 5.1.3 |
| [ ] | M24 | A | `EVT ROOM PRESENCE LEAVE <username>` to the old room and `ENTER` to the new one on MOVE; ENTER in the start room on CONNECT; LEAVE on QUIT or drop | a 2nd client in each room sees all four cases | S p.11, R 6.2.1 |

</details>

### Items

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | M25 | | Each item is one instance: in one room or one inventory | 2 players TAKE it: the 2nd gets 404 | S p.7, R 8.1 |
| [ ] | M26 | | `TAKE <item>` -> `OK taken=<item-id>`, item leaves the room; else `ERR 404 ITEM_NOT_FOUND` | LOOK no longer lists it | S p.7, R 5.4.1 |
| [ ] | M27 | A | `TAKE` on a non-obtainable item -> `ERR 405 ITEM_NOT_OBTAINABLE` | `TAKE torch` -> 405, torch still listed by LOOK | S p.7, R 5.4.1 |
| [ ] | M28 | | `DROP <item>` -> `OK dropped=<item-id>`, item in the room for others; else `ERR 404 ITEM_NOT_IN_INVENTORY` | another player can TAKE it | S p.7, R 5.4.2 |
| [ ] | M29 | A | Item found by full ID, bare key or display name, case-insensitive | `take ITEM.CELL_KEY`, `take cell_key`, `take cell key` all work | S p.7, R 8.3 |
| [ ] | M30 | | Multi-word item names | `DROP Cell Key` works | S p.7, R 8.4 |
| [ ] | M31 | | `INVENTORY` -> `OK [<item-ids>]`, a JSON array | | R 5.4.3 |

</details>

### NPCs and combat

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | M32 | A | `TALK <npc-name>` -> `OK <dialogue>`, plain text; else `ERR 404 NPC_NOT_FOUND`. NPC found like items (M29) | reply is not JSON | R 5.4.4, S p.12 |
| [ ] | M33 | O | Dialogue line chosen by the asking player's quest state when it matters, otherwise rotation per player | prisoner: 'Free me...' before, 'Thank you' after | S p.9 |
| [ ] | M34 | A | `ATTACK <npc-name>` -> `OK {"attacker_hp","target_hp","damage","status"}`, status `combat`, `victory` or `defeated`; `ERR 404 NPC_NOT_FOUND`; `ERR 405 NPC_NOT_HOSTILE` | ATTACK merchant -> 405 | R 5.4.5 |
| [ ] | M35 | A | `STATUS` -> `OK {"hp","max_hp","status"}`, status `healthy` out of combat, `combat` in combat | | S p.8, R 5.4.6 |
| [ ] | M36 | | Players start at 100 HP | STATUS right after CONNECT -> `"hp": 100` | S p.8 |
| [ ] | M37 | | Enemy HP depends on NPC type | read from the world file | S p.8 |
| [ ] | M38 | | ATTACK deals damage, may trigger a counter-attack | target HP drops, player HP can drop | S p.8 |
| [ ] | M39 | O | 1 command = 1 exchange: player first, monster counters if alive. Fixed damage from yaml: fists 5, sword 10, monster 8. Combat from ATTACK to a death or FLEE. Shared monster HP, full again when its room is empty; while alive it blocks `east`: `ERR 301 EXIT_BLOCKED`. Monster and treasure respawn 60 s after the treasure is taken. No status effects | sword: 2 ATTACK kill the monster, player loses 8 HP; `MOVE east` before -> 301 EXIT_BLOCKED | S p.8, R 6.1.1 |
| [ ] | M40 | O | `DEFEND`: next counter halved. `FLEE`: back to the previous room, combat ends. Both out of combat -> `ERR 400 NOT_IN_COMBAT`. `USE <item>`: consumes it; potion +30 HP up to max_hp, in or out of combat, takes the combat turn | each one works and is in the README | S p.8, R 6.1.1 |
| [ ] | M41 | O | 0 HP -> respawn in `room.entrance` with 50 HP, inventory kept | die once: STATUS shows 50 HP in the entrance | S p.8 |
| [ ] | M42 | A | Combat results logged and sent to the room: `EVT ROOM COMBAT <attacker> <target> <damage> <target_hp>` | a 2nd client in the room sees each hit and counter | S p.8 |

</details>

### Quests

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | M43 | A | `QUEST <npc-name>` -> `OK {"quest_id","description","reward","status"}`; `ERR 404 NPC_NOT_FOUND`; `ERR 406 NO_QUEST_AVAILABLE` | QUEST on a finished quest -> 406 | R 5.4.7 |
| [ ] | M44 | A | `QUESTS` -> `OK [{"quest_id","status","progress"}]`, active and completed |  | R 5.4.8 |
| [ ] | M45 | O | Independent quests from the merchant, progress per player, automatic completion, objectives in any order. free_prisoner (reward: potion), treasure_quest (reward: the treasure). Defeating the monster counts for every player who hit it since its last respawn | both quests completable end to end by 2 players in turn | S p.7, R 6.1.2 |

</details>

### Chat and groups

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | M46 | | `CHAT GLOBAL <message>` -> `OK`; everyone, sender included, gets `EVT GLOBAL CHAT <username> <message>` | | S p.11, R 5.2.1, 6.2.2 |
| [ ] | M47 | | `CHAT ROOM <message>` -> `OK`; room, sender included, gets `EVT ROOM CHAT <username> <message>` | another room gets nothing | S p.11, R 5.2.1, 6.2.1 |
| [ ] | M48 | | `CHAT GROUP <message>` -> `OK`; group, sender included, gets `EVT GROUP CHAT <username> <message>`; no group -> `ERR 401 NOT_IN_GROUP` | | S p.11, R 5.2.1, 6.2.3 |
| [ ] | M49 | A | `WHO` -> `OK players=<count>` | 2 clients -> `OK players=2` | R 5.2.2, S p.12 |
| [ ] | M50 | | `GROUP CREATE` -> `OK group=<group-id>`; already in one -> `ERR 402 ALREADY_IN_GROUP` | | R 5.3.1 |
| [ ] | M51 | A | `GROUP INVITE <username>` -> `OK`; target gets `EVT GROUP INVITE <leader>`; no group -> 401; not leader -> `ERR 401 NOT_LEADER` | | R 5.3.2, 6.2.3 |
| [ ] | M52 | A | `GROUP JOIN <leader-name>` -> `OK group=<group-id>` (`group.<n>`), only with a pending invite; members get `EVT GROUP JOIN <username>` | JOIN without invite -> refused | R 5.3.3, 6.2.3 |
| [ ] | M53 | | `GROUP LEAVE` -> `OK`; members get `EVT GROUP LEAVE <username>`; no group -> 401 | | R 5.3.4, 6.2.3 |
| [ ] | M54 | O | Only the leader invites; leader leaves -> oldest member leads, id unchanged; last leave deletes the group; invite to an absent player -> `ERR 404 PLAYER_NOT_FOUND` | README states the rules | R 5.3 |

</details>

### Logging

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | M55 | | Connections and disconnections, with timestamp and IP | | S p.8 |
| [ ] | M56 | | Every command received, with player name and parameters | | S p.8 |
| [ ] | M57 | | Every reply and error code sent | | S p.8 |
| [ ] | M58 | | World changes: item moves, NPC interactions, combat results | | S p.8 |
| [ ] | M59 | | Quest progress and completions | | S p.8 |
| [ ] | M60 | O | Abuse logged at WARN: more than 20 commands/s from a client, more than 5 connections in 10 s from one IP | flood script -> WARN line | S p.8 |
| [ ] | M61 | | Logging does not slow the server noticeably | no visible lag under a flood | S p.8 |

</details>

### CLI client

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | M62 | | Connects, prints incoming messages in real time | | S p.11 |
| [ ] | M63 | | Keeps receiving events while waiting for input | chat from another client shows while the prompt is idle | S p.6, 11 |
| [ ] | M64 | O | CLI accepts raw RFC, plus shortcuts: `n` `s` `e` `w` -> MOVE, `l` -> LOOK, `i` -> INVENTORY, `say` -> CHAT ROOM | `n` moves north; `MOVE north` too | S p.11 |

</details>

### GUI client

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | M65 | | Real graphical toolkit; curses does not count | | S p.11 |
| [ ] | M66 | | Room details, items, NPCs, exits, updated in real time | another player's DROP shows without a click | S p.11 |
| [ ] | M67 | | Inventory shown, TAKE and DROP buttons | | S p.11 |
| [ ] | M68 | | Accepts item IDs and display names | | S p.11 |
| [ ] | M69 | | Room view refreshes itself after TAKE or DROP | | S p.11 |
| [ ] | M70 | | Chat view (Global, Room, Group) separate from log view | | S p.11 |
| [ ] | M71 | | Buttons: LOOK, MOVE, TAKE, DROP, TALK, ATTACK, STATUS, QUEST, QUESTS, WHO, GROUP, QUIT, plus DEFEND, FLEE, USE (M40) | all 15 present | S p.11 |
| [ ] | M72 | | Counters: players in the room (LOOK `players`, PRESENCE events), players on the server (WHO, `EVT STATS`) | | S p.11 |
| [ ] | M73 | | TALK dialogue displayed | | S p.11 |
| [ ] | M74 | | Stays responsive while receiving events | no freeze under a chat burst | S p.6 |

</details>

### Interop and team

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | M75 | | Our CLI and GUI work with another group's server, and theirs with ours | tested against one other group | S p.4 |
| [ ] | M76 | | Every member contributes meaningfully and has commits | `git shortlog -sn` lists all 3 | S p.15 |
| [ ] | M77 | | Proper commit messages and branch management | | S p.15 |
| [ ] | M78 | | Each member explains and modifies their own part live, AI-written code included | | S p.2, 3, 15 |

</details>

## B - Bonus

<details><summary>Checklist</summary>

| Check | ID | Flag | Requirement | Done when | Src |
|---|---|---|---|---|---|
| [ ] | B1 | | Line length limit, 1024 bytes recommended | 2 KB line -> `ERR`, client still connected | R 9.4 |
| [ ] | B2 | | Connection count limit | | R 9.4 |
| [ ] | B3 | | Per-player item acquisition limit | | R 9.4 |
| [ ] | B4 | | Chat rate limit (M60 only logs abuse) | | R 9.4 |
| [ ] | B5 | | Control characters rejected or neutralised | `CHAT GLOBAL` with `ESC[2J` does not clear other terminals | R 9.2 |
| [ ] | B6 | | Clients recover from errors: 4xx continue, 9xx reconnect | | R 7.3 |
| [ ] | B7 | | Authentication stronger than a unique name | | R 9.1 |

</details>

## Decisions

One row per `O` or `A` flag. A decision that departs from R also goes to F19.

<details><summary>Table</summary>

| ID | Flag | Question | Decision | Why |
|---|---|---|---|---|
| F4 | A | LF only, or CRLF accepted too? R 2 says LF, R 3.1 ABNF says CRLF | accept `\n` and `\r\n` (trailing `\r` stripped); send `\n` only | tolerant input, strict output: telnet and R 3.1 ABNF clients work, R 2 respected on the wire |
| F6 | A | R 7.1 `EVT <category> <type> <data>` vs R 4.1 ABNF `event-type = 1*ALPHA`, one word | R 7.1 tables: `EVT <category> <type> <data>`, multi-word type; R 4.1 ABNF noted in F19 as an RFC inconsistency | the tables define every event and S p.11 uses them; the ABNF is too narrow for them |
| F7 | A | Sub-commands and scopes (`CREATE`, `GLOBAL`) case-insensitive too? R 4.2 names commands only | keywords case-insensitive (commands, sub-commands, scopes); free arguments keep their case | same logic as R 4.2 and M29 |
| F8 | A | Code for unknown command and bad syntax? R 8.2 has none | `ERR 400 UNKNOWN_COMMAND`, `ERR 400 BAD_ARGUMENTS`; listed in F19 | R pattern: one code, several tokens (like 404); 4xx is non-fatal for any client (R 7.3) |
| F9 | A | Room prefix `room.` (R 5.1.2 structure) or `loc.` (R and S examples)? `item.`, `npc.`? | `room.<key>`, `item.<key>`, `npc.<key>` | R 5.1.2 normative LOOK structure uses `room.`; examples (`loc.`) are not normative |
| F12 | O | Makefile over cargo, cargo aliases, or other? | Makefile calling cargo; one cargo workspace for server, CLI, GUI crates | `make` is what a 42 evaluator types first and is already installed; one workspace, one lock, one `target/` |
| F27 | O | JSON or another structured format? | JSON Lines: one object per line (`ts`, `level`, `event`, then event fields) | format the subject recommends; `jq`-friendly |
| F29 | A | Timestamp precision? Which stream: stdout, stderr, file? | RFC 3339 UTC, milliseconds (`2026-10-16T18:42:07.123Z`); INFO to stdout, WARN and ERROR to stderr | standard streams split with `2>`, no file or rotation to manage |
| M2 | A | Username charset and length? R 3.1 leaves `username` undefined | 1 to 16 chars, one word: Unicode letters and digits, `_`, `-`; else `ERR 400 BAD_ARGUMENTS` | a space would break `EVT ... <username> <message>` parsing; R 9.2 requires Unicode names |
| M3 | A | Is uniqueness case-sensitive? | case-insensitive: `Alice` = `alice` for `ERR 201`; name shown as typed | two players must stay distinguishable |
| M4 | A | Code for a command before CONNECT? Second CONNECT on the same session? | `ERR 400 NOT_CONNECTED` before CONNECT, `ERR 400 ALREADY_CONNECTED` on a second CONNECT; listed in F19 | no new code, tokens in the 400 family set by F8 |
| M7 | A | Empty line: error or ignored? | empty line ignored, no reply; still counted by the anti-flood (M60) | not a command per R 4.1 ABNF (`command-name = 1*ALPHA`); pressing Enter in nc or telnet stays harmless |
| M13 | A | Who emits 900 and 901, and when? Server, or client-side only? | clients show 900 and 901 and retry (B6); server reuses both tokens in its ERROR logs | nothing can be sent on a broken connection; one vocabulary for both sides |
| M19 | O | Two quest-givers (M45) leave 2 distinct roles out of 3: how to keep 3? | merchant = quest-giver, prisoner = dialogue (rescue target), monster = enemy | smallest change: no new NPC, no trading system |
| M23 | O | Valid directions: 4 cardinals, plus up and down? | `north`, `south`, `east`, `west` only. World: `monster_room` `south_alt` -> `west: armory` | only directions the world uses; a new one is one enum variant + yaml when needed |
| M24 | A | PRESENCE events on CONNECT and on disconnect too? | ENTER in the start room on CONNECT; LEAVE in the last room on QUIT or drop | S p.6 already requires leave events on disconnect (M9); keeps GUI counters right (M72) |
| M27 | A | Code for TAKE on an item that cannot be taken? R has none | `ERR 405 ITEM_NOT_OBTAINABLE`; listed in F19 | R pattern: code = category, token = detail; 405 = exists but action refused, like `NPC_NOT_HOSTILE`. 4xx stays non-fatal for other clients (R 7.3) |
| M29 | A | Display-name match case-sensitive? | case-insensitive; full ID, bare key or display name (`item.cell_key`, `cell_key`, `Cell Key`) | items are unique, so no collision; consistent with F7 |
| M32 | A | TALK reply: plain text (R 5.4.4) or JSON (S p.12)? | `OK <dialogue>`, plain text. NPCs found like items: full ID, bare key or display name, case-insensitive | rule: R normative syntax wins over any example (S or R); S p.6 requires strict RFC compliance; no deviation |
| M33 | O | Several dialogue lines: rotate, random, or first? | line chosen by the asking player's quest state when it matters (prisoner: before / after freed), otherwise rotation per player | deterministic; the prisoner's yaml dialogue is already written as before / after |
| M34 | A | ATTACK keys come from the R example only: adopt them as contract? | `{"attacker_hp","target_hp","damage","status"}` as in R example; status `combat`, `victory` (target dead), `defeated` (attacker dead, respawned) | R gives no normative form; other groups will copy the example (M75) |
| M35 | A | STATUS `status` values? | `healthy` out of combat, `combat` in combat | from R examples; same vocabulary as ATTACK (M34) |
| M39 | O | Turns and initiative, damage formula, combat start and end, status effects? | 1 command = 1 exchange: player acts first, monster counters if alive. Fixed damage from yaml `stats.attack`: fists 5, sword 10, monster 8; DEFEND halves the next counter. Combat starts on ATTACK, ends on a death or FLEE. Monster HP shared, back to full when its room is empty. While alive, the monster blocks `east`: `ERR 301 EXIT_BLOCKED`, listed in F19. Monster and treasure respawn 60 s after the treasure is taken. Status effects: none | deterministic, testable without seed, explainable live; tuning without recompiling; every player can finish the treasure quest while the treasure stays unique (M25); the guard keeps the game goal (no treasure without a fight) |
| M40 | O | Which extra combat commands? | `DEFEND` (next counter halved), `FLEE` (back to the previous room, combat ends), `USE <item>` (consumes it; potion +30 HP up to max_hp, in or out of combat, takes the combat turn); DEFEND or FLEE out of combat -> `ERR 400 NOT_IN_COMBAT`; listed in F19. World: needs a consumable item (potion) | DEFEND and FLEE are the subject's own examples; USE gives items a combat role |
| M41 | O | Respawn room and HP? | `room.entrance`, 50 HP, inventory kept | start room, no enemy; half HP is a clear cost without a dead end |
| M42 | A | Who are the "relevant players" for combat broadcast? | players in the same room, via new `EVT ROOM COMBAT <attacker> <target> <damage> <target_hp>`, for hits and counters; listed in F19 | same scope as CHAT ROOM and PRESENCE; R defines no combat event; unknown EVT is ignored by other clients |
| M43 | A | QUEST keys come from the R example only: adopt them as contract? | `{"quest_id","description","reward","status"}` as in R example | R gives no normative form; other groups will copy the example (M75) |
| M44 | A | QUESTS keys come from the R example only: adopt them as contract? | `[{"quest_id","status","progress"}]` as in R example | R gives no normative form; other groups will copy the example (M75) |
| M45 | O | Progression, completion auto or manual, rewards, dependencies, extra commands? Givers? Multiplayer credit? | Independent quests, both from `QUEST merchant` (each call gives the next quest not yet taken; 406 when none left): free_prisoner (reward: potion), treasure_quest (reward: the treasure). Progress per player, tracked from QUEST onward; objectives already met count; any order; completes on the last objective. Freeing = TALK prisoner with `cell_key` in inventory; key kept; NPC does not move. Defeating the monster counts for every player who hit it since its last respawn. Dependencies: none. Extra commands: none | secret cell stays optional (M17); one giver keeps 3 distinct NPC roles (M19); per-player state avoids conflicts; unique key (M25) stays usable by everyone; shared kill credit rewards cooperation |
| M49 | A | WHO: `OK players=<count>` (R 5.2.2) or `OK {"room":[...],"server":<n>}` (S p.12)? | `OK players=<count>`. GUI room counter (M72) from LOOK `players` | rule: R normative syntax wins over any example (S or R); server counter also from `EVT STATS` |
| M51 | A | Who may invite, and the code otherwise? | leader only (M54); other member -> `ERR 401 NOT_LEADER`, listed in F19 | 401 family = group rights |
| M52 | A | JOIN takes a leader name but answers a group id: which one identifies a group? | opaque id `group.<n>` from GROUP CREATE; GROUP JOIN takes the leader's name, answers the group id | literal reading of R 5.3; no deviation |
| M54 | O | Who may invite? Leader leaves? Invite to an unknown player? | only the leader invites; JOIN needs a pending invite; leader leaves -> oldest member leads, id unchanged; last leave deletes the group; invite to an absent player -> `ERR 404 PLAYER_NOT_FOUND`, listed in F19 | gives INVITE a purpose; group id stays stable (M52) |
| M60 | O | Flood and rapid-reconnect thresholds? | WARN when a client sends more than 20 commands/s, or one IP opens more than 5 connections in 10 s; log only, constants in code | no human reaches it, a script does; blocking is B4 |
| M64 | O | CLI input: raw RFC or friendly commands? | raw RFC accepted, plus shortcuts translated to RFC: `n` `s` `e` `w` -> MOVE, `l` -> LOOK, `i` -> INVENTORY, `say` -> CHAT ROOM | comfortable play while the CLI stays a protocol test tool |

</details>
