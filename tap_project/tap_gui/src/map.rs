//! The mini-map: every known room on a grid, built from the exits of each LOOK.
//! Nothing is written for our world only, so it also works on another group's world.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use eframe::egui;

/// Grid move for an exit direction. Other directions (like `south_alt`)
/// cannot be placed: their room waits until a normal exit leads to it.
fn step(direction: &str) -> Option<(i32, i32)> {
    match direction {
        "north" => Some((0, -1)),
        "south" => Some((0, 1)),
        "east" => Some((1, 0)),
        "west" => Some((-1, 0)),
        _ => None,
    }
}

/// What the player knows of the world.
#[derive(Default)]
pub struct MiniMap {
    /// Grid cell of every placed room: (x, y), y grows to the south.
    cells: HashMap<String, (i32, i32)>,
    /// Rooms the player has stood in.
    visited: BTreeSet<String>,
    /// Rooms linked by an exit, each pair once and sorted.
    links: BTreeSet<(String, String)>,
    /// Room the player is in now.
    current: Option<String>,
}

impl MiniMap {
    /// Adds what a LOOK reply shows: the room we are in and its exits.
    pub fn update(&mut self, room_id: &str, exits: &BTreeMap<String, String>) {
        let here = match self.cells.get(room_id) {
            Some(&cell) => cell,
            None => {
                let cell = self.free_cell();
                self.cells.insert(room_id.to_string(), cell);
                cell
            }
        };
        for (direction, target) in exits {
            let pair = if room_id < target.as_str() {
                (room_id.to_string(), target.clone())
            } else {
                (target.clone(), room_id.to_string())
            };
            self.links.insert(pair);
            if let Some((dx, dy)) = step(direction) {
                // The first place found is kept: a world is not always a
                // real grid, and rooms must not jump around.
                self.cells
                    .entry(target.clone())
                    .or_insert((here.0 + dx, here.1 + dy));
            }
        }
        self.visited.insert(room_id.to_string());
        self.current = Some(room_id.to_string());
    }

    /// The first room goes in the middle. A room no exit leads to goes to the
    /// right of the others, so it never hides one.
    fn free_cell(&self) -> (i32, i32) {
        match self.cells.values().map(|&(x, _)| x).max() {
            None => (0, 0),
            Some(max_x) => (max_x + 2, 0),
        }
    }
}

/// Draws the mini-map: links first, then one square per room.
/// The room we are in is coloured, visited rooms are filled, the others
/// (seen through an exit only) are just outlined. Hover a room to see its id.
pub fn show_map(ui: &mut egui::Ui, map: &MiniMap) {
    if map.cells.is_empty() {
        ui.label("-");
        return;
    }
    let min_x = map.cells.values().map(|c| c.0).min().unwrap_or(0);
    let max_x = map.cells.values().map(|c| c.0).max().unwrap_or(0);
    let min_y = map.cells.values().map(|c| c.1).min().unwrap_or(0);
    let max_y = map.cells.values().map(|c| c.1).max().unwrap_or(0);
    let columns = (max_x - min_x + 1) as f32;
    let rows = (max_y - min_y + 1) as f32;

    // One cell per room, at most 40 points, smaller if the map is wide.
    let width = ui.available_width();
    let cell = (width / columns).min(40.0);
    let size = egui::vec2(width, cell * rows);
    let (response, painter) = ui.allocate_painter(size, egui::Sense::hover());
    let left = response.rect.center().x - cell * columns / 2.0;
    let top = response.rect.top();
    let center_of = |(x, y): (i32, i32)| {
        egui::pos2(
            left + (x - min_x) as f32 * cell + cell / 2.0,
            top + (y - min_y) as f32 * cell + cell / 2.0,
        )
    };

    let visuals = ui.visuals();
    let line = egui::Stroke::new(2.0, visuals.weak_text_color());
    for (a, b) in &map.links {
        if let (Some(&ca), Some(&cb)) = (map.cells.get(a), map.cells.get(b)) {
            painter.line_segment([center_of(ca), center_of(cb)], line);
        }
    }

    let half = egui::vec2(cell * 0.3, cell * 0.3);
    let mut hovered = None;
    for (room, &cell_pos) in &map.cells {
        let square = egui::Rect::from_center_size(center_of(cell_pos), half * 2.0);
        if map.current.as_ref() == Some(room) {
            painter.rect_filled(square, 3.0, visuals.hyperlink_color);
        } else if map.visited.contains(room) {
            painter.rect_filled(square, 3.0, visuals.weak_text_color());
        } else {
            painter.rect_filled(square, 3.0, visuals.panel_fill);
            painter.rect_stroke(square, 3.0, line, egui::StrokeKind::Inside);
        }
        if response.hover_pos().is_some_and(|pos| square.contains(pos)) {
            hovered = Some(room.clone());
        }
    }
    if let Some(room) = hovered {
        response.on_hover_text(room);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exits(list: &[(&str, &str)]) -> BTreeMap<String, String> {
        list.iter()
            .map(|(d, r)| (d.to_string(), r.to_string()))
            .collect()
    }

    #[test]
    fn rooms_are_placed_by_their_exit() {
        let mut map = MiniMap::default();
        map.update(
            "entrance",
            &exits(&[("north", "corridor"), ("east", "guard_room")]),
        );
        assert_eq!(map.cells["entrance"], (0, 0));
        assert_eq!(map.cells["corridor"], (0, -1));
        assert_eq!(map.cells["guard_room"], (1, 0));
        assert!(map.visited.contains("entrance"));
        assert!(!map.visited.contains("corridor"));

        map.update("corridor", &exits(&[("south", "entrance")]));
        assert_eq!(map.current.as_deref(), Some("corridor"));
        // Same link seen from both sides: kept once.
        assert_eq!(map.links.len(), 2);
    }

    #[test]
    fn odd_exits_do_not_move_rooms() {
        let mut map = MiniMap::default();
        map.update("library", &exits(&[("north", "monster_room")]));
        map.update(
            "monster_room",
            &exits(&[("south_alt", "armory"), ("south", "library")]),
        );
        // `south_alt` has no grid move: armory is linked but not placed yet.
        assert!(!map.cells.contains_key("armory"));
        assert_eq!(map.cells["monster_room"], (0, -1));
        assert_eq!(map.cells["library"], (0, 0));
    }
}
