//! Test the real input module without FFmpeg, a graphics device or a running Mac.
//! The viewer imports egui through eframe; expose the same path using the exact egui version.
extern crate self as eframe;
pub use egui;

#[path = "../../src/input.rs"]
pub mod input;

#[cfg(test)]
mod tests {
    use super::input::{self, Keyboard, HID_ALT, HID_CMD, HID_CTRL, HID_SHIFT};
    use egui::{Key, Modifiers};
    use serde_json::Value;

    const CMD: u64 = 0x100000;
    const CTRL: u64 = 0x40000;
    const SHIFT: u64 = 0x20000;
    const ALT: u64 = 0x80000;

    fn keys(events: &[Value]) -> Vec<(u32, bool, u64)> {
        events.iter().map(|v| {
            let k = &v["key"];
            (k["hidUsage"].as_u64().unwrap() as u32,
             k["down"].as_bool().unwrap(), k["modifiers"].as_u64().unwrap())
        }).collect()
    }

    fn mac(local: Modifiers) -> Modifiers {
        input::remote_modifiers(local, true)
    }

    #[test]
    fn common_windows_shortcuts_send_command_not_control() {
        for (letter, usage) in [(Key::Q, 0x14), (Key::W, 0x1A), (Key::S, 0x16),
                                (Key::C, 0x06), (Key::V, 0x19), (Key::A, 0x04),
                                (Key::Z, 0x1D), (Key::T, 0x17)] {
            let mut keyboard = Keyboard::default();
            let events = keyboard.key(input::hid(letter).unwrap(), true, mac(Modifiers::CTRL));
            assert_eq!(keys(&events), [(HID_CMD, true, CMD), (usage, true, CMD)]);
        }
    }

    #[test]
    fn native_mode_keeps_control_for_terminal_shortcuts() {
        let mut keyboard = Keyboard::default();
        let mods = input::remote_modifiers(Modifiers::CTRL, false);
        assert_eq!(keys(&keyboard.key(0x06, true, mods)),
                   [(HID_CTRL, true, CTRL), (0x06, true, CTRL)]);
    }

    #[test]
    fn shift_and_option_survive_mac_mapping() {
        let mut keyboard = Keyboard::default();
        let local = Modifiers { ctrl: true, shift: true, alt: true, ..Modifiers::NONE };
        let events = keys(&keyboard.key(0x16, true, mac(local)));
        assert_eq!(events, [(HID_SHIFT, true, CMD | SHIFT | ALT),
                           (HID_ALT, true, CMD | SHIFT | ALT),
                           (HID_CMD, true, CMD | SHIFT | ALT),
                           (0x16, true, CMD | SHIFT | ALT)]);
    }

    #[test]
    fn native_mac_command_is_preserved_in_both_modes() {
        let local = Modifiers { mac_cmd: true, command: true, ..Modifiers::NONE };
        for enabled in [false, true] {
            assert_eq!(input::flags(&input::remote_modifiers(local, enabled)), CMD);
        }
    }

    #[test]
    fn unmodified_typing_stays_unmodified() {
        assert_eq!(input::flags(&mac(Modifiers::NONE)), 0);
        let mut keyboard = Keyboard::default();
        assert_eq!(keys(&keyboard.key(0x14, true, mac(Modifiers::NONE))), [(0x14, true, 0)]);
    }

    #[test]
    fn fast_chord_keeps_event_modifiers_when_frame_ends_with_control_up() {
        let mut keyboard = Keyboard::default();
        let mut events = keyboard.key(0x14, true, mac(Modifiers::CTRL));
        events.extend(keyboard.key(0x14, false, mac(Modifiers::CTRL)));
        events.extend(keyboard.modifiers(mac(Modifiers::NONE)));
        assert_eq!(keys(&events), [(HID_CMD, true, CMD), (0x14, true, CMD),
                                  (0x14, false, CMD), (HID_CMD, false, 0)]);
    }

    #[test]
    fn releasing_control_before_letter_does_not_leave_command_down() {
        let mut keyboard = Keyboard::default();
        keyboard.key(0x14, true, mac(Modifiers::CTRL));
        assert_eq!(keys(&keyboard.key(0x14, false, Modifiers::NONE)),
                   [(HID_CMD, false, 0), (0x14, false, 0)]);
        assert!(keyboard.release_all().is_empty());
    }

    #[test]
    fn focus_loss_releases_letter_before_command_and_is_idempotent() {
        let mut keyboard = Keyboard::default();
        keyboard.key(0x14, true, mac(Modifiers::CTRL));
        assert_eq!(keys(&keyboard.release_all()), [(0x14, false, CMD), (HID_CMD, false, 0)]);
        assert!(keyboard.release_all().is_empty());
    }

