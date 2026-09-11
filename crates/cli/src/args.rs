// ChatVault CLI 参数契约：扫描、归档与同步子命令的输入定义。
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "chatvault-cli",
    version = "0.1.0",
    about = "ChatVault 桌面归档检索验证工具"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// 自动探测 Windows 微信 4.x (xwechat_files) 存储目录与账号列表
    Detect,

    /// 扫描微信 4.x 或指定文件夹并增量入库 SQLite
    Scan {
        /// 扫描目标: "wechat" 代表自动探测微信 4.x，或传入指定本地文件夹路径
        #[arg(short, long, default_value = "wechat")]
        target: String,

        /// 本地 SQLite 数据库路径
        #[arg(short, long, default_value = "chatvault.db")]
        db: PathBuf,

        /// 当前设备标识
        #[arg(long, default_value = "")]
        device_id: String,
    },

    /// 在本地 SQLite 数据库中检索文件 (支持中文 FTS5 全文搜索与过滤)
    Search {
        /// 检索关键词 (中文、英文、数字均可)
        keyword: Option<String>,

        /// 按扩展名筛选 (如 pdf, docx, mp4)
        #[arg(short, long)]
        ext: Option<String>,

        /// 按账号筛选
        #[arg(short, long)]
        account: Option<String>,

        /// 返回数量限制
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// 数据库文件路径
        #[arg(short, long, default_value = "chatvault.db")]
        db: PathBuf,
    },

    /// 测试 WebDAV 服务连通性与 RFC4918 核心能力
    WebdavTest {
        /// WebDAV 服务根地址，例如 https://dav.example.com
        #[arg(long)]
        url: String,

        /// 认证用户名
        #[arg(long)]
        user: Option<String>,

        /// 认证密码
        #[arg(long)]
        pass: Option<String>,
    },

    /// 执行端到端归档：从 SQLite 取待归档任务并上传到 WebDAV，执行远端回读哈希校验
    Archive {
        /// WebDAV 服务地址
        #[arg(long)]
        url: String,

        /// 认证用户名
        #[arg(long)]
        user: Option<String>,

        /// 认证密码
        #[arg(long)]
        pass: Option<String>,

        /// 目标 Vault 标识
        #[arg(long, default_value = "default-vault")]
        vault_id: String,

        /// 本地 SQLite 数据库路径
        #[arg(short, long, default_value = "chatvault.db")]
        db: PathBuf,

        /// 最大上传任务数量 (0 表示全部处理)
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// 定时任务入口：从本地设置读取采集目录与 WebDAV 参数，执行扫描并归档
    ScheduledRun {
        /// 本地 SQLite 数据库路径
        #[arg(short, long, default_value = "chatvault.db")]
        db: PathBuf,
    },

    /// 发布本机未推送的元数据日志事件到 WebDAV
    SyncPublish {
        #[arg(long)]
        url: String,
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        pass: Option<String>,
        #[arg(long, default_value = "default-vault")]
        vault_id: String,
        #[arg(short, long, default_value = "chatvault.db")]
        db: PathBuf,
        #[arg(long, default_value = "")]
        device_id: String,
    },

    /// 从 WebDAV 拉取其他设备日志并合并到本地索引
    SyncPull {
        #[arg(long)]
        url: String,
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        pass: Option<String>,
        #[arg(long, default_value = "default-vault")]
        vault_id: String,
        #[arg(short, long, default_value = "chatvault.db")]
        db: PathBuf,
        #[arg(long, default_value = "")]
        device_id: String,
    },

    /// 空索引恢复：从 WebDAV 拉取全部元数据重建本地索引
    Restore {
        #[arg(long)]
        url: String,
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        pass: Option<String>,
        #[arg(long, default_value = "default-vault")]
        vault_id: String,
        #[arg(short, long, default_value = "chatvault.db")]
        db: PathBuf,
        #[arg(long, default_value = "")]
        device_id: String,
    },
}
