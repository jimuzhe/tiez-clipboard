//! Passive global listener for physical ⌘V — plays paste sound without intercepting the shortcut.
//!
//! Uses a listen-only HID tap so we never synthesize/re-inject events (no
//! `kTCCServicePostEvent` checks). If Accessibility is revoked, the tap is
//! torn down instead of force-re-enabled — otherwise WindowServer stalls the
//! whole HID path querying TCC on every key/mouse event.

use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::permissions::{has_accessibility_permission, V_KEY_CODE};

type CFMachPortRef = *mut std::ffi::c_void;
type CFRunLoopRef = *mut std::ffi::c_void;
type CFRunLoopSourceRef = *mut std::ffi::c_void;
type CGEventRef = *mut std::ffi::c_void;
type CGEventTapProxy = *mut std::ffi::c_void;
type CGEventType = u32;
type CGEventMask = u64;
type CGEventFlags = u64;
type CGEventTapLocation = u32;
type CGEventTapPlacement = u32;
type CGEventTapOptions = u32;
type CGEventField = u32;

const K_CG_EVENT_KEY_DOWN: CGEventType = 10;
const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: CGEventType = 0xFFFF_FFFE;
const K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT: CGEventType = 0xFFFF_FFFF;
/// Listen-only: callback return value is ignored; events are never re-injected.
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: CGEventTapOptions = 1;
const K_CG_HEAD_INSERT_EVENT_TAP: CGEventTapPlacement = 0;
const K_CG_HID_EVENT_TAP: CGEventTapLocation = 0;
const K_CG_EVENT_FLAG_MASK_COMMAND: CGEventFlags = 1 << 20;
const K_CG_KEYBOARD_EVENT_KEYCODE: CGEventField = 9;
const K_CG_KEYBOARD_EVENT_AUTOREPEAT: CGEventField = 8;

static SUPPRESS_CMDV_SOUND_UNTIL_MS: AtomicU64 = AtomicU64::new(0);
static MONITOR_STARTED: OnceLock<()> = OnceLock::new();
static ACTIVE_EVENT_TAP: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Skip tap feedback briefly after TieZ posts a synthetic ⌘V (schedule_paste_sound handles that).
pub fn suppress_paste_sound_briefly() {
    SUPPRESS_CMDV_SOUND_UNTIL_MS.store(now_ms().saturating_add(250), Ordering::Release);
}

fn should_play_for_physical_cmd_v() -> bool {
    now_ms() >= SUPPRESS_CMDV_SOUND_UNTIL_MS.load(Ordering::Acquire)
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: CGEventTapLocation,
        place: CGEventTapPlacement,
        options: CGEventTapOptions,
        events_of_interest: CGEventMask,
        callback: Option<
            unsafe extern "C" fn(
                CGEventTapProxy,
                CGEventType,
                CGEventRef,
                *mut std::ffi::c_void,
            ) -> CGEventRef,
        >,
        user_info: *mut std::ffi::c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetFlags(event: CGEventRef) -> CGEventFlags;
    fn CGEventGetIntegerValueField(event: CGEventRef, field: CGEventField) -> i64;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: *const std::ffi::c_void,
        port: CFMachPortRef,
        order: isize,
    ) -> CFRunLoopSourceRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(
        rl: CFRunLoopRef,
        source: CFRunLoopSourceRef,
        mode: *const std::ffi::c_void,
    );
    fn CFRunLoopRemoveSource(
        rl: CFRunLoopRef,
        source: CFRunLoopSourceRef,
        mode: *const std::ffi::c_void,
    );
    fn CFRunLoopRun();
    fn CFRunLoopStop(rl: CFRunLoopRef);
    fn CFMachPortInvalidate(port: CFMachPortRef);
    fn CFRelease(cf: *const std::ffi::c_void);
    static kCFRunLoopCommonModes: *const std::ffi::c_void;
}

unsafe fn tear_down_active_tap(tap: *mut std::ffi::c_void) {
    if tap.is_null() {
        return;
    }
    CGEventTapEnable(tap as CFMachPortRef, false);
    ACTIVE_EVENT_TAP.store(std::ptr::null_mut(), Ordering::Release);
    CFRunLoopStop(CFRunLoopGetCurrent());
}