    #[test]
    fn mapping_switch_releases_command_before_new_control_press() {
        let mut keyboard = Keyboard::default();
        keyboard.modifiers(mac(Modifiers::CTRL));
        let mut events = keyboard.release_all();
        events.extend(keyboard.key(0x06, true, Modifiers::CTRL));
        assert_eq!(keys(&events), [(HID_CMD, false, 0), (HID_CTRL, true, CTRL),
                                  (0x06, true, CTRL)]);
    }

    #[test]
    fn repeated_presses_are_not_duplicated() {
        let mut keyboard = Keyboard::default();
        keyboard.key(0x14, true, mac(Modifiers::CTRL));
        assert!(keyboard.key(0x14, true, mac(Modifiers::CTRL)).is_empty());
        assert_eq!(keys(&keyboard.release_all()).iter().filter(|k| k.0 == 0x14).count(), 1);
    }

    #[test]
    fn unmatched_key_release_is_not_forwarded() {
        let mut keyboard = Keyboard::default();
        assert!(keyboard.key(0x09, false, Modifiers::NONE).is_empty());
    }

    #[test]
    fn clipboard_events_send_command_c_x_v() {
        for usage in [0x06, 0x1B, 0x19] {
            let mut keyboard = Keyboard::default();
            let mods = input::clipboard_modifiers(Modifiers::CTRL, true);
            let mut events = keyboard.tap(usage, mods);
            events.extend(keyboard.modifiers(Modifiers::NONE));
            assert_eq!(keys(&events), [(HID_CMD, true, CMD), (usage, true, CMD),
                                      (usage, false, CMD), (HID_CMD, false, 0)]);
        }
    }

    #[test]
    fn clipboard_event_after_control_release_still_sends_command() {
        let mut keyboard = Keyboard::default();
        let mods = input::clipboard_modifiers(Modifiers::NONE, true);
        assert_eq!(keys(&keyboard.tap(0x19, mods)),
                   [(HID_CMD, true, CMD), (0x19, true, CMD), (0x19, false, CMD)]);
    }

    #[test]
    fn clipboard_event_does_not_release_a_letter_already_held() {
        let mut keyboard = Keyboard::default();
        keyboard.key(0x06, true, mac(Modifiers::CTRL));
        assert!(keyboard.tap(0x06, mac(Modifiers::CTRL)).is_empty());
        assert_eq!(keys(&keyboard.release_all()), [(0x06, false, CMD), (HID_CMD, false, 0)]);
    }

    #[test]
    fn new_press_works_after_focus_returns() {
        let mut keyboard = Keyboard::default();
        keyboard.key(0x14, true, mac(Modifiers::CTRL));
        keyboard.release_all();
        assert_eq!(keys(&keyboard.key(0x14, true, mac(Modifiers::CTRL))),
                   [(HID_CMD, true, CMD), (0x14, true, CMD)]);
    }

    #[test]
    fn local_fullscreen_chord_releases_remote_modifiers_until_all_are_up() {
        let mut keyboard = Keyboard::default();
        keyboard.modifiers(mac(Modifiers::CTRL));
        assert_eq!(keys(&keyboard.local_shortcut()), [(HID_CMD, false, 0)]);
        let chord = mac(Modifiers { ctrl: true, shift: true, alt: true, ..Modifiers::NONE });
        assert!(keyboard.modifiers(chord).is_empty());
        assert!(keyboard.key(0x09, false, chord).is_empty());
        assert!(keyboard.modifiers(mac(Modifiers::CTRL)).is_empty());
        assert!(keyboard.modifiers(Modifiers::NONE).is_empty());
        assert_eq!(keys(&keyboard.key(0x14, true, mac(Modifiers::CTRL))),
                   [(HID_CMD, true, CMD), (0x14, true, CMD)]);
    }

    #[test]
    fn every_modifier_combination_preserves_shift_and_option_without_control() {
        for bits in 0..16 {
            let local = Modifiers { shift: bits & 1 != 0, ctrl: bits & 2 != 0,
                alt: bits & 4 != 0, mac_cmd: bits & 8 != 0, command: false };
            let remote = mac(local);
            assert_eq!(remote.shift, local.shift);
            assert_eq!(remote.alt, local.alt);
            assert!(!remote.ctrl);
            assert_eq!(remote.mac_cmd, local.ctrl || local.mac_cmd);
            assert_eq!(input::remote_modifiers(local, false), local);
        }
    }
}
