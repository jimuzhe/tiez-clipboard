#[cfg(target_os = "macos")]
use clipboard_rs::{ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext};
use std::sync::Arc;

#[cfg(target_os = "macos")]
const MIN_MACOS_LISTENER_INTERVAL_MS: u64 = 120;

#[cfg(target_os = "macos")]
struct MacClipboardHandler {
    callback: Arc<dyn Fn() + Send + Sync + 'static>,
    last_callback_at_ms: u64,
}

#[cfg(target_os = "macos")]
impl ClipboardHandler for MacClipboardHandler {
    fn on_clipboard_change(&mut self) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if now_ms.saturating_sub(self.last_callback_at_ms) < MIN_MACOS_LISTENER_INTERVAL_MS {
            return;
        }

        self.last_callback_at_ms = now_ms;
        (self.callback)();
    }
}

pub fn listen_clipboard(callback: Arc<dyn Fn() + Send + Sync + 'static>) {
    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(move || {
            let mut watcher = match ClipboardWatcherContext::new() {
                Ok(w) => w,
                Err(err) => {
                    eprintln!(">>> [CLIPBOARD] failed to create mac watcher: {}", err);
                    return;
                }
            };

            watcher.add_handler(MacClipboardHandler {
                callback,
                last_callback_at_ms: 0,
            });
            watcher.start_watch();
        });
        return;
    }

    #[cfg(not(target_os = "macos"))]
    std::thread::spawn(move || {
        let mut last_seq =
            crate::infrastructure::macos_api::clipboard::get_clipboard_sequence_number();
        loop {
            let current_seq =
                crate::infrastructure::macos_api::clipboard::get_clipboard_sequence_number();
            if current_seq != last_seq {
                println!(
                    ">>> [CLIPBOARD] macOS change detected (seq: {})",
                    current_seq
                );
                last_seq = current_seq;
                callback();
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

// Windows-specific wnd_proc path removed