unsafe extern "C" fn cmd_v_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: CGEventRef,
    _user_info: *mut std::ffi::c_void,
) -> CGEventRef {
    if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT
        || event_type == K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT
    {
        let tap = ACTIVE_EVENT_TAP.load(Ordering::Acquire);
        if tap.is_null() {
            return event;
        }

        // Permission revoked: leave the HID path immediately. Force-enabling
        // here keeps WindowServer doing a TCC PostEvent round-trip per event
        // and freezes keyboard/mouse system-wide.
        if !has_accessibility_permission() {
            eprintln!("[paste-key-monitor] accessibility lost — stopping ⌘V tap");
            tear_down_active_tap(tap);
            return event;
        }

        if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT {
            CGEventTapEnable(tap as CFMachPortRef, true);
        } else {
            // Disabled by user input while still trusted — rebuild via outer loop.
            eprintln!("[paste-key-monitor] tap disabled by user input — rebuilding");
            tear_down_active_tap(tap);
        }
        return event;
    }

    if event_type == K_CG_EVENT_KEY_DOWN && !event.is_null() {
        let flags = CGEventGetFlags(event);
        let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16;
        let autorepeat = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_AUTOREPEAT);

        if autorepeat == 0
            && keycode == V_KEY_CODE
            && (flags & K_CG_EVENT_FLAG_MASK_COMMAND) != 0
            && should_play_for_physical_cmd_v()
        {
            if let Some(app) = crate::global_state::GLOBAL_APP_HANDLE.get() {
                crate::services::ui_sound::schedule_paste_sound(app);
            }
        }
    }
    // Listen-only: return value is ignored by the system.
    event
}

/// Blocks on a run loop until the tap is torn down. Returns when accessibility
/// is lost or the tap is otherwise stopped so the caller can poll and rebuild.
unsafe fn try_start_event_tap() -> bool {
    if !has_accessibility_permission() {
        return false;
    }

    let mask: CGEventMask = 1u64 << K_CG_EVENT_KEY_DOWN;
    let tap = CGEventTapCreate(
        K_CG_HID_EVENT_TAP,
        K_CG_HEAD_INSERT_EVENT_TAP,
        K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
        mask,
        Some(cmd_v_callback),
        std::ptr::null_mut(),
    );
    if tap.is_null() {
        eprintln!("[paste-key-monitor] CGEventTapCreate returned null");
        return false;
    }

    ACTIVE_EVENT_TAP.store(tap as *mut std::ffi::c_void, Ordering::Release);

    let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
    if source.is_null() {
        eprintln!("[paste-key-monitor] Failed to create run loop source");
        ACTIVE_EVENT_TAP.store(std::ptr::null_mut(), Ordering::Release);
        CFMachPortInvalidate(tap);
        CFRelease(tap as *const std::ffi::c_void);
        return false;
    }

    let run_loop = CFRunLoopGetCurrent();
    CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);

    CGEventTapEnable(tap, true);
    eprintln!("[paste-key-monitor] ⌘V listen-only listener active");
    CFRunLoopRun();
    ACTIVE_EVENT_TAP.store(std::ptr::null_mut(), Ordering::Release);
    // Release the old tap before the outer loop builds a new one; each
    // revoke/re-grant cycle used to leak a Mach port and run loop source.
    // From PR #178 by @andya1lan.
    CFRunLoopRemoveSource(run_loop, source, kCFRunLoopCommonModes);
    CFMachPortInvalidate(tap);
    CFRelease(source as *const std::ffi::c_void);
    CFRelease(tap as *const std::ffi::c_void);
    eprintln!("[paste-key-monitor] ⌘V listener stopped — waiting for accessibility");
    true
}

pub fn start_paste_key_monitor() {
    if MONITOR_STARTED.set(()).is_err() {
        return;
    }

    std::thread::Builder::new()
        .name("tiez-paste-key-monitor".into())
        .spawn(|| {
            // Keep polling forever: after a tap is torn down (e.g. user revokes
            // Accessibility), re-create it once permission returns — without
            // requiring an app restart.
            loop {
                if has_accessibility_permission() {
                    let _ = unsafe { try_start_event_tap() };
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{NSPoint, NSString};

    /// Autorepeat and keycode of a ⌘V key-down built by AppKit, read the way
    /// the tap callback reads them.
    fn cmd_v_fields(is_repeat: bool) -> (i64, u16) {
        let v = NSString::from_str("v");
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown,
            NSPoint::new(0.0, 0.0),
            NSEventModifierFlags::Command,
            0.0,
            0,
            None,
            &v,
            &v,
            is_repeat,
            V_KEY_CODE,
        )
        .expect("AppKit builds the key event");
        let cg_event = event.CGEvent().expect("the key event has a CGEvent");
        let cg_event = Retained::as_ptr(&cg_event) as CGEventRef;
        unsafe {
            (
                CGEventGetIntegerValueField(cg_event, K_CG_KEYBOARD_EVENT_AUTOREPEAT),
                CGEventGetIntegerValueField(cg_event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16,
            )
        }
    }

    #[test]
    fn tells_a_held_cmd_v_from_a_new_press() {
        assert_eq!(cmd_v_fields(false), (0, V_KEY_CODE));
        assert_eq!(cmd_v_fields(true), (1, V_KEY_CODE));
    }
}
