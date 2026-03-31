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
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    WM_CLIPBOARDUPDATE, WNDCLASSW, MSG, GWLP_USERDATA, SetWindowLongPtrW, GetWindowLongPtrW,
    HWND_MESSAGE,
};
#[cfg(target_os = "linux")]
use crate::infrastructure::linux_api::{detect_display_server, DisplayServer};

pub fn listen_clipboard(callback: Arc<dyn Fn() + Send + Sync + 'static>) {
    #[cfg(target_os = "windows")]
    std::thread::spawn(move || {
        unsafe {
            let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap();
            let window_class = "TieZClipboardListener";
            let window_class_w: Vec<u16> = window_class.encode_utf16().chain(std::iter::once(0)).collect();

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
                0, 0, 0, 0,
                Some(HWND_MESSAGE), // Use HWND_MESSAGE for invisible message-only window
                None,
                Some(HINSTANCE(instance.0)),
                None,
            ) {
                Ok(hwnd) => hwnd,
                Err(e) => {
                    eprintln!("[ERROR] Failed to create clipboard listener window: {:?}", e);
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

    #[cfg(target_os = "linux")]
    std::thread::spawn(move || {
        let display_server = detect_display_server();

        match display_server {
            DisplayServer::X11 => {
                listen_clipboard_x11(callback);
            }
            DisplayServer::Wayland | DisplayServer::Unknown => {
                listen_clipboard_polling(callback);
            }
        }
    });

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    std::thread::spawn(move || {
        listen_clipboard_polling(callback);
    });
}

#[cfg(target_os = "linux")]
fn listen_clipboard_x11(callback: Arc<dyn Fn() + Send + Sync + 'static>) {
    use std::hash::{Hash, Hasher};
    use x11rb::connection::{Connection, RequestConnection};
    use x11rb::protocol::xfixes::{ConnectionExt as XfixesConnectionExt, SelectionEventMask};
    use x11rb::protocol::xproto::ConnectionExt as XprotoConnectionExt;

    let result: Result<(), Box<dyn std::error::Error>> = (|| {
        let (conn, screen_num) = x11rb::connect(None)?;
        let setup = conn.setup();
        let screen = setup.roots.get(screen_num).ok_or("No screen found")?;
        let root = screen.root;

        let clipboard_atom = conn.intern_atom(false, b"CLIPBOARD")?.reply()?.atom;
        let xfixes_ver = conn.xfixes_query_version(1, 0)?.reply()?;

        let mask = SelectionEventMask::SET_SELECTION_OWNER;
        conn.xfixes_select_selection_input(root, clipboard_atom, mask)?;
        conn.flush()?;

        println!(">>> [CLIPBOARD] Linux X11 hybrid listener started (XFixes {} + file polling).", xfixes_ver.major_version);

        let xfixes_first_event = conn
            .extension_information(x11rb::protocol::xfixes::X11_EXTENSION_NAME)?
            .map(|info| info.first_event)
            .unwrap_or(0);
        let selection_notify_code = xfixes_first_event + x11rb::protocol::xfixes::SELECTION_NOTIFY_EVENT;

        // Seed clipboard text hash to avoid false-trigger on startup.
        // IMPORTANT: We use arboard::get_text() here instead of x11_clipboard's
        // get_clipboard_files() because x11_clipboard creates its own X11 connection
        // which interferes with the callback's subsequent get_clipboard_files() call
        // (the owner's SelectionRequest responses get routed to the wrong connection).
        let mut last_text_hash: u64 = {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                if let Ok(text) = cb.get_text() {
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    text.hash(&mut h);
                    h.finish()
                } else {
                    0
                }
            } else {
                0
            }
        };

        loop {
            // Drain pending X11 selection-owner-change events (fast path)
            while let Ok(Some(event)) = conn.poll_for_event() {
                if event.response_type() == selection_notify_code {
                    callback();
                }
            }

            // Slow-path polling: XFixes SET_SELECTION_OWNER fires only when the
            // owner *changes*, so re-copying files inside the same app (e.g.
            // Nautilus) produces no event.  Use arboard (lightweight, separate
            // from x11_clipboard) to detect text content changes which include
            // file:// URIs when files are copied.
            if let Ok(mut cb) = arboard::Clipboard::new() {
                if let Ok(text) = cb.get_text() {
                    if !text.is_empty() {
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        text.hash(&mut h);
                        let hash = h.finish();
                        if hash != last_text_hash {
                            last_text_hash = hash;
                            callback();
                        }
                    }
                }
            }

            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        Ok(())
    })();

    if let Err(e) = result {
        eprintln!("[WARN] X11 clipboard listener failed: {:?}, falling back to polling", e);
        listen_clipboard_polling(callback);
    }
}

#[cfg(any(target_os = "linux", not(any(target_os = "windows", target_os = "linux"))))]
fn listen_clipboard_polling(callback: Arc<dyn Fn() + Send + Sync + 'static>) {
    use std::hash::{Hash, Hasher};

    let mut last_hash = 0u64;
    loop {
        let mut current_hash = 0u64;

        // Try to get text content hash
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                if !text.is_empty() {
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    text.hash(&mut hasher);
                    current_hash = hasher.finish();
                }
            }
        }

        // If no text, try to get image hash (important for browser image copies)
        if current_hash == 0 {
            #[cfg(target_os = "linux")]
            {
                unsafe {
                    if let Some(image) = crate::infrastructure::linux_api::win_clipboard::get_clipboard_image() {
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        // Hash image dimensions and sample bytes
                        image.width.hash(&mut hasher);
                        image.height.hash(&mut hasher);
                        if !image.bytes.is_empty() {
                            image.bytes[0].hash(&mut hasher);
                            if image.bytes.len() > 100 {
                                image.bytes[100].hash(&mut hasher);
                            }
                            image.bytes[image.bytes.len() - 1].hash(&mut hasher);
                        }
                        current_hash = hasher.finish();
                    }
                }
            }
        }

        // If no text or image, try to get file hash
        if current_hash == 0 {
            #[cfg(target_os = "linux")]
            {
                unsafe {
                    if let Some(files) = crate::infrastructure::linux_api::win_clipboard::get_clipboard_files() {
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        for file in &files {
                            file.hash(&mut hasher);
                        }
                        current_hash = hasher.finish();
                    }
                }
            }
        }

        if current_hash != 0 && current_hash != last_hash {
            last_hash = current_hash;
            callback();
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
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
