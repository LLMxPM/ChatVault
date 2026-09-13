// ChatVault 同步测试夹具：仅监听回环地址的 WebDAV 模拟器，支持请求失败与断点重试。
use chatvault_core::models::{JournalEvent, JournalEventType};
use chatvault_index::Database;
use chatvault_webdav::{WebDavClient, WebDavConfig};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Default)]
pub struct Storage {
    pub files: HashMap<String, Vec<u8>>,
    pub calls: HashMap<String, usize>,
    pub fail_on: HashMap<String, usize>,
    pub fail_after_put: bool,
}

pub struct Server {
    pub client: WebDavClient,
    pub state: Arc<Mutex<Storage>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    /// 测试完成后关闭模拟服务，不留下后台线程。
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    /// 创建隔离 WebDAV；可选择命名空间前缀验证服务兼容性。
    pub async fn new(prefix: &str) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(Storage::default()));
        let shared = state.clone();
        let prefix = prefix.to_string();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut buf = [0u8; 4096];
                    let n = socket.read(&mut buf).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(p) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        break p + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..header_end]).to_string();
                let first: Vec<_> = headers.lines().next().unwrap().split_whitespace().collect();
                let method = first[0];
                let path = first[1];
                let length = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|s| s.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                while bytes.len() < header_end + length {
                    let mut buf = [0u8; 4096];
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                }
                let (status, body) = {
                    let mut store = shared.lock().unwrap();
                    let key = format!("{method} {path}");
                    let count = store.calls.entry(key.clone()).or_default();
                    *count += 1;
                    let count = *count;
                    let fail = store.fail_on.get(&key) == Some(&count);
                    if fail && !(method == "PUT" && store.fail_after_put) {
                        (503, Vec::new())
                    } else {
                        let mut result = respond(
                            &mut store,
                            method,
                            path,
                            &headers,
                            &bytes[header_end..],
                            &prefix,
                        );
                        if fail {
                            result = (503, Vec::new());
                        }
                        result
                    }
                };
                socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
                socket.write_all(&body).await.unwrap();
            }
        });
        Self {
            client: WebDavClient::new(WebDavConfig {
                base_url: format!("http://{address}"),
                username: None,
                password: None,
            })
            .unwrap(),
            state,
            task,
        }
    }

    /// 预置真实哈希对应的对象内容。
    pub fn seed_object(&self) {
        let path = chatvault_metadata::get_object_path("chatvault-v", &hash());
        self.state
            .lock()
            .unwrap()
            .files
            .insert(format!("/{path}"), b"content".to_vec());
    }

    /// 指定某次请求返回错误；用于模拟上传成功但响应丢失等窗口。
    pub fn fail(&self, method: &str, path: &str, n: usize, after_put: bool) {
        let mut s = self.state.lock().unwrap();
        s.fail_on.insert(format!("{method} /{path}"), n);
        s.fail_after_put = after_put;
    }
}

/// 处理模拟协议，目录来自已存文件路径，支持条件写入和禁止覆盖 MOVE。
fn respond(
    store: &mut Storage,
    method: &str,
    path: &str,
    headers: &str,
    body: &[u8],
    prefix: &str,
) -> (u16, Vec<u8>) {
    match method {
        "MKCOL" => (201, vec![]),
        "HEAD" => (
            if store.files.contains_key(path) {
                200
            } else {
                404
            },
            vec![],
        ),
        "GET" => store
            .files
            .get(path)
            .map(|b| (200, b.clone()))
            .unwrap_or((404, vec![])),
        "DELETE" => {
            store.files.remove(path);
            (204, vec![])
        }
        "PUT" => {
            if headers.to_lowercase().contains("if-none-match: *") && store.files.contains_key(path)
            {
                return (412, vec![]);
            }
            store.files.insert(path.into(), body.to_vec());
            (201, vec![])
        }
        "MOVE" => {
            let url = headers
                .lines()
                .find_map(|l| {
                    l.split_once(':')
                        .filter(|(k, _)| k.eq_ignore_ascii_case("destination"))
                        .map(|(_, v)| v.trim())
                })
                .unwrap();
            let dest = url.split_once("://").unwrap().1;
            let dest = &dest[dest.find('/').unwrap()..];
            if store.files.contains_key(dest) && headers.to_lowercase().contains("overwrite: f") {
                return (412, vec![]);
            }
            let value = store.files.remove(path).unwrap();
            store.files.insert(dest.into(), value);
            (201, vec![])
        }
        "PROPFIND" => {
            let root = format!("{}/", path.trim_end_matches('/'));
            let children: BTreeSet<_> = store
                .files
                .keys()
                .filter_map(|p| p.strip_prefix(&root))
                .map(|p| p.split('/').next().unwrap())
                .collect();
            let mut xml = format!("<{prefix}:multistatus xmlns:{prefix}=\"DAV:\">");
            for name in children {
                xml.push_str(&format!("<{prefix}:response><{prefix}:href>{root}{name}</{prefix}:href></{prefix}:response>"));
            }
            xml.push_str(&format!("</{prefix}:multistatus>"));
            (207, xml.into_bytes())
        }
        _ => (500, vec![]),
    }
}

/// 测试对象完整哈希。
pub fn hash() -> String {
    blake3::hash(b"content").to_hex().to_string()
}

/// 构造合法来源事件，所有时间字段稳定以验证不可变重试。
pub fn event(seq: u64) -> JournalEvent {
    let time = chrono::DateTime::parse_from_rfc3339("2026-09-11T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    JournalEvent {
        event_id: format!("e{seq}"),
        device_id: "a".into(),
        epoch: 1,
        seq,
        logical_clock: seq,
        schema_version: 1,
        event_type: JournalEventType::FileRecordAdded,
        created_at: time,
        payload: serde_json::json!({"object_id":format!("blake3:{}",hash()),"hash":hash(),"record_id":format!("r{seq}"),"size":7,
            "source_type":"wechat-windows-4","source_account_id":"wxid_test","source_conversation_id":null,
            "original_name":"中文测试文件.pdf","file_time":time.to_rfc3339(),"discovered_at":time.to_rfc3339(),"extension":"pdf"}),
    }
}

/// 在本机模拟已完成或未完成的上传任务。
pub fn add_source(db: &mut Database, ev: &JournalEvent, ready: bool) {
    db.apply_file_record_added_event(ev).unwrap();
    db.insert_journal_event_if_absent(ev).unwrap();
    db.connection().execute("INSERT INTO upload_tasks(task_id,record_id,object_id,status,updated_at) VALUES (?1,?2,?3,?4,?5)",
        rusqlite::params![format!("t{}",ev.event_id),ev.payload["record_id"].as_str().unwrap(),ev.payload["object_id"].as_str().unwrap(),if ready{"backed_up"}else{"queued"},ev.created_at.to_rfc3339()]).unwrap();
}
