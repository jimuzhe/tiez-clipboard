use serde::{Serialize, Serializer};
use std::fmt;

#[derive(Debug)]
pub enum AppError {
    Database(String),
    IO(String),
    Network(String),
    Internal(String),
    Validation(String),
    Encryption(String),
}

impl std::error::Error for AppError {}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Database(e) => write!(f, "数据库错误: {}", e),
            AppError::IO(e) => write!(f, "文件系统错误: {}", e),
            AppError::Network(e) => write!(f, "网络错误: {}", e),
            AppError::Internal(e) => write!(f, "内部系统错误: {}", e),
            AppError::Validation(e) => write!(f, "验证错误: {}", e),
            AppError::Encryption(e) => write!(f, "加密错误: {}", e),
        }
    }
}

// 允许 AppError 被序列化，以便直接返回给 Tauri 前端
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// 通用转换宏或实现，便于从 common errors 转换过来
impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::IO(err.to_string())
    }
}

impl From<String> for AppError {
    fn from(err: String) -> Self {
        AppError::Internal(err)
    }
}

impl From<arboard::Error> for AppError {
    fn from(err: arboard::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(err: tauri::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<image::ImageError> for AppError {
    fn from(err: image::ImageError) -> Self {
        AppError::IO(err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
