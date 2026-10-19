//! Keys as USB HID usages (what Sill hosts take) and Sill's input JSON.

use eframe::egui::{Key, Modifiers};
use serde_json::{json, Value};

pub const HID_CTRL: u32 = 0xE0;
pub const HID_SHIFT: u32 = 0xE1;
pub const HID_ALT: u32 = 0xE2;
pub const HID_CMD: u32 = 0xE3;

/// Only the outgoing stream is remapped; egui keeps the local modifiers for its own shortcuts.
pub fn remote_modifiers(local: Modifiers, mac_shortcuts: bool) -> Modifiers {
    if mac_shortcuts {
        Modifiers {
            ctrl: false,
            mac_cmd: local.mac_cmd || local.ctrl,
            command: local.mac_cmd || local.ctrl,
            ..local
        }
    } else {
        local
    }
}

/// Copy/Cut/Paste events may arrive after Ctrl was released in the same UI frame. The event
/// still represents a shortcut; reconstruct its modifier instead of sending a bare C/X/V.
pub fn clipboard_modifiers(local: Modifiers, mac_shortcuts: bool) -> Modifiers {
    let mut remote = remote_modifiers(local, mac_shortcuts);
    if !remote.ctrl && !remote.mac_cmd {
        if mac_shortcuts || cfg!(target_os = "macos") {
            remote.mac_cmd = true;
        } else {
            remote.ctrl = true;
        }
        remote.command = true;
    }
    remote
}

/// The keys actually sent to a host, including modifiers after remapping. Releases use the same
/// state on focus loss, disconnect and a mapping change, so Command cannot remain held remotely.
#[derive(Default)]
pub struct Keyboard {
    modifiers: Modifiers,
    held: Vec<u32>,
    local_shortcut: bool,
}

impl Keyboard {
    fn suppressed(&mut self, next: Modifiers) -> bool {
        if self.local_shortcut && flags(&next) == 0 {
            self.local_shortcut = false;
        }
        self.local_shortcut
    }

    pub fn modifiers(&mut self, next: Modifiers) -> Vec<Value> {
        if self.suppressed(next) {
            return Vec::new();
        }
        let events = modifier_changes(&self.modifiers, &next)
            .into_iter()
            .map(|(usage, down)| key(usage, down, &next))
            .collect();
        self.modifiers = next;
        events
    }

    pub fn key(&mut self, usage: u32, down: bool, modifiers: Modifiers) -> Vec<Value> {
        if self.suppressed(modifiers) {
            return Vec::new();
        }
        let mut events = self.modifiers(modifiers);
        if down {
            if self.held.contains(&usage) {
                return events; // key repeat belongs to the host
            }
            self.held.push(usage);
        } else {
            if !self.held.contains(&usage) {
                return events; // no matching press was sent (for example, a local shortcut)
            }
            self.held.retain(|&held| held != usage);
        }
        events.push(key(usage, down, &modifiers));
        events
    }

    pub fn tap(&mut self, usage: u32, modifiers: Modifiers) -> Vec<Value> {
        if self.held.contains(&usage) {
            return Vec::new();
        }
        let mut events = self.key(usage, true, modifiers);
        events.extend(self.key(usage, false, modifiers));
        events
    }

    pub fn release_all(&mut self) -> Vec<Value> {
        self.local_shortcut = false;
        // Let go of ordinary keys before their modifiers, just as a person releases a chord.
        let mut events: Vec<_> = self.held.drain(..)
            .map(|usage| key(usage, false, &self.modifiers))
            .collect();
        events.extend(self.modifiers(Modifiers::NONE));
        events
    }

    /// A local fullscreen shortcut must not turn into Command+Option+Shift on the Mac.
    /// Wait until its physical modifiers are released before accepting another remote chord.
    pub fn local_shortcut(&mut self) -> Vec<Value> {
        let events = self.release_all();
        self.local_shortcut = true;
        events
    }
}

pub fn hid(key: Key) -> Option<u32> {
    use Key::*;
    let letters = [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z];
    if let Some(i) = letters.iter().position(|k| *k == key) {
        return Some(0x04 + i as u32);
    }
    let nums = [Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9, Num0];
    if let Some(i) = nums.iter().position(|k| *k == key) {
        return Some(0x1E + i as u32);
    }
    let f = [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12];
    if let Some(i) = f.iter().position(|k| *k == key) {
        return Some(0x3A + i as u32);
    }
    let f2 = [F13, F14, F15, F16, F17, F18, F19, F20, F21, F22, F23, F24];
    if let Some(i) = f2.iter().position(|k| *k == key) {
        return Some(0x68 + i as u32);
    }
    Some(match key {
        Enter => 0x28,
        Escape => 0x29,
        Backspace => 0x2A,
        Tab => 0x2B,
        Space => 0x2C,
        Minus => 0x2D,
        Equals | Plus => 0x2E,
        OpenBracket => 0x2F,
        CloseBracket => 0x30,
        Backslash | Pipe => 0x31,
        Semicolon | Colon => 0x33,
        Quote => 0x34,
        Backtick => 0x35,
        Comma => 0x36,
        Period => 0x37,
        Slash | Questionmark => 0x38,
        Insert => 0x49,
        Home => 0x4A,
        PageUp => 0x4B,
        Delete => 0x4C,
        End => 0x4D,
        PageDown => 0x4E,
        ArrowRight => 0x4F,
        ArrowLeft => 0x50,
        ArrowDown => 0x51,
        ArrowUp => 0x52,
        _ => return None,
    })
}

/// UIKeyModifierFlags bits, as Sill sends them.
pub fn flags(m: &Modifiers) -> u64 {
    let mut f = 0;
    if m.shift { f |= 0x20000 }
    if m.ctrl { f |= 0x40000 }
    if m.alt { f |= 0x80000 }
    if m.mac_cmd { f |= 0x100000 }
    f
}

pub fn key(usage: u32, down: bool, m: &Modifiers) -> Value {
    json!({"key": {"hidUsage": usage, "down": down, "modifiers": flags(m)}})
}

pub fn pointer(action: &str, x: f64, y: f64) -> Value {
    json!({"pointer": {"_0": action, "x": x, "y": y}})
}

pub fn scroll(x: f64, y: f64, dx: f64, dy: f64) -> Value {
    json!({"scroll": {"x": x, "y": y, "dx": dx, "dy": dy}})
}

pub fn scroll_phase(phase: &str, x: f64, y: f64) -> Value {
    json!({"scrollGesture": {"_0": phase, "x": x, "y": y}})
}

/// The modifier keys whose state changed, as (usage, down).
pub fn modifier_changes(old: &Modifiers, new: &Modifiers) -> Vec<(u32, bool)> {
    let mut out = Vec::new();
    for (was, is, usage) in [(old.shift, new.shift, HID_SHIFT), (old.ctrl, new.ctrl, HID_CTRL),
                             (old.alt, new.alt, HID_ALT), (old.mac_cmd, new.mac_cmd, HID_CMD)] {
        if was != is {
            out.push((usage, is));
        }
    }
    out
}
