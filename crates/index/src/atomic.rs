// ChatVault SQLite 原子操作：允许事件应用与游标更新共用事务，失败全部回滚。
use crate::Database;
use chatvault_core::error::{ChatVaultError, Result};

impl Database {
    /// 在可嵌套的保存点中执行操作；任何错误均回滚此操作的全部写入。
    pub fn atomic<T>(&mut self, operation: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let name = format!("tx_{}", uuid::Uuid::new_v4().simple());
        self.conn
            .execute_batch(&format!("SAVEPOINT {name}"))
            .map_err(db_error)?;
        match operation(self) {
            Ok(value) => {
                if let Err(e) = self.conn.execute_batch(&format!("RELEASE {name}")) {
                    let _ = self
                        .conn
                        .execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name}"));
                    return Err(db_error(e));
                }
                Ok(value)
            }
            Err(e) => {
                self.conn
                    .execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name}"))
                    .map_err(db_error)?;
                Err(e)
            }
        }
    }
}

/// 转换数据库错误。
fn db_error(e: rusqlite::Error) -> ChatVaultError {
    ChatVaultError::Database(e.to_string())
}
