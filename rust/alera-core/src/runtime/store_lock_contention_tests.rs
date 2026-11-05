use super::RuntimeStore;
use sqlx::Row;
use std::time::Duration;

#[tokio::test]
async fn runtime_database_connections_wait_for_transient_wal_writers() {
    let dir = tempfile::tempdir().expect("private runtime dir");
    let store = RuntimeStore::open(dir.path()).await.expect("open runtime");
    let mut writer = store.pool().acquire().await.expect("writer connection");
    let mut second = store.pool().acquire().await.expect("reader connection");
    for connection in [&mut writer, &mut second] {
        let row = sqlx::query("PRAGMA busy_timeout")
            .fetch_one(&mut **connection)
            .await
            .expect("busy timeout PRAGMA");
        let millis: i64 = row.try_get(0).expect("timeout in milliseconds");
        assert_eq!(millis, 15_000, "all connections must wait for a writer");
    }

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *writer)
        .await
        .expect("reserve write lock");
    let (tx, rx) = tokio::sync::oneshot::channel();
    let delayed_write = tokio::spawn(async move {
        let result = sqlx::query("CREATE TABLE IF NOT EXISTS busy_timeout_probe (id INTEGER)")
            .execute(&mut *second)
            .await;
        let _ = tx.send(result.map(|_| ()));
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    sqlx::query("ROLLBACK")
        .execute(&mut *writer)
        .await
        .expect("release write lock");
    rx.await
        .expect("delayed write completed")
        .expect("transient lock must not fail a write");
    delayed_write.await.expect("write join");
}
