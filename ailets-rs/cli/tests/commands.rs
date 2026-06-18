use dagsh::DagShell;

fn make_shell() -> DagShell {
    DagShell::new()
}

#[test]
fn test_dag_exists_absent() {
    let shell = make_shell();
    assert_eq!(shell.cmd_dag(&["exists", "input"]).unwrap(), "0");
}

#[test]
fn test_dag_exists_present() {
    let mut shell = make_shell();
    shell.cmd_value(&["hello"]).unwrap();
    shell.cmd_alias(&["input", "1"]).unwrap();
    assert_eq!(shell.cmd_dag(&["exists", "input"]).unwrap(), "1");
}

#[test]
fn test_dag_handle_present() {
    let mut shell = make_shell();
    let value_handle = shell.cmd_value(&["hello"]).unwrap();
    let alias_handle = shell.cmd_alias(&["input", "1"]).unwrap();
    let id = shell.cmd_dag(&["handle", "input"]).unwrap();
    assert_eq!(id, alias_handle.id().to_string());
    let val_id = shell.cmd_dag(&["handle", &value_handle.id().to_string()]).unwrap();
    assert_eq!(val_id, value_handle.id().to_string());
}

#[test]
fn test_dag_handle_absent() {
    let shell = make_shell();
    assert!(shell.cmd_dag(&["handle", "nosuchnode"]).is_err());
}

#[test]
fn test_dag_unknown_subcommand() {
    let shell = make_shell();
    assert!(shell.cmd_dag(&["bogus", "x"]).is_err());
}

#[test]
fn test_dag_no_subcommand() {
    let shell = make_shell();
    assert!(shell.cmd_dag(&[]).is_err());
}
