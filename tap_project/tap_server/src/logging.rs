//! Server logging, subject V.1: one JSON object per line, on the console
//! and appended to `logs/server.jsonl`.
//!
//! Every line starts with `ts` (UTC, milliseconds), `level` (`INFO`,
//! `WARN` or `ERROR`) and `event`, then the fields of that event:
//!
//! ```text
//! {"ts":"2026-10-22T14:23:20.512Z","level":"INFO","event":"item_taken","arg":"Cell Key","player":"alice","result":"taken=cell_key"}
//! ```
//!
//! Lines go through a channel to one writer thread: a slow console or
//! disk never holds a client thread or the `GameState` lock. Before
//! [`init`], as in tests, logging does nothing.
//!
//! - Owns: the line format, the writer thread, which command makes which
//!   world event, the abuse counters.
//! - Does NOT own: deciding what happened; callers report it.

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::Command;

/// Log file, relative to the directory the server runs in.
pub const LOG_FILE: &str = "logs/server.jsonl";

/// More commands than this on one connection within [`FLOOD_WINDOW`] is
/// logged as command flooding.
pub const FLOOD_LIMIT: usize = 20;
pub const FLOOD_WINDOW: Duration = Duration::from_secs(1);

/// More connections than this from one IP within [`RAPID_WINDOW`] is
/// logged as rapid reconnection.
pub const RAPID_LIMIT: usize = 10;
pub const RAPID_WINDOW: Duration = Duration::from_secs(10);

/// Writer thread input, set once by [`init`].
static SINK: OnceLock<Sender<String>> = OnceLock::new();

// MARK: - Output

#[derive(Clone, Copy)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

/// Starts the writer thread: each line goes to the console and to `path`.
/// Without a usable file, the console alone is kept and a `WARN` says so.
pub fn init(path: &str) {
    let file = Path::new(path).parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| OpenOptions::new().create(true).append(true).open(path));
    let (mut sink, failure) = match file {
        Ok(file) => (Some(file), None),
        Err(error) => (None, Some(error)),
    };
    let (sender, receiver) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in receiver {
            println!("{line}");
            if let Some(file) = &mut sink {
                let _ = writeln!(file, "{line}");
            }
        }
    });
    let _ = SINK.set(sender);

    if let Some(error) = failure {
        log(Level::Warn, "log_file_unavailable", json!({"path": path, "error": error.to_string()}));
    }
}

/// Logs `event` with `fields`, an object whose keys follow `ts`, `level`
/// and `event`.
pub fn log(level: Level, event: &str, fields: Value) {
    if let Some(sink) = SINK.get() {
        let _ = sink.send(render(level, event, &fields, SystemTime::now()));
    }
}

pub fn info(event: &str, fields: Value) {
    log(Level::Info, event, fields);
}

pub fn warn(event: &str, fields: Value) {
    log(Level::Warn, event, fields);
}

pub fn error(event: &str, fields: Value) {
    log(Level::Error, event, fields);
}

/// One log line. `fields` must be a JSON object; its keys come out sorted.
fn render(level: Level, event: &str, fields: &Value, at: SystemTime) -> String {
    let head = format!(
        "{{\"ts\":\"{}\",\"level\":\"{}\",\"event\":{}",
        iso_utc(at.duration_since(UNIX_EPOCH).unwrap_or_default()),
        level.as_str(),
        Value::from(event)
    );
    // serde_json writes an object as `{...}`: its inside is spliced after
    // the head, so the three fixed keys always come first.
    let body = fields.to_string();
    match body.strip_prefix('{').and_then(|b| b.strip_suffix('}')) {
        Some(inside) if !inside.is_empty() => format!("{head},{inside}}}"),
        _ => format!("{head}}}"),
    }
}

/// `since` the Unix epoch as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
fn iso_utc(since: Duration) -> String {
    let secs = since.as_secs();
    let (hour, minute, second) = (secs % 86_400 / 3_600, secs % 3_600 / 60, secs % 60);

    // Days since 1970-01-01 to a calendar date, Howard Hinnant's
    // civil_from_days: eras of 400 years (146 097 days), each year counted
    // from March so that February 29 falls last.
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524
        - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        since.subsec_millis()
    )
}

// MARK: - World events

