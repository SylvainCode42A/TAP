# Fiche de correction — serveur TAP (Rust)

## 0. Lancement

```
make run-server   →   cd tap_project/tap_server && cargo run
```
Le serveur doit démarrer depuis `tap_server/` car il lit `data/world.yaml` en chemin relatif et écrit `logs/server.jsonl`.

---

## 1. Le chemin complet (vue d'ensemble)

```
                 main.rs  fn main()  (l.239)
                        │
   ① charger le monde   │  World::load_world()            l.233  → lit data/world.yaml (serde_yaml)
   ② vérifier le monde  │  world_check::validate()        l.243  → erreurs ? exit(1)
   ③ état du jeu        │  GameState { world, players, npc_hp, groups, senders }   l.255
   ④ logs               │  logging::init()                l.263  → thread écrivain (channel mpsc)
   ⑤ partage            │  Arc::new(Mutex::new(game_state)) l.278
   ⑥ CRÉER LE SERVEUR   │  TcpListener::bind("0.0.0.0:7878") l.280
                        ▼
   ⑦ boucle d'accept    for stream in listener.incoming()   l.285
                        │   (anti-reconnexion rapide : RateWatch par IP)
                        │
                        ├── client A ──► thread::spawn → parsing::handle_()
                        ├── client B ──► thread::spawn → parsing::handle_()
                        └── client C ──► thread::spawn → parsing::handle_()
```

```
            parsing.rs  handle_()   (1 thread = 1 client)
                        │
                        │  stream.try_clone() → BufReader (un seul pour toute la session, bug #36)
                        │  écrit  "OK hello proto=1\n"
                        ▼
              ┌──── loop read_line ─────────────────────────────┐
              │  Ok(0)        → client parti (EOF) → break       │
              │  > 1024 octets→ ERR 401 LINE_TOO_LONG            │
              │  sinon        → split_whitespace, mot-clé en     │
              │                 majuscule → enum Command         │
              │                 mauvais args → ERR 401           │
              │                                                  │
              │  dispatch::dispatch(cmd, &game_state, &user)     │
              │        │                                         │
              │        ▼ Response                                │
              │  Ok(body)  → "OK body\n"                         │
              │     si CONNECT : current_username = nom          │
              │                  + clone du stream dans          │
              │                  gs.senders (pour les EVT)       │
              │  Err{code,msg} → "ERR code msg\n"                │
              │  QUIT → break                                    │
              └──────────────────────────────────────────────────┘
                        │
                        ▼
              events::disconnect()  → quitte groupe + salle, retire
                                      senders/players, EVT STATS
```

```
            dispatch.rs  run()  — un `match` par Command
                        │
                        │  1. current_username == None ? → ERR 404 "Try with first CONNECTED"
                        │  2. game_state.lock()  (verrou tenu toute la commande)
                        │  3. appelle le bon module :
                        │
        LOOK / MOVE / INVENTORY / TALK / WHO / CONNECT → traités directement ici
        TAKE / DROP          → items.rs
        ATTACK / STATUS      → combat.rs
        QUEST / QUESTS       → quest.rs
        GROUP …              → group.rs
        CHAT …               → events.rs
                        │
                        ▼  après le changement d'état → events.rs envoie les "EVT …"
                           aux autres joueurs via gs.senders
```

---

## 2. Exemple concret : `CONNECT alice` puis `MOVE north`

| Étape | Fichier | Ce qui se passe |
|---|---|---|
| client se connecte au port 7878 | main.rs l.285 | `incoming()` rend un `TcpStream`, `thread::spawn` |
| | parsing.rs | serveur envoie `OK hello proto=1` |
| client envoie `CONNECT alice\n` | parsing.rs | `actions = ["CONNECT","alice"]` → `Command::Connect{username:"alice"}` |
| | dispatch.rs l.41 | lock, nom déjà pris (insensible casse) ? sinon `Player::new("alice")` en `entrance`, 100 hp |
| | events.rs | `EVT ROOM PRESENCE ENTER alice` aux autres dans `entrance`, `EVT STATS players=N` à tous |
| | parsing.rs | `current_username = Some("alice")`, stream cloné dans `gs.senders` → réponse `OK connected` |
| client envoie `MOVE north` | dispatch.rs l.117 | sortie `north` de `entrance` = `corridor` → `current_room` remplacé |
| | events.rs `moved` | `LEAVE alice` dans entrance, `ENTER alice` dans corridor |
| | parsing.rs | réponse `OK room=corridor` |

---

## 3. Rôle de chaque fichier (`tap_server/src/`)

