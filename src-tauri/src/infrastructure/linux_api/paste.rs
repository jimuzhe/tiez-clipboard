pub fn simulate_paste() -> Result<(), String> {
    simulate_paste_with_method("ctrl_v", None)
}

pub fn simulate_paste_with_method(method: &str, content_type: Option<&str>) -> Result<(), String> {
    let key_combo = match resolve_effective_paste_method(method, content_type) {
        "ctrl_v" => "ctrl+v",
        _ => "Shift+Insert",
    };

    let status = std::process::Command::new("xdotool")
        .args([&"key", &"--clearmodifiers", key_combo])
        .status()
        .map_err(|e| format!("xdotool 粘贴失败: {}", e))?;

    if !status.success() {
        return Err(format!("xdotool 粘贴失败，退出码: {:?}", status.code()));
    }

    Ok(())
}

pub fn simulate_paste_sequence(texts: Vec<String>) -> Result<(), String> {
    for (i, text) in texts.iter().enumerate() {
        set_clipboard_text(text)?;
        simulate_paste_with_method("shift_insert", Some("text"))?;

        if i < texts.len() - 1 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    Ok(())
}

fn set_clipboard_text(text: &str) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("初始化剪贴板失败: {}", e))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("设置剪贴板失败: {}", e))?;
    Ok(())
}

fn resolve_effective_paste_method(method: &str, content_type: Option<&str>) -> &'static str {
    let text_like = matches!(content_type, Some("text" | "code" | "url" | "rich_text"));

    if method == "ctrl_v" {
        "ctrl_v"
    } else if text_like {
        "shift_insert"
    } else {
        "ctrl_v"
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_effective_paste_method;

    #[test]
    fn keeps_explicit_ctrl_v() {
        assert_eq!(
            resolve_effective_paste_method("ctrl_v", Some("text")),
            "ctrl_v"
        );
    }

    #[test]
    fn prefers_shift_insert_for_text_like_content() {
        assert_eq!(
            resolve_effective_paste_method("shift_insert", Some("rich_text")),
            "shift_insert"
        );
    }

    #[test]
    fn keeps_ctrl_v_for_file_like_content() {
        assert_eq!(
            resolve_effective_paste_method("shift_insert", Some("file")),
            "ctrl_v"
        );
        assert_eq!(
            resolve_effective_paste_method("shift_insert", Some("image")),
            "ctrl_v"
        );
    }
}
