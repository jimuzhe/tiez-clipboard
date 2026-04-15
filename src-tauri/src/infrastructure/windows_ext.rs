use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Threading::AttachThreadInput;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_LWIN,
    VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, MessageBoxW, SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOPMOST,
    MB_ICONERROR, MB_OK, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_RESTORE,
    SW_SHOWNA,
};

/// 安全封装的窗口辅助工具
pub struct WindowExt;

impl WindowExt {
    /// 获取当前前台窗口句柄
    pub fn get_foreground_window() -> HWND {
        unsafe { GetForegroundWindow() }
    }

    /// 检查窗口是否可见
    pub fn is_window_visible(hwnd: HWND) -> bool {
        unsafe { IsWindowVisible(hwnd).as_bool() }
    }

    /// 获取窗口矩形区域
    pub fn get_window_rect(hwnd: HWND) -> Option<RECT> {
        let mut rect = RECT::default();
        unsafe {
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                Some(rect)
            } else {
                None
            }
        }
    }

    /// 释放 Windows 键（防止开始菜单弹出）
    pub fn release_win_keys() {
        unsafe {
            let dummy_vk = VIRTUAL_KEY(0xFF);
            let inputs = [
                Self::create_key_input(dummy_vk, false),
                Self::create_key_input(dummy_vk, true),
                Self::create_key_input(VK_LWIN, true),
                Self::create_key_input(VK_RWIN, true),
            ];
            SendInput(&inputs, core::mem::size_of::<INPUT>() as i32);
        }
    }

    /// 强力恢复窗口焦点（处理跨线程输入附加）
    pub fn force_focus_window(hwnd: HWND) {
        if hwnd.0.is_null() {
            return;
        }

        unsafe {
            if !IsWindowVisible(hwnd).as_bool() {
                return;
            }
            let should_restore = IsIconic(hwnd).as_bool();

            let fg_hwnd = GetForegroundWindow();
            if fg_hwnd != hwnd {
                let fg_thread_id = GetWindowThreadProcessId(fg_hwnd, None);
                let target_thread_id = GetWindowThreadProcessId(hwnd, None);

                if fg_thread_id != 0 && target_thread_id != 0 && fg_thread_id != target_thread_id {
                    let _ = AttachThreadInput(fg_thread_id, target_thread_id, true);
                    let _ = SetForegroundWindow(hwnd);
                    if should_restore {
                        let _ = ShowWindow(hwnd, SW_RESTORE);
                    }
                    let _ = BringWindowToTop(hwnd);
                    let _ = AttachThreadInput(fg_thread_id, target_thread_id, false);
                } else {
                    let _ = SetForegroundWindow(hwnd);
                    if should_restore {
                        let _ = ShowWindow(hwnd, SW_RESTORE);
                    }
                    let _ = BringWindowToTop(hwnd);
                }
            }
        }
    }

    /// 无感显示置顶窗口（不夺取焦点）
    pub fn show_window_no_activate(hwnd: HWND) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNA);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_NOACTIVATE,
            );
        }
    }

    /// 无激活显示普通窗口（不置顶）
    pub fn show_window_no_activate_normal(hwnd: HWND) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNA);
            // Bring to front without activation by temporarily toggling TOPMOST.
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_NOACTIVATE,
            );
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_NOACTIVATE,
            );
        }
    }

    /// 弹出错误消息框
    pub fn show_error_box(title: &str, msg: &str) {
        use windows::core::PCWSTR;
        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let msg_w: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            let _ = MessageBoxW(
                None,
                PCWSTR(msg_w.as_ptr()),
                PCWSTR(title_w.as_ptr()),
                MB_ICONERROR | MB_OK,
            );
        }
    }

    fn create_key_input(vk: VIRTUAL_KEY, is_up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    dwFlags: if is_up {
                        KEYEVENTF_KEYUP
                    } else {
                        windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
                    },
                    ..Default::default()
                },
            },
        }
    }
}
