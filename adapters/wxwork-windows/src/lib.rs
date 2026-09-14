//! # Windows 企业微信适配器
//!
//! 提供 Windows 企业微信 (`WXWork`) 数据根探测、多账号识别，
//! 以及附件/视频目录的批量解析。

pub mod detector;
pub mod parser;

pub use detector::{WxWorkAccount, WxWorkDetector};
pub use parser::WxWorkParser;
