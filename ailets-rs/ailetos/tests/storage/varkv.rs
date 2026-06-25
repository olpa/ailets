use std::sync::Arc;

use ailetos::storage::varkv::VarKV;
use ailetos::storage::{KVBuffers, KVError, OpenMode};
use ailetos::VarStore;

fn make_varkv(var_store: Arc<VarStore>) -> VarKV {
    VarKV::new(var_store)
}

#[tokio::test]
async fn open_returns_per_actor_value() {
    let store = Arc::new(VarStore::new());
    store.set(Some(7), "KEY", "actor-val");
    let kv = make_varkv(store);

    let buf = kv.open("/7/KEY", OpenMode::Read).await.unwrap();
    assert_eq!(&*buf.lock(), b"actor-val");
}

#[tokio::test]
async fn open_falls_back_to_global() {
    let store = Arc::new(VarStore::new());
    store.set(None, "KEY", "global-val");
    let kv = make_varkv(store);

    let buf = kv.open("/7/KEY", OpenMode::Read).await.unwrap();
    assert_eq!(&*buf.lock(), b"global-val");
}

#[tokio::test]
async fn open_falls_back_to_os_env() {
    std::env::set_var("VARKV_TEST_OS_VAR", "os-val");
    let kv = make_varkv(Arc::new(VarStore::new()));

    let buf = kv
        .open("/7/VARKV_TEST_OS_VAR", OpenMode::Read)
        .await
        .unwrap();
    assert_eq!(&*buf.lock(), b"os-val");
}

#[tokio::test]
async fn open_returns_not_found_when_absent() {
    let kv = make_varkv(Arc::new(VarStore::new()));

    let result = kv.open("/7/NO_SUCH_VAR_XYZZY", OpenMode::Read).await;
    assert!(matches!(result, Err(KVError::NotFound(_))));
}

#[tokio::test]
async fn open_write_returns_error() {
    let kv = make_varkv(Arc::new(VarStore::new()));

    let result = kv.open("/7/KEY", OpenMode::Write).await;
    assert!(matches!(result, Err(KVError::Backend(_))));
}

#[tokio::test]
async fn listdir_root_is_forbidden() {
    let kv = make_varkv(Arc::new(VarStore::new()));
    let result = kv.listdir("/").await;
    assert!(matches!(result, Err(KVError::Backend(_))));
}

#[tokio::test]
async fn listdir_subdir_is_forbidden() {
    let kv = make_varkv(Arc::new(VarStore::new()));
    let result = kv.listdir("/7/sub/").await;
    assert!(matches!(result, Err(KVError::Backend(_))));
}

#[tokio::test]
async fn listdir_env_returns_varstore_keys() {
    std::env::set_var("VARKV_LISTDIR_TEST_OS_KEY", "os-val");
    let store = Arc::new(VarStore::new());
    store.set(None, "A", "1");
    store.set(Some(7), "B", "2");
    let kv = make_varkv(store);

    let keys = kv.listdir("/7/").await.unwrap();
    assert!(keys.contains(&"/7/A".to_string()));
    assert!(keys.contains(&"/7/B".to_string()));
    assert!(keys.contains(&"/7/VARKV_LISTDIR_TEST_OS_KEY".to_string()));
}
