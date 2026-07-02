use std::sync::Arc;

use ailetos::VarStore;

fn arc(s: &str) -> Arc<str> {
    Arc::from(s)
}

#[test]
fn get_returns_none_for_unknown_key() {
    let store = VarStore::new();
    assert_eq!(store.get(1, "MISSING"), None);
}

#[test]
fn get_returns_global_var() {
    let store = VarStore::new();
    store.set(None, "KEY", "value");
    assert_eq!(store.get(42, "KEY"), Some(arc("value")));
}

#[test]
fn get_returns_per_actor_var() {
    let store = VarStore::new();
    store.set(Some(7), "KEY", "actor-value");
    assert_eq!(store.get(7, "KEY"), Some(arc("actor-value")));
}

#[test]
fn per_actor_takes_priority_over_global() {
    let store = VarStore::new();
    store.set(None, "KEY", "global");
    store.set(Some(7), "KEY", "actor");
    assert_eq!(store.get(7, "KEY"), Some(arc("actor")));
}

#[test]
fn falls_back_to_global_when_no_per_actor_entry() {
    let store = VarStore::new();
    store.set(None, "KEY", "global");
    assert_eq!(store.get(7, "KEY"), Some(arc("global")));
}

#[test]
fn per_actor_var_not_returned_for_different_actor() {
    let store = VarStore::new();
    store.set(Some(7), "KEY", "actor7");
    assert_eq!(store.get(99, "KEY"), None);
}

#[test]
fn keys_returns_global_keys() {
    let store = VarStore::new();
    store.set(None, "A", "1");
    store.set(None, "B", "2");
    let mut keys = store.keys(42);
    keys.sort();
    assert_eq!(keys, vec![arc("A"), arc("B")]);
}

#[test]
fn keys_returns_per_actor_keys() {
    let store = VarStore::new();
    store.set(Some(7), "X", "1");
    store.set(Some(7), "Y", "2");
    let mut keys = store.keys(7);
    keys.sort();
    assert_eq!(keys, vec![arc("X"), arc("Y")]);
}

#[test]
fn keys_returns_union_without_duplicates() {
    let store = VarStore::new();
    store.set(None, "A", "global");
    store.set(Some(7), "A", "actor");
    store.set(Some(7), "B", "actor");
    let mut keys = store.keys(7);
    keys.sort();
    assert_eq!(keys, vec![arc("A"), arc("B")]);
}

#[test]
fn own_keys_excludes_global_keys() {
    let store = VarStore::new();
    store.set(None, "GLOBAL", "g");
    store.set(Some(7), "LOCAL", "l");
    let mut keys = store.own_keys(7);
    keys.sort();
    assert_eq!(keys, vec![arc("LOCAL")]);
}

#[test]
fn own_keys_returns_empty_for_actor_with_only_global_vars() {
    let store = VarStore::new();
    store.set(None, "GLOBAL", "g");
    assert_eq!(store.own_keys(7), Vec::<Arc<str>>::new());
}

#[test]
fn last_set_wins() {
    let store = VarStore::new();
    store.set(None, "KEY", "first");
    store.set(None, "KEY", "second");
    assert_eq!(store.get(1, "KEY"), Some(arc("second")));

    store.set(Some(5), "KEY", "actor-first");
    store.set(Some(5), "KEY", "actor-second");
    assert_eq!(store.get(5, "KEY"), Some(arc("actor-second")));
}

#[test]
fn getenv_returns_varstore_value() {
    let store = VarStore::new();
    store.set(Some(7), "MY_KEY", "my-value");
    assert_eq!(store.getenv(7, "MY_KEY"), Some(arc("my-value")));
}

#[test]
fn getenv_returns_none_when_absent() {
    let store = VarStore::new();
    assert_eq!(store.getenv(7, "NO_SUCH_KEY_XYZZY"), None);
}

#[test]
fn getenv_falls_back_to_global() {
    let store = VarStore::new();
    store.set(None, "GLOBAL_KEY", "global-val");
    assert_eq!(store.getenv(99, "GLOBAL_KEY"), Some(arc("global-val")));
}

#[test]
fn getenv_falls_back_to_os_env() {
    std::env::set_var("GETENV_TEST_OS_VAR", "os-val");
    let store = VarStore::new();
    assert_eq!(store.getenv(7, "GETENV_TEST_OS_VAR"), Some(arc("os-val")));
}

#[test]
fn keysenv_includes_os_env_keys() {
    std::env::set_var("KEYSENV_TEST_OS_KEY", "os-val");
    let store = VarStore::new();
    store.set(Some(7), "STORE_KEY", "store-val");
    let keys = store.keysenv(7);
    assert!(keys.contains(&arc("STORE_KEY")));
    assert!(keys.contains(&arc("KEYSENV_TEST_OS_KEY")));
}
