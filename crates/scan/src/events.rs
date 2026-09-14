// ChatVault 扫描编排进度事件：桌面与 CLI 据此输出日志/进度。
use crate::accounts::SourceAccountInfo;

/// 媒体根类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// 文档附件目录（含会话子目录的来源）。
    Files,
    /// 视频本体目录。
    Videos,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MediaKind::Files => "file",
            MediaKind::Videos => "video",
        }
    }
}

/// 扫描过程事件；桌面可用 tracing，CLI 直接打印。
#[derive(Debug, Clone)]
pub enum ScanEvent {
    SourceStart {
        source_type: String,
        path: String,
    },
    UnknownSourceType {
        source_type: String,
        path: String,
    },
    SourceMissing {
        path: String,
        label: &'static str,
    },
    AccountListed {
        accounts: Vec<SourceAccountInfo>,
    },
    AccountSkipped {
        account_id: String,
    },
    AccountStart {
        account_id: String,
    },
    MediaRootStart {
        kind: MediaKind,
        path: String,
    },
    MediaRootCandidates {
        kind: MediaKind,
        path: String,
        count: usize,
    },
    MediaRootIncomplete {
        path: String,
    },
    VideosDisabled {
        account_id: String,
    },
}
