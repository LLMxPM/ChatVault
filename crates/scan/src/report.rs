// ChatVault 扫描入库计数与请求参数。
use crate::accounts::AccountTarget;
use crate::events::ScanEvent;

/// 一轮扫描汇总。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanReport {
    /// 发现的候选文件数（含合并进来的已知变更）。
    pub discovered: usize,
    /// 本轮新写入来源记录数。
    pub indexed: usize,
    /// 本轮新增内容对象数（按内容哈希去重后）。
    pub new_objects: usize,
    /// 幂等跳过（未变更或同内容重复记录）。
    pub skipped: usize,
}

impl ScanReport {
    pub fn merge(&mut self, other: ScanReport) {
        self.discovered += other.discovered;
        self.indexed += other.indexed;
        self.new_objects += other.new_objects;
        self.skipped += other.skipped;
    }
}

/// 扫描编排请求。
pub struct ScanRequest<'a> {
    pub device_id: &'a str,
    /// true 时忽略检查点做全量发现。
    pub full_scan: bool,
    /// 微信/企业微信账号勾选；None 表示全选，Some(&[]) 表示不扫账号。
    pub target_accounts: Option<&'a [AccountTarget]>,
    /// 本轮扫描开始时刻（毫秒）；完整候选处理成功后写入检查点。
    pub scan_started_ms: i64,
    /// 返回 true 时在来源/账号边界协作取消。
    pub should_cancel: Option<&'a dyn Fn() -> bool>,
    /// 进度事件回调。
    pub on_event: Option<&'a dyn Fn(ScanEvent)>,
    /// 单个采集源扫描完成后的累计报告回调。
    pub on_source_done: Option<&'a dyn Fn(&str, ScanReport)>,
}

impl<'a> ScanRequest<'a> {
    pub fn emit(&self, event: ScanEvent) {
        if let Some(on) = self.on_event {
            on(event);
        }
    }

    pub fn cancelled(&self) -> bool {
        self.should_cancel.map(|f| f()).unwrap_or(false)
    }

    pub fn notify_source_done(&self, path: &str, report: ScanReport) {
        if let Some(on) = self.on_source_done {
            on(path, report);
        }
    }
}

/// 账号勾选集合是否包含指定账号。
pub(crate) fn account_selected(
    targets: Option<&[AccountTarget]>,
    root: &std::path::Path,
    account_id: &str,
) -> bool {
    crate::accounts::is_selected_account(targets, root, account_id)
}
