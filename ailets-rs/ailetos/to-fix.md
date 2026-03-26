# cli doesnt compile

# review

- [x] ailets-rs/ailetos/examples/stdin_dag_flow.rs
- [x] ailets-rs/ailetos/src/environment.rs
- [x] ailets-rs/ailetos/src/pipe/pool.rs
- [x] ailets-rs/ailetos/src/system_runtime.rs
- [x] ailets-rs/ailetos/tests/environment.rs
- [x] ailets-rs/ailetos/tests/pipepool.rs

git diff a214-dag-iterator .... | gvim -

# [ ]

environment.rs:127
        let path = format!("pipes/actor-{}-{:?}", handle.id(), StdHandle::Stdout);
To a function: from node id to its path on the vfs

# [x]

environment.rs:128
        if let Ok(buffer) = self.kv.open(&path, OpenMode::Write).await {
                      if let Err(e) = buffer.append(&data) {
I think "kv" should have something like "copy_in", to move away the streaming in the ewnvironemtn. A better name is needed.

# [x]

environment.rs:67
    /// Value data for value nodes (keyed by node handle)
        value_nodes: HashMap<Handle, ValueNodeData>,
Not needed anymore. The values are insid vks
The usages of value_nodes should be removed too

# [ ]

pool.rs:41
+    pub fn new(kv: Arc<K>, notification_queue: NotificationQueueArc, dag: Arc<RwLock<Dag>>) -> Self {
I think we should pass not dag, but a closure which knows how to check if a node will ever be instantiated

# [ ]

pool.rs:93
logic error: We don't need to check "allow_latent": Maybe the node was terminated in a previous run and its output is not tracked in pool but tracked in vks.

# [ ]

pool.rs:
+            match state_check {
Race condition here? Likely not, but should explain why.

Comment: does "Wait(notify)" will autoactivate itself if was fulfilled before the call to Wait?

# [ ]

pool.rs
                            // Producer will eventually produce output - create latent pipe
We have a problem here: If scheduler will never actually start the dep node, it will still hang. The app shutdown shold handle the issue.

# [x]

poolrs:289
                            // Producer terminated - check KV for existing output
                                                        let path = format!("pipes/actor-{}-{:?}", actor_handle.id(), key.1);
                                                                                    match self.kv.open(&path, OpenMode::Read).await {
                                                                                                                      Ok(kv_buffer) => {
Wrong responsibility. Should be moved to vks or reader-writer.

# [ ]

pipe.rs:321
                        None => {
                                                      // Node doesn't exist in DAG - create latent pipe anyway
                                                                                  // (may be in test environment or node not yet added to DAG)
                                                                                Sounds wrong. Should be an error.

# [ ]

pipe.rs:200
    /// Get or create a reader for a pipe

The comment doesn't reflect the new functionality.

Check also the module comment.

# [ ]

pip.rs:170
    /// - `notification_queue`: Shared notification queue for pipe data events

Why the queue is passed from outside? Probably because of shutdown works. An explanation for maintainers is needed.

# [ ]

tests/pipepool.rs
+#[tokio::test]
+async fn test_terminated_node_without_kv_data_returns_none() {
More likely, it should be an error

