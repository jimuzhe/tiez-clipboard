use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use tauri::Emitter;

use crate::global_state::{GLOBAL_APP_HANDLE, IS_RECORDING};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, EventMask, GrabMode, GrabStatus, ModMask};

const XK_ESCAPE: u32 = 0xFF1B;
const XK_SPACE: u32 = 0x0020;
const XK_RETURN: u32 = 0xFF0D;
const XK_TAB: u32 = 0xFF09;
const XK_BACKSPACE: u32 = 0xFF08;
const XK_DELETE: u32 = 0xFFFF;
const XK_INSERT: u32 = 0xFF63;
const XK_PAGE_UP: u32 = 0xFF55;
const XK_PAGE_DOWN: u32 = 0xFF56;
const XK_HOME: u32 = 0xFF50;
const XK_END: u32 = 0xFF57;
const XK_LEFT: u32 = 0xFF51;
const XK_UP: u32 = 0xFF52;
const XK_RIGHT: u32 = 0xFF53;
const XK_DOWN: u32 = 0xFF54;
const XK_F1: u32 = 0xFFBE;
const XK_F12: u32 = 0xFFC9;
const XK_PLUS: u32 = 0x002B;
const XK_COMMA: u32 = 0x002C;
const XK_MINUS: u32 = 0x002D;
const XK_PERIOD: u32 = 0x002E;
const XK_SLASH: u32 = 0x002F;
const XK_GRAVE: u32 = 0x0060;
const XK_SEMICOLON: u32 = 0x003B;
const XK_BRACKETLEFT: u32 = 0x005B;
const XK_BACKSLASH: u32 = 0x005C;
const XK_BRACKETRIGHT: u32 = 0x005D;
const XK_APOSTROPHE: u32 = 0x0027;

struct GrabGuard<'a> {
    conn: &'a x11rb::rust_connection::RustConnection,
}

impl<'a> Drop for GrabGuard<'a> {
    fn drop(&mut self) {
        let _ = self.conn.ungrab_keyboard(x11rb::CURRENT_TIME);
        let _ = self.conn.ungrab_pointer(x11rb::CURRENT_TIME);
        let _ = self.conn.flush();
    }
}

pub fn start_recording_grab() {
    thread::spawn(|| {
        if let Err(e) = recording_thread() {
            eprintln!("input_grab error: {}", e);
        }
    });
}

