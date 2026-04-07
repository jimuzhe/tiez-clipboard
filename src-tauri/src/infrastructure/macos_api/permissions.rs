#[cfg(target_os = "macos")]
use std::ffi::c_void;

#[cfg(target_os = "macos")]
type CFAllocatorRef = *const c_void;
#[cfg(target_os = "macos")]
type CFMutableDictionaryRef = *mut c_void;
#[cfg(target_os = "macos")]
type CFDictionaryRef = *const c_void;
#[cfg(target_os = "macos")]
type CFStringRef = *const c_void;
#[cfg(target_os = "macos")]
type CFTypeRef = *const c_void;
#[cfg(target_os = "macos")]
type Boolean = u8;

#[cfg(target_os = "macos")]
type CGEventSourceRef = *mut c_void;
#[cfg(target_os = "macos")]
type CGEventRef = *mut c_void;
#[cfg(target_os = "macos")]
type CGEventTapLocation = u32;
#[cfg(target_os = "macos")]
type CGEventFlags = u64;
#[cfg(target_os = "macos")]
type CGKeyCode = u16;
#[cfg(target_os = "macos")]
type CGEventSourceStateID = i32;

#[cfg(target_os = "macos")]
const K_CG_HID_EVENT_TAP: CGEventTapLocation = 0;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FLAG_MASK_COMMAND: CGEventFlags = 1 << 20;
#[cfg(target_os = "macos")]
const K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: CGEventSourceStateID = 1;
#[cfg(target_os = "macos")]
const V_KEY_CODE: CGKeyCode = 9;

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> Boolean;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
    static kAXTrustedCheckOptionPrompt: CFStringRef;

    fn CGEventSourceCreate(state_id: CGEventSourceStateID) -> CGEventSourceRef;
    fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: CGKeyCode,
        key_down: Boolean,
    ) -> CGEventRef;
    fn CGEventSetFlags(event: CGEventRef, flags: CGEventFlags);
    fn CGEventPost(tap: CGEventTapLocation, event: CGEventRef);
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDictionaryCreateMutable(
        allocator: CFAllocatorRef,
        capacity: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFMutableDictionaryRef;
    fn CFDictionaryAddValue(
        the_dict: CFMutableDictionaryRef,
        key: *const c_void,
        value: *const c_void,
    );
    fn CFRelease(cf: CFTypeRef);
    static kCFBooleanTrue: CFTypeRef;
}

#[cfg(target_os = "macos")]
fn trusted_with_prompt(prompt: bool) -> bool {
    if !prompt {
        return unsafe { AXIsProcessTrusted() != 0 };
    }

    let dict = unsafe {
        CFDictionaryCreateMutable(std::ptr::null(), 1, std::ptr::null(), std::ptr::null())
    };
    if dict.is_null() {
        return unsafe { AXIsProcessTrusted() != 0 };
    }

    unsafe {
        CFDictionaryAddValue(
            dict,
            kAXTrustedCheckOptionPrompt as *const c_void,
            kCFBooleanTrue as *const c_void,
        );
    }

    let trusted = unsafe { AXIsProcessTrustedWithOptions(dict as CFDictionaryRef) != 0 };
    unsafe {
        CFRelease(dict as CFTypeRef);
    }
    trusted
}

pub fn has_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        return trusted_with_prompt(false);
    }

    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn request_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        return trusted_with_prompt(true);
    }

    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn send_command_v() -> bool {
    #[cfg(target_os = "macos")]
    {
        let source = unsafe { CGEventSourceCreate(K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE) };
        if source.is_null() {
            return false;
        }

        let key_down = unsafe { CGEventCreateKeyboardEvent(source, V_KEY_CODE, 1) };
        let key_up = unsafe { CGEventCreateKeyboardEvent(source, V_KEY_CODE, 0) };
        if key_down.is_null() || key_up.is_null() {
            if !key_down.is_null() {
                unsafe { CFRelease(key_down as CFTypeRef) };
            }
            if !key_up.is_null() {
                unsafe { CFRelease(key_up as CFTypeRef) };
            }
            unsafe { CFRelease(source as CFTypeRef) };
            return false;
        }

        unsafe {
            CGEventSetFlags(key_down, K_CG_EVENT_FLAG_MASK_COMMAND);
            CGEventSetFlags(key_up, K_CG_EVENT_FLAG_MASK_COMMAND);
            CGEventPost(K_CG_HID_EVENT_TAP, key_down);
            CGEventPost(K_CG_HID_EVENT_TAP, key_up);
            CFRelease(key_down as CFTypeRef);
            CFRelease(key_up as CFTypeRef);
            CFRelease(source as CFTypeRef);
        }
        true
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}
