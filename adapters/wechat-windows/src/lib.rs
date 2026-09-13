//! # Windows 微信 4.x 适配器
//!
//! 提供专为 Windows 微信 4.x (`xwechat_files`) 设计的目录探测、多账号识别、
//! 文件/视频元数据批量解析，以及聊天图片离线解密所需的参数发现与格式解析。

pub mod detector;
pub mod media;
pub mod parser;

pub use detector::{WeChat4Detector, WeChatAccount};
pub use parser::WeChat4Parser;