fn recording_thread() -> Result<(), Box<dyn std::error::Error>> {
    let (conn, screen_num) = match x11rb::connect(None) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to connect to X11: {}", e);
            return Ok(());
        }
    };

    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;

    let kb_cookie = conn.grab_keyboard(
        false,
        root,
        x11rb::CURRENT_TIME,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
    )?;
    let kb_reply = kb_cookie.reply()?;
    if kb_reply.status != GrabStatus::SUCCESS {
        eprintln!("Failed to grab keyboard: {:?}", kb_reply.status);
        return Ok(());
    }

    let ptr_cookie = conn.grab_pointer(
        false,
        root,
        EventMask::BUTTON_PRESS,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
        x11rb::NONE,
        x11rb::NONE,
        x11rb::CURRENT_TIME,
    )?;
    let ptr_reply = ptr_cookie.reply()?;
    if ptr_reply.status != GrabStatus::SUCCESS {
        eprintln!("Failed to grab pointer: {:?}", ptr_reply.status);
        let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
        let _ = conn.flush();
        return Ok(());
    }

    let _guard = GrabGuard { conn: &conn };
    let _ = conn.flush();

    while IS_RECORDING.load(Ordering::SeqCst) {
        match conn.poll_for_event()? {
            Some(event) => match event {
                x11rb::protocol::Event::KeyPress(event) => {
                    let ctrl = (u16::from(event.state) & u16::from(ModMask::CONTROL)) != 0;
                    let shift = (u16::from(event.state) & u16::from(ModMask::SHIFT)) != 0;
                    let alt = (u16::from(event.state) & u16::from(ModMask::M1)) != 0;
                    let win = (u16::from(event.state) & u16::from(ModMask::M4)) != 0;

                    let reply = conn.get_keyboard_mapping(event.detail, 1)?.reply()?;
                    let keysym = reply.keysyms.first().copied().unwrap_or(0);

                    if is_modifier_key(keysym) {
                        continue;
                    }

                    if keysym == XK_ESCAPE {
                        if let Some(handle) = GLOBAL_APP_HANDLE.get() {
                            let _ = handle.emit("recording-cancelled", ());
                        }
                        IS_RECORDING.store(false, Ordering::SeqCst);
                        break;
                    }

                    let key_name = keysym_to_name(keysym);
                    let hotkey = build_hotkey_string(ctrl, shift, alt, win, &key_name);

                    if let Some(handle) = GLOBAL_APP_HANDLE.get() {
                        let _ = handle.emit("hotkey-recorded", hotkey);
                    }
                    IS_RECORDING.store(false, Ordering::SeqCst);
                    break;
                }
                x11rb::protocol::Event::ButtonPress(event) => {
                    if event.detail == 2 {
                        if let Some(handle) = GLOBAL_APP_HANDLE.get() {
                            let _ = handle.emit("hotkey-recorded", "MouseMiddle");
                        }
                        IS_RECORDING.store(false, Ordering::SeqCst);
                        break;
                    }
                }
                _ => {}
            },
            None => {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    Ok(())
}

fn is_modifier_key(keysym: u32) -> bool {
    matches!(
        keysym,
        0xFFE1
            | 0xFFE2
            | 0xFFE3
            | 0xFFE4
            | 0xFFE9
            | 0xFFEA
            | 0xFFE7
            | 0xFFE8
            | 0xFFEB
            | 0xFFEC
            | 0xFFED
            | 0xFFEE
            | 0xFE03
            | 0xFF7E
    )
}

fn keysym_to_name(keysym: u32) -> String {
    match keysym {
        XK_SPACE => "Space".to_string(),
        XK_RETURN => "Enter".to_string(),
        XK_TAB => "Tab".to_string(),
        XK_BACKSPACE => "Backspace".to_string(),
        XK_DELETE => "Delete".to_string(),
        XK_INSERT => "Insert".to_string(),
        XK_PAGE_UP => "PageUp".to_string(),
        XK_PAGE_DOWN => "PageDown".to_string(),
        XK_HOME => "Home".to_string(),
        XK_END => "End".to_string(),
        XK_LEFT => "Left".to_string(),
        XK_UP => "Up".to_string(),
        XK_RIGHT => "Right".to_string(),
        XK_DOWN => "Down".to_string(),
        XK_PLUS => "Plus".to_string(),
        XK_COMMA => "Comma".to_string(),
        XK_MINUS => "Minus".to_string(),
        XK_PERIOD => "Period".to_string(),
        XK_SLASH => "/".to_string(),
        XK_GRAVE => "`".to_string(),
        XK_SEMICOLON => ";".to_string(),
        XK_BRACKETLEFT => "[".to_string(),
        XK_BACKSLASH => "\\".to_string(),
        XK_BRACKETRIGHT => "]".to_string(),
        XK_APOSTROPHE => "'".to_string(),
        k if k >= XK_F1 && k <= XK_F12 => format!("F{}", k - XK_F1 + 1),
        k if k >= 0x30 && k <= 0x39 => format!("{}", char::from_u32(k).unwrap()),
        k if k >= 0x41 && k <= 0x5A => format!("{}", char::from_u32(k).unwrap()),
        k if k >= 0x61 && k <= 0x7A => {
            format!("{}", char::from_u32(k).unwrap().to_ascii_uppercase())
        }
        _ => format!("Key_{}", keysym),
    }
}

fn build_hotkey_string(ctrl: bool, shift: bool, alt: bool, win: bool, key_name: &str) -> String {
    format!(
        "{}{}{}{}{}",
        if ctrl { "Ctrl+" } else { "" },
        if shift { "Shift+" } else { "" },
        if alt { "Alt+" } else { "" },
        if win { "Win+" } else { "" },
        key_name
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keysym_to_name() {
        assert_eq!(keysym_to_name(XK_ESCAPE), "Key_65307");
        assert_eq!(keysym_to_name(XK_SPACE), "Space");
        assert_eq!(keysym_to_name(XK_F1), "F1");
        assert_eq!(keysym_to_name(0x41), "A");
        assert_eq!(keysym_to_name(0x31), "1");
        assert_eq!(keysym_to_name(XK_PLUS), "Plus");
    }

    #[test]
    fn test_build_hotkey_string() {
        assert_eq!(
            build_hotkey_string(true, false, false, false, "A"),
            "Ctrl+A"
        );
        assert_eq!(
            build_hotkey_string(true, true, true, true, "F1"),
            "Ctrl+Shift+Alt+Win+F1"
        );
        assert_eq!(
            build_hotkey_string(false, false, false, false, "Space"),
            "Space"
        );
        assert_eq!(
            build_hotkey_string(false, true, false, true, "1"),
            "Shift+Win+1"
        );
    }
}
