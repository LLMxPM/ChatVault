// ChatVault 归档/同步进度回调：供桌面事件推送与运行日志写入复用。

/// 归档循环进度与关键明细回调。
pub trait ArchiveProgressSink: Send {
    /// 阶段切换或状态变化
    fn on_stage(&mut self, _stage: &str, _status: &str, _message: Option<&str>) {}
    /// 文件进度：done 已处理，total 当前批次总量，current 当前文件名
    fn on_progress(&mut self, _stage: &str, _done: usize, _total: usize, _current: Option<&str>) {}
    /// 关键明细（失败/缺失等）
    #[allow(clippy::too_many_arguments)]
    fn on_item(
        &mut self,
        _stage: &str,
        _task_id: Option<&str>,
        _record_id: Option<&str>,
        _name: &str,
        _status: &str,
        _error: Option<&str>,
        _size: Option<u64>,
    ) {
    }
}

/// 空实现：无 UI 时使用
pub struct NoopProgressSink;

impl ArchiveProgressSink for NoopProgressSink {}
