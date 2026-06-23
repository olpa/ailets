use ailetos::VarStore;

#[test]
fn get_returns_none_for_unknown_key() {
    let store = VarStore::new();
    assert_eq!(store.get(1, "MISSING"), None);
}

#[test]
fn get_returns_global_var() {
    let store = VarStore::new();
    store.set(None, "KEY", "value");
    assert_eq!(store.get(42, "KEY"), Some("value".to_string()));
}

#[test]
fn get_returns_per_actor_var() {
    let store = VarStore::new();
    store.set(Some(7), "KEY", "actor-value");
    assert_eq!(store.get(7, "KEY"), Some("actor-value".to_string()));
}

#[test]
fn per_actor_takes_priority_over_global() {
    let store = VarStore::new();
    store.set(None, "KEY", "global");
    store.set(Some(7), "KEY", "actor");
    assert_eq!(store.get(7, "KEY"), Some("actor".to_string()));
}

#[test]
fn falls_back_to_global_when_no_per_actor_entry() {
    let store = VarStore::new();
    store.set(None, "KEY", "global");
    assert_eq!(store.get(7, "KEY"), Some("global".to_string()));
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
    assert_eq!(keys, vec!["A", "B"]);
}

#[test]
fn keys_returns_per_actor_keys() {
    let store = VarStore::new();
    store.set(Some(7), "X", "1");
    store.set(Some(7), "Y", "2");
    let mut keys = store.keys(7);
    keys.sort();
    assert_eq!(keys, vec!["X", "Y"]);
}

#[test]
fn keys_returns_union_without_duplicates() {
    let store = VarStore::new();
    store.set(None, "A", "global");
    store.set(Some(7), "A", "actor");
    store.set(Some(7), "B", "actor");
    let mut keys = store.keys(7);
    keys.sort();
    assert_eq!(keys, vec!["A", "B"]);
}

#[test]
fn last_set_wins() {
    let store = VarStore::new();
    store.set(None, "KEY", "first");
    store.set(None, "KEY", "second");
    assert_eq!(store.get(1, "KEY"), Some("second".to_string()));

    store.set(Some(5), "KEY", "actor-first");
    store.set(Some(5), "KEY", "actor-second");
    assert_eq!(store.get(5, "KEY"), Some("actor-second".to_string()));
}
