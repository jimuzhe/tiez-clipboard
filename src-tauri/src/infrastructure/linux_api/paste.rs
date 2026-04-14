pub fn simulate_paste() -> Result<(), String> {
    std::process::Command::new("xdotool")
        .args([&"key", &"--clearmodifiers", &"ctrl+v"])
        .spawn()
        .map_err(|e| format!("xdotool 粘贴失败: {}", e))?;
    Ok(())
}

pub fn simulate_paste_sequence(texts: Vec<String>) -> Result<(), String> {
    for (i, text) in texts.iter().enumerate() {
        set_clipboard_text(text)?;
        simulate_paste()?;

        if i < texts.len() - 1 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    Ok(())
}

fn set_clipboard_text(text: &str) -> Result<(), String> {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        clipboard
            .set_text(text.to_string())
            .map_err(|e| format!("设置剪贴板失败: {}", e))?;
    }
    Ok(())
}
