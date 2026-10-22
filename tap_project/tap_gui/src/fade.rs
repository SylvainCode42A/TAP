//! The fade-in of the room panel when the player enters another room.

/// Seconds a new room takes to fully appear.
const FADE_SECONDS: f64 = 0.4;

/// Remembers which room is shown and since when.
#[derive(Default)]
pub struct RoomFade {
    room: Option<String>,
    started: f64,
}

impl RoomFade {
    /// Opacity of the room panel at time `now` (in seconds): 0 just after
    /// the room changed, then up to 1 after `FADE_SECONDS`.
    pub fn opacity(&mut self, room: &str, now: f64) -> f32 {
        if self.room.as_deref() != Some(room) {
            self.room = Some(room.to_string());
            self.started = now;
        }
        ((now - self.started) / FADE_SECONDS).clamp(0.0, 1.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_room_fades_in() {
        let mut fade = RoomFade::default();
        assert_eq!(fade.opacity("entrance", 0.0), 0.0);
        assert_eq!(fade.opacity("entrance", 0.2), 0.5);
        assert_eq!(fade.opacity("entrance", 1.0), 1.0);
        // Another room starts again from 0.
        assert_eq!(fade.opacity("corridor", 2.0), 0.0);
    }
}
