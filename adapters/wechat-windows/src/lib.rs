//! # Windows 微信 4.x 适配器
//!
//! 提供专为 Windows 微信 4.x (`xwechat_files`) 设计的目录探测、多账号识别
//! 与文件元数据批量解析提取功能。

pub mod detector;
pub mod parser;

pub use detector::{WeChat4Detector, WeChatAccount};
pub use parser::WeChat4Parser;
