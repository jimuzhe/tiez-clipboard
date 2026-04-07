use std::path::PathBuf;

/// v0.2.8 Rename Migration: 贴汁 -> TieZ
pub fn perform_migration_v028(default_app_dir: &PathBuf) {
    // macOS migration logic from old "贴汁" folder
    // On macOS, Tauri's app_data_dir is usually Library/Application Support/com.tiez
    // Let's check if there's a legacy folder in Application Support

    let mut old_app_dirs_to_check = Vec::new();

    if let Some(parent) = default_app_dir.parent() {
        // Try looking for "贴汁" or "tie-z" in the same Application Support folder
        old_app_dirs_to_check.push(parent.join("贴汁"));
        old_app_dirs_to_check.push(parent.join("tie-z"));
        old_app_dirs_to_check.push(parent.join("com.tiez.app"));
    }

    // Try each possible location
    for old_app_dir in old_app_dirs_to_check {
        if old_app_dir.exists() && old_app_dir.is_dir() {
            println!(
                ">>> [MIGRATION] Found old data folder at: {:?}",
                old_app_dir
            );
            let new_db = default_app_dir.join("clipboard.db");
            let old_db = old_app_dir.join("clipboard.db");

            let mut success = false;

            // 1. Data Migration Logic
            if !default_app_dir.exists() {
                println!(
                    ">>> [MIGRATION] Renaming old data folder {:?} to 'TieZ'...",
                    old_app_dir
                );
                success = std::fs::rename(&old_app_dir, &default_app_dir).is_ok();
            } else if old_db.exists() && !new_db.exists() {
                println!(
                    ">>> [MIGRATION] Pulling old data from {:?} to 'TieZ'...",
                    old_app_dir
                );
                let _ = std::fs::create_dir_all(&default_app_dir);
                if std::fs::copy(&old_db, &new_db).is_ok() {
                    success = true;
                    let old_log = old_app_dir.join("tiez.log");
                    if old_log.exists() {
                        let _ = std::fs::copy(&old_log, default_app_dir.join("tiez.log"));
                    }
                }
            } else if old_db.exists() && new_db.exists() {
                let old_size = std::fs::metadata(&old_db).map(|m| m.len()).unwrap_or(0);
                let new_size = std::fs::metadata(&new_db).map(|m| m.len()).unwrap_or(0);

                if old_size > new_size && new_size < 50_000 {
                    println!(">>> [MIGRATION] Old database ({} bytes) has more data than new ({} bytes). Replacing...", old_size, new_size);
                    let backup_db = default_app_dir.join("clipboard.db.backup");
                    let _ = std::fs::rename(&new_db, &backup_db);

                    if std::fs::copy(&old_db, &new_db).is_ok() {
                        success = true;
                        println!(">>> [MIGRATION] Successfully migrated old database to TieZ.");
                    } else {
                        let _ = std::fs::rename(&backup_db, &new_db);
                    }
                } else {
                    success = true;
                }
            } else {
                success = true;
            }

            if success {
                println!(">>> [CLEANUP] Cleaning up residues of old version...");
                if old_app_dir.exists() {
                    let _ = std::fs::remove_dir_all(&old_app_dir);
                }
            }
        }
    }
}

pub fn cleanup_old_install_registry() -> Option<PathBuf> {
    None
}

pub fn cleanup_old_start_menu() {}

pub fn enable_autostart_manually(_app_name: &str, _exe_path: &str) -> std::io::Result<()> {
    Ok(())
}

pub fn cleanup_old_install_folder(_custom_path: Option<PathBuf>) {}
