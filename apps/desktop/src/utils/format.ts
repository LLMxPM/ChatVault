// ChatVault 前端展示格式化工具
// 统一时间、时长等用户可见数值的呈现，避免各视图各写一套

/**
 * 将 RFC3339 / ISO 时间字符串格式化为本地可读时间。
 *
 * 输入为空或无法解析时返回 fallback。
 * 默认精度到分钟：`YYYY-MM-DD HH:mm`；同年可省略日期为 `HH:mm`（useCompact）。
 */
export function formatDateTime(
  iso?: string | number | null,
  options: { fallback?: string; withSeconds?: boolean; compact?: boolean } = {},
): string {
  const { fallback = "未知", withSeconds = false, compact = false } = options;
  if (iso === null || iso === undefined || iso === "") return fallback;

  const date = typeof iso === "number" ? new Date(iso) : new Date(String(iso));
  if (Number.isNaN(date.getTime())) return fallback;

  const pad = (n: number) => String(n).padStart(2, "0");
  const y = date.getFullYear();
  const mo = pad(date.getMonth() + 1);
  const d = pad(date.getDate());
  const h = pad(date.getHours());
  const mi = pad(date.getMinutes());
  const s = pad(date.getSeconds());

  if (compact) {
    const now = new Date();
    if (now.getFullYear() === y && now.getMonth() === date.getMonth() && now.getDate() === date.getDate()) {
      return withSeconds ? `${h}:${mi}:${s}` : `${h}:${mi}`;
    }
  }

  const time = withSeconds ? `${h}:${mi}:${s}` : `${h}:${mi}`;
  return `${y}-${mo}-${d} ${time}`;
}

/** 将毫秒时长格式化为紧凑可读文本，如 `1.2 秒`、`3 分 5 秒`。 */
export function formatDurationMs(ms?: number | null): string {
  if (ms === null || ms === undefined || Number.isNaN(ms) || ms < 0) return "—";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  const totalSec = ms / 1000;
  if (totalSec < 60) {
    return `${totalSec >= 10 ? Math.round(totalSec) : totalSec.toFixed(1)} 秒`;
  }
  const totalMin = Math.floor(totalSec / 60);
  const sec = Math.round(totalSec % 60);
  if (totalMin < 60) {
    return sec === 0 ? `${totalMin} 分` : `${totalMin} 分 ${sec} 秒`;
  }
  const hours = Math.floor(totalMin / 60);
  const min = totalMin % 60;
  return min === 0 ? `${hours} 小时` : `${hours} 小时 ${min} 分`;
}