| Fichier | Rôle en une phrase |
|---|---|
| **main.rs** | Point d'entrée + types partagés (`World`, `GameState`, `Player`, `Command`, `Response`), crée le serveur TCP et un thread par client. |
| **parsing.rs** | Lit les lignes d'un client, les transforme en `Command`, écrit `OK`/`ERR`, gère la déconnexion. |
| **dispatch.rs** | Aiguillage : pour chaque `Command`, vérifie le CONNECT, prend le verrou, appelle le bon module. |
| **events.rs** | Messages asynchrones `EVT …` (présence, stats, chat, groupe, combat) + nettoyage d'un joueur qui part. |
| **items.rs** | TAKE / DROP — un objet se nomme par id ou par nom affiché. |
| **combat.rs** | ATTACK / STATUS : 5 dégâts (+10 avec l'épée), riposte du PNJ, mort → respawn à l'entrée avec 50 hp. |
| **quest.rs** | QUEST / QUESTS : prendre, suivre, rendre une quête (objet requis / PNJ tué), récompense = soin complet. |
| **group.rs** | GROUP CREATE / INVITE / JOIN / LEAVE, id `grp.<fondateur>`. |
| **logging.rs** | Logs JSON (1 objet/ligne) console + `logs/server.jsonl`, via un thread dédié ; détection flood/reconnexions. |
| **world_check.rs** | Vérifie `world.yaml` au démarrage (sorties valides, références, minimums du sujet : 8 salles, 4 objets…). |
| **game_stress.rs** | Tests uniquement (`#[cfg(test)]`) : campagnes aléatoires qui vérifient que rien ne panique. |
| **data/world.yaml** | La carte : salles, objets, PNJ, quêtes. |

---

## 4. Par où j'ai commencé (ordre logique à raconter)

1. **Le socket** : `TcpListener::bind` + `incoming()` — juste accepter et répondre `OK hello`.
2. **Un thread par client** : `thread::spawn` pour gérer plusieurs joueurs en même temps.
3. **Charger le monde** : structs `serde` qui collent au YAML, `load_world()`.
4. **L'état partagé** : `GameState` dans `Arc<Mutex<…>>`.
5. **Le parsing** : ligne → `enum Command` (CONNECT, LOOK, MOVE en premier).
6. **Le dispatch** : `match` sur `Command` → `enum Response` → `OK` / `ERR`.
7. **Les modules de jeu** : items, combat, quêtes, groupes.
8. **Les événements** : garder un clone du stream (`senders`) pour pousser des `EVT`.
9. **Robustesse** : `world_check`, logs, limite 1024 octets, tests.

> Adapte cet ordre à ce que tu as vraiment fait — le correcteur veut ton histoire.

---

## 5. Questions probables du correcteur

**Pourquoi `Arc<Mutex<GameState>>` ?**
`Arc` = plusieurs threads possèdent le même état (compteur de références atomique). `Mutex` = un seul thread le modifie à la fois. Une commande garde le verrou du début à la fin → les commandes s'appliquent une par une, pas de données corrompues.

**Pourquoi `0.0.0.0` et pas `127.0.0.1` ?**
`0.0.0.0` écoute sur toutes les interfaces : les clients d'autres machines (autres groupes) peuvent se connecter. `127.0.0.1` = seulement en local.

**Pourquoi `try_clone()` sur le stream ?**
Une copie sert à lire (dans le `BufReader`), l'autre à écrire. Une autre copie va dans `gs.senders` pour que d'autres threads puissent envoyer des `EVT` à ce joueur.

**Pourquoi un seul `BufReader` pour toute la session ?**
Si on le recrée à chaque ligne, les octets déjà bufferisés (deux commandes dans le même paquet TCP) sont perdus (issue #36).

**Si un client crash / coupe sa connexion ?**
`read_line` renvoie `Ok(0)` (EOF) ou `ConnectionReset` → on sort de la boucle → `events::disconnect` retire le joueur proprement. Le serveur continue.

**Si un thread panique ?**
Le `Mutex` serait « empoisonné » pour tous. D'où la règle : jamais de `unwrap` risqué dans les commandes, on renvoie `ERR 404` à la place (ex. PNJ absent du YAML dans `combat.rs`).

**Comment les autres joueurs voient ce que je fais ?**
`events.rs` parcourt `gs.senders` et écrit `EVT …\n` sur leurs sockets, pendant que le verrou est tenu → deux événements ne se mélangent jamais.

**Que se passe-t-il si `world.yaml` est cassé ?**
`world_check::validate` liste toutes les erreurs et le serveur refuse de démarrer (`exit(1)`).

**Format des réponses ?**
Une ligne terminée par `\n` : `OK <données>`, `OK` seul si vide, ou `ERR <code> <message>`. Événements : `EVT …`.
