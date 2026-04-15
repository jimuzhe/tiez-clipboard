use std::sync::Arc;
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowLongPtrW,
    RegisterClassW, SetWindowLongPtrW, GWLP_USERDATA, HWND_MESSAGE, MSG, WM_CLIPBOARDUPDATE,
    WNDCLASSW,
};

pub fn listen_clipboard(callback: Arc<dyn Fn() + Send + Sync + 'static>) {
    #[cfg(target_os = "windows")]
    std::thread::spawn(move || {
        unsafe {
            let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap();
            let window_class = "TieZClipboardListener";
            let window_class_w: Vec<u16> = window_class
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let wnd_class = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: instance.into(),
                lpszClassName: PCWSTR(window_class_w.as_ptr()),
                ..Default::default()
            };

            RegisterClassW(&wnd_class);

            let hwnd = match CreateWindowExW(
                Default::default(),
                PCWSTR(window_class_w.as_ptr()),
                PCWSTR(std::ptr::null()),
                Default::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE), // Use HWND_MESSAGE for invisible message-only window
                None,
                Some(HINSTANCE(instance.0)),
                None,
            ) {
                Ok(hwnd) => hwnd,
                Err(e) => {
                    eprintln!(
                        "[ERROR] Failed to create clipboard listener window: {:?}",
                        e
                    );
                    return;
                }
            };

            // Wrap callback in a Box to store in window user data
            let boxed_callback = Box::new(callback);
            let ptr = Box::into_raw(boxed_callback);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);

            if let Err(e) = AddClipboardFormatListener(hwnd) {
                eprintln!("[ERROR] Failed to add clipboard listener: {:?}", e);
                let _ = Box::from_raw(ptr);
                return;
            }

            println!(">>> [CLIPBOARD] Windows event-driven listener started.");

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                DispatchMessageW(&msg);
            }

            let _ = RemoveClipboardFormatListener(hwnd);
            // Cleanup callback
            let _ = Box::from_raw(ptr);
        }
    });

    #[cfg(not(target_os = "windows"))]
    std::thread::spawn(move || {
        let mut last_hash = 0u64;
        let mut clipboard = arboard::Clipboard::new().unwrap();
        loop {
            // Very primitive polling, relies on higher layers to deduplicate properly.
            if let Ok(text) = clipboard.get_text() {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                text.hash(&mut hasher);
                let current_hash = hasher.finish();
                if current_hash != last_hash {
                    last_hash = current_hash;
                    callback();
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CLIPBOARDUPDATE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if ptr != 0 {
                let callback = &*(ptr as *const Arc<dyn Fn() + Send + Sync + 'static>);
                callback();
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
