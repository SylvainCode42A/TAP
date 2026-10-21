//! The log panel: every line sent and received, with a colour for each kind.

use eframe::egui;

use crate::network::CONNECTION_CLOSED;

/// Lines longer than this are cut in the panel. The whole line shows on hover.
const MAX_CHARS: usize = 80;

/// What a log line is about, to choose its colour.
#[derive(Debug, PartialEq)]
enum LineKind {
    /// A command we sent: `> LOOK`.
    Sent,
    /// An error from the server (`ERR ...`) or from the connection.
    Error,
    /// Everything else: replies, greeting, events.
    Other,
}

fn line_kind(line: &str) -> LineKind {
    let local_errors = [
        "Not connected",
        "Send error",
        "Connection to",
        "Cannot clone",
        CONNECTION_CLOSED,
    ];
    if line.starts_with("> ") {
        LineKind::Sent
    } else if line.starts_with("ERR ") || local_errors.iter().any(|e| line.starts_with(e)) {
        LineKind::Error
    } else {
        LineKind::Other
    }
}

/// The line cut to `MAX_CHARS` characters with `…`, or None if it is short enough.
fn shorten(line: &str) -> Option<String> {
    if line.chars().count() <= MAX_CHARS {
        return None;
    }
    let start: String = line.chars().take(MAX_CHARS).collect();
    Some(format!("{start}…"))
}

/// Draws the log, newest line at the bottom.
pub fn show_log(ui: &mut egui::Ui, log: &[String]) {
    // Colours of the current theme, so they read well in light and dark mode.
    let sent_color = ui.visuals().hyperlink_color;
    let error_color = ui.visuals().error_fg_color;
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink(false)
        .show(ui, |ui| {
            for line in log {
                let text = shorten(line).unwrap_or_else(|| line.clone());
                let mut rich = egui::RichText::new(text);
                match line_kind(line) {
                    LineKind::Sent => rich = rich.color(sent_color),
                    LineKind::Error => rich = rich.color(error_color),
                    LineKind::Other => {}
                }
                let label = ui.label(rich);
                if line.chars().count() > MAX_CHARS {
                    label.on_hover_text(line);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_get_their_kind() {
        assert_eq!(line_kind("> LOOK"), LineKind::Sent);
        assert_eq!(line_kind("ERR 301 NO_EXIT"), LineKind::Error);
        assert_eq!(line_kind("Not connected"), LineKind::Error);
        assert_eq!(line_kind(CONNECTION_CLOSED), LineKind::Error);
        assert_eq!(line_kind("Connected to 127.0.0.1:4242"), LineKind::Other);
        assert_eq!(line_kind("OK players=1"), LineKind::Other);
    }

    #[test]
    fn long_lines_are_cut() {
        assert_eq!(shorten("OK players=1"), None);
        let long = "x".repeat(100);
        let cut = shorten(&long).expect("cut");
        assert_eq!(cut.chars().count(), MAX_CHARS + 1);
        assert!(cut.ends_with('…'));
    }
}
