use crate::dag::NodeState;
use crate::idgen::Handle;
use tokio::sync::oneshot;

/// Actor lifecycle events sent from IoBridge to the executor.
pub enum ActorLifecycleEvent {
    /// Request to transition actor to Terminating state.
    /// Executor replies with the state that was set before the transition.
    /// If the prior state was already Terminating or Terminated, the IO bridge skips cleanup.
    Terminating { node_handle: Handle, reply: oneshot::Sender<NodeState> },
    /// I/O cleanup complete; executor should mark the actor Terminated.
    /// Executor replies with the prior state once the transition is done.
    Terminated { node_handle: Handle, exit_code: i32, reply: oneshot::Sender<NodeState> },
}
