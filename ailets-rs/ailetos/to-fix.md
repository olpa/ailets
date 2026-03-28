# cli doesnt compile

# review

- [x] ailets-rs/ailetos/examples/stdin_dag_flow.rs
- [x] ailets-rs/ailetos/src/environment.rs
- [x] ailets-rs/ailetos/src/pipe/pool.rs
- [x] ailets-rs/ailetos/src/system_runtime.rs
- [x] ailets-rs/ailetos/tests/environment.rs
- [x] ailets-rs/ailetos/tests/pipepool.rs
- [ ] ailets-rs/ailetos/src/dag.rs
- [ ] ailets-rs/ailetos/src/pipe/merge.rs
- [ ] ailets-rs/ailetos/src/pipe/pool.rs
- [ ] ailets-rs/ailetos/src/system_runtime.rs
- [ ] ailets-rs/ailetos/tests/pipepool.rs

git diff a214-dag-iterator .... | gvim -

# [ ]

environment.rs:127
        let path = format!("pipes/actor-{}-{:?}", handle.id(), StdHandle::Stdout);
To a function: from node id to its path on the vfs

