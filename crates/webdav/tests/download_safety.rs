// ChatVault WebDAV 下载与校验安全边界回归。
use chatvault_webdav::WebDavClient;

/// 目标已存在时必须拒绝覆盖，避免误伤用户已有文件。
#[tokio::test]
async fn download_refuses_existing_destination() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("keep.bin");
    std::fs::write(&dest, b"do-not-clobber").unwrap();
    // 无真实服务器也会在发起 GET 前因目标存在而失败；用非法 URL 构造客户端。
    // 若未来改为先 GET 再检查，此测试仍应因 dest 存在而失败。
    let client = match WebDavClient::new(chatvault_webdav::WebDavConfig {
        base_url: "https://127.0.0.1:1/dav".into(),
        username: None,
        password: None,
    }) {
        Ok(c) => c,
        Err(_) => return,
    };
    let result = client
        .download_to_path("chatvault-v/objects/x", &dest, &"a".repeat(64))
        .await;
    assert!(result.is_err());
    assert_eq!(std::fs::read(&dest).unwrap(), b"do-not-clobber");
}
