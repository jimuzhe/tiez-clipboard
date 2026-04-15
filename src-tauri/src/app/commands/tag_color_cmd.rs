use crate::database::DbState;
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::tag_repo::TagRepository;
use std::collections::HashMap;
use tauri::State;

#[tauri::command]
pub fn set_tag_color(
    state: State<'_, DbState>,
    name: String,
    color: Option<String>,
) -> AppResult<()> {
    state
        .tag_repo
        .set_color(&name, color)
        .map_err(AppError::from)
}

#[tauri::command]
pub fn get_tag_colors(state: State<'_, DbState>) -> AppResult<HashMap<String, String>> {
    state.tag_repo.get_colors().map_err(AppError::from)
}
