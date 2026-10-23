//! Pictures shown in the window. They are built into the program, so it
//! finds them wherever it is started from.

use eframe::egui;

/// The picture of a room of our world, found by its id. Another group's
/// world has other rooms: they get no picture.
pub fn room_image(room_id: &str) -> Option<egui::ImageSource<'static>> {
    let image = match room_id {
        "entrance" => egui::include_image!("../../tap_client/assets/Dungeon_Entrance.jpeg"),
        "corridor" => egui::include_image!("../../tap_client/assets/Corridor.jpeg"),
        "guard_room" => egui::include_image!("../../tap_client/assets/Guard_Room.jpeg"),
        "library" => egui::include_image!("../../tap_client/assets/Library.jpeg"),
        "armory" => egui::include_image!("../../tap_client/assets/Armory.jpeg"),
        "monster_room" => egui::include_image!("../../tap_client/assets/Monster_Lair.jpeg"),
        "treasure" => egui::include_image!("../../tap_client/assets/Treasure_Room.jpeg"),
        "secret_cell" => egui::include_image!("../../tap_client/assets/Secret_Cell.jpeg"),
        _ => return None,
    };
    Some(image)
}

/// The small picture of an item or an NPC of our world, found by its id.
/// Another group's world has other ids: they get no picture, only their name.
pub fn icon(id: &str) -> Option<egui::ImageSource<'static>> {
    let image = match id {
        "cell_key" => egui::include_image!("../../tap_client/assets/Cell_Key.jpeg"),
        "sword" => egui::include_image!("../../tap_client/assets/Sword.jpeg"),
        "treasure" => egui::include_image!("../../tap_client/assets/Treasure.jpeg"),
        "torch" => egui::include_image!("../../tap_client/assets/Torch.jpeg"),
        "shield" => egui::include_image!("../../tap_client/assets/Shield.jpeg"),
        "merchant" => egui::include_image!("../../tap_client/assets/Merchant.jpeg"),
        "prisoner" => egui::include_image!("../../tap_client/assets/Prisoner.jpeg"),
        "monster" => egui::include_image!("../../tap_client/assets/Monster.jpeg"),
        _ => return None,
    };
    Some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_room_of_our_world_has_a_picture() {
        let rooms = [
            "entrance",
            "corridor",
            "guard_room",
            "library",
            "armory",
            "monster_room",
            "treasure",
            "secret_cell",
        ];
        for room in rooms {
            assert!(room_image(room).is_some(), "no picture for {room}");
        }
        assert!(room_image("loc.bakery").is_none());
    }

    #[test]
    fn every_item_and_npc_of_our_world_has_an_icon() {
        let ids = [
            "cell_key", "sword", "treasure", "torch", "shield", "merchant", "prisoner", "monster",
        ];
        for id in ids {
            assert!(icon(id).is_some(), "no icon for {id}");
        }
        assert!(icon("item.loaf_bread").is_none());
    }
}