/// World event a command makes when it succeeds, with the argument worth
/// logging. `None` for commands that only read the world.
pub fn world_event(cmd: &Command) -> Option<(&'static str, String)> {
    let event = match cmd {
        Command::Connect { username } => ("player_joined", username.clone()),
        Command::Move { direction } => ("player_moved", direction.clone()),
        Command::Take { item } => ("item_taken", item.clone()),
        Command::Drop { item } => ("item_dropped", item.clone()),
        Command::Talk { npc } => ("npc_talk", npc.clone()),
        Command::Attack { target } => ("combat_round", target.clone()),
        Command::Quest { quest_id } => ("quest_update", quest_id.clone()),
        Command::GroupCreate => ("group_created", String::new()),
        Command::GroupInvite { username } => ("group_invite", username.clone()),
        Command::GroupJoin { group_id } => ("group_joined", group_id.clone()),
        Command::GroupLeave => ("group_left", String::new()),
        Command::ChatGlobal { message } => ("chat_global", message.clone()),
        Command::ChatRoom { message } => ("chat_room", message.clone()),
        Command::ChatGroup { message } => ("chat_group", message.clone()),
        Command::Look | Command::Inventory | Command::Status | Command::Quests
        | Command::Who | Command::Quit => return None,
    };
    Some(event)
}

/// Logs the world event of a command that succeeded with reply `body`.
/// A JSON body (ATTACK, QUEST) is logged as JSON; a quest whose status
/// becomes `completed` is logged as `quest_completed`.
pub fn world(event: &str, player: Option<&str>, arg: &str, body: &str) {
    let result = serde_json::from_str::<Value>(body).unwrap_or_else(|_| Value::from(body));
    let event = if event == "quest_update" && result["status"] == "completed" {
        "quest_completed"
    } else {
        event
    };
    info(event, json!({"player": player, "arg": arg, "result": result}));
}

// MARK: - Abuse

/// Counts hits in a sliding time window, to spot bursts.
pub struct RateWatch {
    window: Duration,
    limit: usize,
    hits: VecDeque<Instant>,
    reported: bool,
}

impl RateWatch {
    pub fn new(limit: usize, window: Duration) -> RateWatch {
        RateWatch { window, limit, hits: VecDeque::new(), reported: false }
    }

    /// Records one hit at `now`. True once per burst, on the hit that goes
    /// over the limit: a flood is logged once, not once per command.
    pub fn hit(&mut self, now: Instant) -> bool {
        while self.hits.front().is_some_and(|&first| now.duration_since(first) > self.window) {
            self.hits.pop_front();
        }
        self.hits.push_back(now);
        if self.hits.len() <= self.limit {
            self.reported = false;
            return false;
        }
        !std::mem::replace(&mut self.reported, true)
    }

    /// True when no hit is left in the window, so the watch can be dropped.
    pub fn is_idle(&self, now: Instant) -> bool {
        self.hits.back().is_none_or(|&last| now.duration_since(last) > self.window)
    }
}

// MARK: - Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_utc_known_dates() {
        assert_eq!(iso_utc(Duration::ZERO), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_utc(Duration::from_secs(951_782_400)), "2000-02-29T00:00:00.000Z");
        assert_eq!(
            iso_utc(Duration::from_millis(1_792_679_000_512)),
            "2026-10-22T14:23:20.512Z"
        );
    }

    #[test]
    fn render_puts_fixed_keys_first_and_stays_json() {
        let at = UNIX_EPOCH + Duration::from_millis(1_792_679_000_512);
        let line = render(Level::Warn, "reply", &json!({"reply": "ERR 401 \"x\"", "ip": "1.2.3.4"}), at);
        assert!(line.starts_with(
            r#"{"ts":"2026-10-22T14:23:20.512Z","level":"WARN","event":"reply","#
        ));
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["reply"], "ERR 401 \"x\"");

        let bare = render(Level::Info, "server_start", &json!({}), at);
        assert!(serde_json::from_str::<Value>(&bare).is_ok());
    }

    #[test]
    fn rate_watch_reports_once_per_burst() {
        let start = Instant::now();
        let mut watch = RateWatch::new(3, Duration::from_secs(1));
        let reports: Vec<bool> = (0..6).map(|i| watch.hit(start + Duration::from_millis(i * 10))).collect();
        assert_eq!(reports, [false, false, false, true, false, false]);

        // Quiet for longer than the window: a new burst is reported again.
        let later = start + Duration::from_secs(3);
        assert!(watch.is_idle(later));
        let again: Vec<bool> = (0..4).map(|i| watch.hit(later + Duration::from_millis(i))).collect();
        assert_eq!(again, [false, false, false, true]);
    }

    #[test]
    fn read_only_commands_make_no_world_event() {
        assert!(world_event(&Command::Look).is_none());
        let take = Command::Take { item: "Cell Key".to_string() };
        assert_eq!(world_event(&take), Some(("item_taken", "Cell Key".to_string())));
    }
}
