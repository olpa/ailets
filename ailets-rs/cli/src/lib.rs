//! DAG Shell library - DagShell and OutputSink.
//!
//! The ailetos executor runs on a dedicated `tokio::runtime::Runtime` owned by
//! `DagShell` for the session lifetime. The CLI thread stays synchronous.
//!
//! A permanent notification watcher thread consumes executor events and either
//! signals the active `join_handle` call or prints a background notification.
//! This means node terminations are always reported, even while the user is at
//! the prompt.

pub(crate) mod dbg_actor;
pub(crate) mod dbg_control;
pub(crate) mod shell_input_actor;
pub(crate) mod shell_input_control;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use actor_runtime::StdHandle;
use ailetos::{
    pipe::pipe_path, DependsOn, Environment, Executor, ExecutorEvent, For, Handle, KVBuffers,
    MemKV, NodeState, OpenMode, StopConditions, TopologicalOrderIter,
};
use futures::future::Abortable;

// ---------------------------------------------------------------------------
// OutputSink
// ---------------------------------------------------------------------------

/// Where DagShell output is written. `Send + Sync` so the notification
/// watcher thread can hold an `Arc<dyn OutputSink>`.
pub trait OutputSink: Send + Sync {
    fn println(&self, line: &str);
}

pub struct StdoutSink;

impl OutputSink for StdoutSink {
    fn println(&self, line: &str) {
        println!("{line}");
    }
}

// ---------------------------------------------------------------------------
// Color support
// ---------------------------------------------------------------------------

fn parse_color(s: &str) -> Result<u8, String> {
    if let Ok(n) = s.parse::<u8>() {
        return Ok(n);
    }
    let key = s.to_ascii_lowercase().replace(['-', '_', ' '], "");
    named_color(&key).ok_or_else(|| format!("unknown color '{s}'; use a CSS/X11 name or 0-255"))
}

#[allow(clippy::too_many_lines)]
fn named_color(name: &str) -> Option<u8> {
    Some(match name {
        // Standard 16 terminal colors
        "black"                             =>   0,
        "maroon"                            =>   1,
        "darkgreen"                         =>   2,
        "olive" | "darkyellow"              =>   3,
        "navy"                              =>   4,
        "purple" | "darkmagenta"            =>   5,
        "teal" | "darkcyan"                 =>   6,
        "silver" | "lightgray" | "lightgrey"=>   7,
        "darkgray" | "darkgrey"
        | "grey" | "gray"                   =>   8,
        "red"                               =>   9,
        "green" | "lime"                    =>  10,
        "yellow"                            =>  11,
        "blue"                              =>  12,
        "fuchsia" | "magenta"               =>  13,
        "aqua" | "cyan"                     =>  14,
        "white"                             =>  15,
        // 256-color extended names
        "darkred"                           =>  88,
        "darkblue"                          =>  18,
        "deepskyblue"                       =>  39,
        "dodgerblue"                        =>  33,
        "royalblue"                         =>  62,
        "steelblue"                         =>  67,
        "cornflowerblue"                    =>  69,
        "skyblue"                           => 117,
        "lightskyblue"                      => 117,
        "lightblue"                         => 152,
        "powderblue"                        => 153,
        "lightsteelblue"                    => 147,
        "cadetblue"                         =>  73,
        "mediumblue"                        =>  20,
        "midnightblue"                      =>  18,
        "indigo"                            =>  54,
        "darkslateblue"                     =>  60,
        "slateblue"                         =>  62,
        "mediumslateblue"                   => 105,
        "mediumpurple"                      => 141,
        "blueviolet"                        =>  57,
        "darkviolet"                        =>  92,
        "darkorchid"                        =>  98,
        "orchid"                            => 170,
        "violet"                            => 213,
        "plum"                              => 183,
        "lavender"                          => 189,
        "thistle"                           => 182,
        "mediumorchid"                      => 134,
        "darkmagentaext"                    =>  90,
        "mediumvioletred"                   => 162,
        "palevioletred"                     => 168,
        "hotpink"                           => 205,
        "deeppink"                          => 197,
        "pink"                              => 218,
        "lightpink"                         => 217,
        "crimson"                           => 160,
        "firebrick"                         => 124,
        "darkred2"                          =>  52,
        "indianred"                         => 131,
        "lightcoral"                        => 210,
        "salmon"                            => 209,
        "darksalmon"                        => 173,
        "lightsalmon"                       => 216,
        "tomato"                            => 202,
        "orangered"                         => 202,
        "darkorange"                        => 208,
        "orange"                            => 214,
        "coral"                             => 209,
        "gold"                              => 220,
        "goldenrod"                         => 178,
        "darkgoldenrod"                     => 136,
        "yellow2"                           => 226,
        "lightyellow"                       => 230,
        "lemonchiffon"                      => 230,
        "khaki"                             => 185,
        "darkkhaki"                         => 143,
        "palegoldenrod"                     => 229,
        "chartreuse"                        => 118,
        "lawngreen"                         => 118,
        "greenyellow"                       => 154,
        "yellowgreen"                       => 148,
        "limegreen"                         =>  40,
        "mediumspringgreen"                 =>  48,
        "springgreen"                       =>  48,
        "green2"                            =>  46,
        "forestgreen"                       =>  28,
        "seagreen"                          =>  29,
        "mediumseagreen"                    =>  35,
        "darkseagreen"                      => 108,
        "palegreen"                         => 120,
        "lightgreen"                        => 120,
        "darkolivegreen"                    =>  58,
        "olivedrab"                         =>  64,
        "darkturquoise"                     =>  44,
        "mediumturquoise"                   =>  80,
        "turquoise"                         =>  80,
        "aquamarine"                        => 122,
        "mediumaquamarine"                  =>  79,
        "paleturquoise"                     => 159,
        "lightcyan"                         => 195,
        "lightseagreen"                     =>  37,
        "cyan2"                             =>  51,
        "rosybrown"                         => 138,
        "sienna"                            => 130,
        "saddlebrown"                       =>  94,
        "chocolate"                         => 166,
        "peru"                              => 136,
        "sandybrown"                        => 215,
        "tan"                               => 180,
        "burlywood"                         => 180,
        "wheat"                             => 229,
        "moccasin" | "peachpuff"            => 223,
        "navajowhite"                       => 223,
        "brown"                             => 124,
        "slategray" | "slategrey"           => 103,
        "lightslategray" | "lightslategrey" => 103,
        "darkslategray" | "darkslategrey"   =>  23,
        "dimgray" | "dimgrey"               => 241,
        "gainsboro"                         => 253,
        "whitesmoke"                        => 255,
        // Grayscale ramp (grey0-grey23 → indices 232-255)
        "grey0"  | "gray0"                  => 232,
        "grey1"  | "gray1"                  => 233,
        "grey2"  | "gray2"                  => 234,
        "grey3"  | "gray3"                  => 235,
        "grey4"  | "gray4"                  => 236,
        "grey5"  | "gray5"                  => 237,
        "grey6"  | "gray6"                  => 238,
        "grey7"  | "gray7"                  => 239,
        "grey8"  | "gray8"                  => 240,
        "grey9"  | "gray9"                  => 241,
        "grey10" | "gray10"                 => 242,
        "grey11" | "gray11"                 => 243,
        "grey12" | "gray12"                 => 244,
        "grey13" | "gray13"                 => 245,
        "grey14" | "gray14"                 => 246,
        "grey15" | "gray15"                 => 247,
        "grey16" | "gray16"                 => 248,
        "grey17" | "gray17"                 => 249,
        "grey18" | "gray18"                 => 250,
        "grey19" | "gray19"                 => 251,
        "grey20" | "gray20"                 => 252,
        "grey21" | "gray21"                 => 253,
        "grey22" | "gray22"                 => 254,
        "grey23" | "gray23"                 => 255,
        _                                   => return None,
    })
}

// ---------------------------------------------------------------------------
// OutputSinkWriter — adapts OutputSink as std::io::Write for attach_stdout_to
// ---------------------------------------------------------------------------

/// Line-buffers bytes and forwards complete lines through an `OutputSink`,
/// optionally colorizing each line with a 256-color ANSI code.
struct OutputSinkWriter {
    sink: Arc<dyn OutputSink>,
    buf: Vec<u8>,
    color: Option<u8>,
}

impl OutputSinkWriter {
    fn new(sink: Arc<dyn OutputSink>, color: Option<u8>) -> Self {
        Self { sink, buf: Vec::new(), color }
    }

    fn emit(&self, line: &str) {
        match self.color {
            Some(c) => self.sink.println(&format!("\x1b[38;5;{c}m{line}\x1b[0m")),
            None    => self.sink.println(line),
        }
    }
}

impl std::io::Write for OutputSinkWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&self.buf[..pos]).into_owned();
            self.buf.drain(..=pos);
            self.emit(&line);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            let line = String::from_utf8_lossy(&self.buf).into_owned();
            self.buf.clear();
            self.emit(&line);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// State set by `join_handle` so the watcher thread knows what to signal.
struct JoinWaiter {
    target: Handle,
    ready_tx: std::sync::mpsc::SyncSender<()>,
}

/// Sent to the watcher thread when the executor is replaced (on `reset`).
struct WatcherUpdate {
    events_rx: std::sync::mpsc::Receiver<ExecutorEvent>,
    env: Arc<Environment>,
}

// ---------------------------------------------------------------------------
// Executor startup helpers
// ---------------------------------------------------------------------------

fn start_executor_with_bridge(
    rt: &tokio::runtime::Runtime,
    env: Arc<Environment>,
) -> (Executor, std::sync::mpsc::Receiver<ExecutorEvent>) {
    let (tokio_tx, mut tokio_rx) = tokio::sync::mpsc::unbounded_channel::<ExecutorEvent>();
    let (sync_tx, sync_rx) = std::sync::mpsc::channel::<ExecutorEvent>();

    let executor = {
        let _guard = rt.enter();
        Executor::start(env, Some(tokio_tx))
    };

    rt.spawn(async move {
        while let Some(event) = tokio_rx.recv().await {
            if sync_tx.send(event).is_err() {
                break;
            }
        }
    });

    (executor, sync_rx)
}

fn make_env(kv: &Arc<MemKV>) -> Arc<Environment> {
    let env = Arc::new(Environment::new(Arc::clone(kv) as Arc<dyn KVBuffers>));
    {
        let mut reg = env.actor_registry.write();
        reg.register("cat", cat::execute);
        reg.register("dbg", dbg_actor::execute);
        reg.register("shell_input", shell_input_actor::execute);
    }
    env
}

/// Spawn the watcher thread.
///
/// The watcher owns `events_rx` for the current executor. On each event:
/// - if `pending_join` targets this handle → signal the waiter
/// - otherwise → print a notification via `notification_sink`
///
/// When the executor is replaced (`reset`), `DagShell` sends a `WatcherUpdate`
/// so the watcher switches to the new receiver. When `update_rx` closes (on
/// `DagShell` drop), the watcher exits.
fn start_notification_watcher(
    initial: WatcherUpdate,
    update_rx: std::sync::mpsc::Receiver<WatcherUpdate>,
    pending_join: Arc<Mutex<Option<JoinWaiter>>>,
    notification_sink: Arc<dyn OutputSink>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut env = initial.env;
        let mut events_rx = initial.events_rx;

        loop {
            match update_rx.try_recv() {
                Ok(upd) => {
                    env = upd.env;
                    events_rx = upd.events_rx;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }

            match events_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(ExecutorEvent::NodeTerminated(h)) => {
                    let mut pending = pending_join.lock().unwrap();
                    if pending.as_ref().map(|j| j.target == h).unwrap_or(false) {
                        if let Some(waiter) = pending.take() {
                            let _ = waiter.ready_tx.send(());
                        }
                    } else if pending.is_none() {
                        let name = {
                            let dag = env.dag.read();
                            dag.get_node(h)
                                .map(|n| format!("{}#{}", n.idname, h.id()))
                                .unwrap_or_else(|| format!("node#{}", h.id()))
                        };
                        notification_sink.println(&format!("[{name}] done"));
                    }
                    // else: foreground join active but not our target — suppress
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // Old executor done; wait for the next executor (or drop).
                    match update_rx.recv() {
                        Ok(upd) => {
                            env = upd.env;
                            events_rx = upd.events_rx;
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// DagShell
// ---------------------------------------------------------------------------

pub struct DagShell {
    env: Arc<Environment>,
    kv: Arc<MemKV>,
    handles: Vec<Handle>,
    vars: HashMap<String, Handle>,
    sink: Box<dyn OutputSink>,
    notification_sink: Arc<dyn OutputSink>,
    pending_join: Arc<Mutex<Option<JoinWaiter>>>,
    watcher_update_tx: std::sync::mpsc::SyncSender<WatcherUpdate>,
    // Kept alive until DagShell drops; the drop closes watcher_update_tx
    // which causes the watcher to exit.
    _watcher: std::thread::JoinHandle<()>,
    // executor drops before ailetos_rt (declaration order = drop order).
    executor: Executor,
    ailetos_rt: tokio::runtime::Runtime,
}

impl DagShell {
    pub fn new() -> Self {
        Self::new_with_sinks(Box::new(StdoutSink), Arc::new(StdoutSink))
    }

    pub fn new_with_sink(sink: Box<dyn OutputSink>) -> Self {
        Self::new_with_sinks(sink, Arc::new(StdoutSink))
    }

    /// Create a shell with separate sinks for synchronous command output and
    /// background notifications (node terminations while at the prompt).
    pub fn new_with_sinks(
        command_sink: Box<dyn OutputSink>,
        notification_sink: Arc<dyn OutputSink>,
    ) -> Self {
        let kv = Arc::new(MemKV::new());
        let env = make_env(&kv);
        let ailetos_rt =
            tokio::runtime::Runtime::new().expect("failed to create ailetos runtime");
        let (executor, events_rx) = start_executor_with_bridge(&ailetos_rt, Arc::clone(&env));

        let pending_join: Arc<Mutex<Option<JoinWaiter>>> = Arc::new(Mutex::new(None));
        let (watcher_update_tx, update_rx) =
            std::sync::mpsc::sync_channel::<WatcherUpdate>(4);

        let notification_sink_clone = Arc::clone(&notification_sink);
        let watcher = start_notification_watcher(
            WatcherUpdate {
                events_rx,
                env: Arc::clone(&env),
            },
            update_rx,
            Arc::clone(&pending_join),
            notification_sink,
        );

        Self {
            env,
            kv,
            handles: Vec::new(),
            vars: HashMap::new(),
            sink: command_sink,
            notification_sink: notification_sink_clone,
            pending_join,
            watcher_update_tx,
            _watcher: watcher,
            executor,
            ailetos_rt,
        }
    }

    fn parse_handle(&self, s: &str) -> Option<Handle> {
        if let Some(var_name) = s.strip_prefix('$') {
            return self.vars.get(var_name).copied();
        }
        s.parse::<i64>().ok().map(Handle::new)
    }

    pub fn execute(&mut self, line: &str) -> Result<bool, String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let (cmd, rest) = match parts.split_first() {
            None => return Ok(true),
            Some((cmd, rest)) => (*cmd, rest),
        };

        match cmd {
            "quit" | "exit" | "q" => {
                self.prepare_exit();
                return Ok(false);
            }
            "help" | "?" => self.cmd_help(),
            "set" => self.cmd_set(rest)?,
            "node" => {
                self.cmd_node(rest)?;
            }
            "dep" => self.cmd_dep(rest)?,
            "deps" => self.cmd_deps(rest)?,
            "show" => self.cmd_show(rest)?,
            "run" => self.cmd_run(rest)?,
            "join" | "await" => self.cmd_join(rest)?,
            "follow" => self.cmd_follow(rest)?,
            "cat" => self.cmd_cat(rest)?,
            "status" => self.cmd_status(rest)?,
            "source" | "load" => self.cmd_source(rest)?,
            "reset" => self.cmd_reset(),
            "suspend" => self.cmd_suspend(rest)?,
            "resume" => self.cmd_resume(rest)?,
            "wait" => self.cmd_wait(rest)?,
            "write" => self.cmd_write(rest)?,
            "close" => self.cmd_close(rest)?,
            "kill" => self.cmd_kill(rest)?,
            "fg" => {
                self.sink
                    .println("'fg' has been removed. Use 'join <node>' instead.");
            }
            _ => {
                self.sink
                    .println(&format!("Unknown command: {cmd}. Type 'help' for usage."));
            }
        }

        Ok(true)
    }

    fn cmd_help(&self) {
        self.sink.println(
            r"DAG Shell Commands:

Node Management:
  node add <actor> [--explain=text]   Add actor node (actors: cat, dbg, shell_input)
  node value <data> [--explain=text]  Add value node (constant data)
  node alias <name> <target>          Add alias node
  node list                           List all nodes with status

Dependencies:
  dep <node> <dependency>             Add dependency (node depends on dependency)
  deps <node>                         Show direct dependencies

Visualization:
  show [node]                         Tree view (default: whole DAG)

Execution:
  run [node] [options]                Submit run to ailetos; waits by default
    --one-step                        Execute only the first ready node
    --stop-before <node>              Stop before executing this node
    --stop-after <node>               Stop after executing this node
    --bg                              Submit and return immediately (background)
    --color <name>                    Colorize output (CSS/X11 name or 0-255; --bg only)

Job Control:
  join <node>                         Wait for node to terminate; Ctrl+C to detach
  await <node>                        Synonym for join
  follow <node> [--color <name>]      Attach node stdout; optional 256-color name or 0-255
  kill [-N] <node>                    Kill actor with exit code N (default 130)

I/O:
  cat <node>                          Show output of a node

Status:
  status                              Overall DAG status
  status <node>                       Node status

Debug:
  suspend <node>                      Suspend a running actor
  resume <node>                       Resume a suspended actor (dbg or general)
  wait suspended <node>               Block until node is suspended (polls 10 ms, 5 s timeout)
  wait terminated <node>              Block until node is terminated (polls 10 ms, 5 s timeout)

Shell Input:
  write <node> <data>                 Write data to a shell_input actor
  close <node>                        Close a shell_input actor (send EOF)

Session:
  load <file>                         Run script file (alias: source)
  reset                               Clear all nodes and start fresh
  help                                Show this help
  quit                                Exit

Variables:
  set var = node ...                  Assign node to variable
  dep $foo $bar                       Use $var to reference variables",
        );
    }

    fn cmd_set(&mut self, args: &[&str]) -> Result<(), String> {
        match args {
            [var_name, "=", "node", rest @ ..] => {
                let handle = self.cmd_node_inner(rest)?;
                self.vars.insert((*var_name).to_string(), handle);
                Ok(())
            }
            _ => Err("Usage: set <var> = node ...".to_string()),
        }
    }

    fn cmd_node(&mut self, args: &[&str]) -> Result<(), String> {
        if args.first() == Some(&"list") {
            self.cmd_node_list();
        } else {
            self.cmd_node_inner(args)?;
        }
        Ok(())
    }

    fn cmd_node_list(&self) {
        if self.handles.is_empty() {
            self.sink.println("No nodes");
        } else {
            let dag = self.env.dag.read();
            for &handle in &self.handles {
                if let Some(node) = dag.get_node(handle) {
                    let state_str = format_state(node.state);
                    let explain = node
                        .explain
                        .as_ref()
                        .map_or_else(String::new, |e| format!(" # {e}"));
                    let pid = node.pid.id();
                    self.sink
                        .println(&format!("  {pid} {} [{state_str}]{explain}", node.idname));
                }
            }
        }
    }

    fn cmd_node_inner(&mut self, args: &[&str]) -> Result<Handle, String> {
        match args {
            ["add", actor, rest @ ..] => {
                let actor = (*actor).to_string();
                let explain = parse_explain(rest);
                let handle = self.env.add_node(actor.clone(), &[], explain.clone());
                self.handles.push(handle);

                if actor == "dbg" {
                    let bytes_before_pause = parse_bytes_before_pause(rest);
                    dbg_control::register_dbg_actor(handle, bytes_before_pause);
                }
                if actor == "shell_input" {
                    shell_input_control::register_shell_input_actor(handle);
                }

                let id = handle.id();
                let expl = explain.map_or_else(String::new, |e| format!("({e})"));
                self.sink.println(&format!("Added node {id}: {actor} {expl}"));
                Ok(handle)
            }
            ["add"] => Err("Usage: node add <actor> [--explain=text]".to_string()),
            ["value", rest @ ..] if !rest.is_empty() => {
                let data = parse_quoted_string(rest);
                let explain = parse_explain(rest);
                let env = Arc::clone(&self.env);
                let data_bytes = data.as_bytes().to_vec();
                let explain_clone = explain.clone();
                let handle = self
                    .ailetos_rt
                    .block_on(async move { env.add_value_node(data_bytes, explain_clone).await })
                    .map_err(|e| format!("Failed to add value node: {e}"))?;
                self.handles.push(handle);
                let id = handle.id();
                let truncated = truncate(&data, 30);
                let expl = explain.map_or_else(String::new, |e| format!("({e})"));
                self.sink
                    .println(&format!("Added value node {id}: \"{truncated}\" {expl}"));
                Ok(handle)
            }
            ["value"] => Err("Usage: node value <data> [--explain=text]".to_string()),
            ["alias", name, target_str, ..] => {
                let name = (*name).to_string();
                let target = self
                    .parse_handle(target_str)
                    .ok_or_else(|| format!("Invalid handle: {target_str}"))?;
                let handle = self.env.add_alias(name.clone(), target);
                self.handles.push(handle);
                let id = handle.id();
                let tid = target.id();
                self.sink
                    .println(&format!("Added alias {id}: {name} -> {tid}"));
                Ok(handle)
            }
            ["alias", ..] => Err("Usage: node alias <name> <target>".to_string()),
            [cmd, ..] => Err(format!("Unknown node subcommand: {cmd}")),
            [] => Err("Usage: node <add|value|alias|list> ...".to_string()),
        }
    }

    fn cmd_dep(&mut self, args: &[&str]) -> Result<(), String> {
        let (node_str, dep_str) = match args {
            [n, d, ..] => (*n, *d),
            _ => return Err("Usage: dep <node> <dependency>".to_string()),
        };
        let node = self
            .parse_handle(node_str)
            .ok_or_else(|| format!("Invalid handle: {node_str}"))?;
        let dep = self
            .parse_handle(dep_str)
            .ok_or_else(|| format!("Invalid handle: {dep_str}"))?;
        self.env
            .dag
            .write()
            .add_dependency(For(node), DependsOn(dep));
        let nid = node.id();
        let did = dep.id();
        self.sink
            .println(&format!("Added dependency: {nid} depends on {did}"));
        Ok(())
    }

    fn cmd_deps(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: deps <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        let dag = self.env.dag.read();
        let deps: Vec<_> = dag.get_direct_dependencies(handle).collect();
        let hid = handle.id();
        if deps.is_empty() {
            self.sink.println(&format!("Node {hid} has no dependencies"));
        } else {
            self.sink.println(&format!("Node {hid} depends on:"));
            for dep in deps {
                let node = dag.get_node(dep);
                let name = node.map_or("?", |n| n.idname.as_str());
                let did = dep.id();
                self.sink.println(&format!("  {did} ({name})"));
            }
        }
        Ok(())
    }

    fn cmd_show(&self, args: &[&str]) -> Result<(), String> {
        let dag = self.env.dag.read();
        if args.is_empty() {
            if self.handles.is_empty() {
                self.sink.println("No nodes");
                return Ok(());
            }
            let terminals: Vec<Handle> = self
                .handles
                .iter()
                .filter(|&&h| dag.get_direct_dependents(h).next().is_none())
                .copied()
                .collect();

            let suspension = Some(&*self.env.suspension);
            let roots = if terminals.is_empty() {
                self.handles.clone()
            } else {
                terminals
            };
            for handle in roots {
                let tree = dag.dump_colored(handle, suspension);
                for line in tree.lines() {
                    self.sink.println(line);
                }
            }
            return Ok(());
        }
        let handle_str = args.first().ok_or("Usage: show <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        let suspension = Some(&*self.env.suspension);
        let tree = dag.dump_colored(handle, suspension);
        for line in tree.lines() {
            self.sink.println(line);
        }
        Ok(())
    }

    fn cmd_run(&mut self, args: &[&str]) -> Result<(), String> {
        let mut one_step = false;
        let mut stop_before: Option<Handle> = None;
        let mut stop_after: Option<Handle> = None;
        let mut target_arg: Option<&str> = None;
        let mut bg_flag = false;
        let mut color: Option<u8> = None;

        let mut i = 0;
        while i < args.len() {
            let arg = args.get(i).ok_or("Internal error: index out of bounds")?;
            match arg {
                &"--one-step" => one_step = true,
                &"--bg" => bg_flag = true,
                &"--color" => {
                    i += 1;
                    let name = args.get(i).ok_or("--color requires a color name")?;
                    color = Some(parse_color(name)?);
                }
                &"--stop-before" => {
                    i += 1;
                    let h = args.get(i).ok_or("--stop-before requires a node")?;
                    stop_before = Some(
                        self.parse_handle(h)
                            .ok_or_else(|| format!("Invalid handle: {h}"))?,
                    );
                }
                &"--stop-after" => {
                    i += 1;
                    let h = args.get(i).ok_or("--stop-after requires a node")?;
                    stop_after = Some(
                        self.parse_handle(h)
                            .ok_or_else(|| format!("Invalid handle: {h}"))?,
                    );
                }
                arg if !arg.starts_with("--") => {
                    target_arg = Some(arg);
                }
                other => return Err(format!("Unknown option: {other}")),
            }
            i += 1;
        }

        let handle = if let Some(h) = target_arg {
            self.parse_handle(h)
                .ok_or_else(|| format!("Invalid handle: {h}"))?
        } else if let Some(sb) = stop_before {
            sb
        } else {
            self.find_default_target()?
        };
        let handle = self.env.resolve(handle);

        let stop_conditions = StopConditions {
            one_step,
            stop_before,
            stop_after,
        };

        self.executor
            .submit(handle, stop_conditions)
            .map_err(|_| "Executor has shut down".to_string())?;

        if bg_flag {
            self.attach_stdout_for_run(handle, one_step, stop_before, stop_after, true, color);
            self.sink.println("Started background run");
        } else {
            self.attach_stdout_for_run(handle, one_step, stop_before, stop_after, false, color);
            self.join_handle(handle)?;
        }

        self.sink.println("");
        Ok(())
    }

    /// Wait for `target` to terminate. Ctrl+C detaches; the node keeps running.
    ///
    /// Registers a `JoinWaiter` so the notification watcher signals us instead
    /// of printing a background notification for this particular node.
    fn join_handle(&mut self, target: Handle) -> Result<(), String> {
        // Bail early if already terminated.
        if matches!(
            self.env.dag.read().get_node(target).map(|n| n.state),
            Some(NodeState::Terminated)
        ) {
            return Ok(());
        }

        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<()>(1);
        *self.pending_join.lock().unwrap() = Some(JoinWaiter { target, ready_tx });

        let (abort_handle, abort_reg) = futures::future::AbortHandle::new_pair();
        let (ctrlc_tx, ctrlc_rx) = std::sync::mpsc::channel::<()>();

        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async {
                if Abortable::new(tokio::signal::ctrl_c(), abort_reg)
                    .await
                    .is_ok_and(|r| r.is_ok())
                {
                    let _ = ctrlc_tx.send(());
                }
            });
        });

        let result = loop {
            if ctrlc_rx.try_recv().is_ok() {
                *self.pending_join.lock().unwrap() = None;
                abort_handle.abort();
                self.sink
                    .println("\n^C - Detached (node continues running in ailetos)");
                break Ok(());
            }

            match ready_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(()) => {
                    abort_handle.abort();
                    break Ok(());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    abort_handle.abort();
                    break Ok(());
                }
            }
        };

        result
    }

    fn cmd_join(&mut self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: join <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        self.join_handle(handle)
    }

    fn cmd_follow(&mut self, args: &[&str]) -> Result<(), String> {
        let mut handle_str: Option<&str> = None;
        let mut color: Option<u8> = None;

        let mut i = 0;
        while i < args.len() {
            let arg = args[i];
            if arg == "--color" {
                i += 1;
                let name = args.get(i).ok_or("--color requires a color name")?;
                color = Some(parse_color(name)?);
            } else if arg.starts_with("--") {
                return Err(format!("Unknown option: {arg}"));
            } else if handle_str.is_none() {
                handle_str = Some(arg);
            } else {
                color = Some(parse_color(arg)?);
            }
            i += 1;
        }

        let handle_str = handle_str.ok_or("Usage: follow <node> [--color <name>]")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        let handle = self.env.resolve(handle);
        let writer: Box<dyn std::io::Write + Send + Sync> =
            Box::new(OutputSinkWriter::new(Arc::clone(&self.notification_sink), color));
        self.env.attach_stdout_to(handle, writer);
        Ok(())
    }

    fn attach_one_node(&mut self, handle: Handle, bg: bool, color: Option<u8>) {
        let resolved = self.env.resolve(handle);
        if bg {
            let writer: Box<dyn std::io::Write + Send + Sync> =
                Box::new(OutputSinkWriter::new(Arc::clone(&self.notification_sink), color));
            self.env.attach_stdout_to(resolved, writer);
        } else {
            self.env.attach_stdout(resolved);
        }
    }

    fn attach_stdout_for_run(
        &mut self,
        target: Handle,
        one_step: bool,
        stop_before: Option<Handle>,
        stop_after: Option<Handle>,
        bg: bool,
        color: Option<u8>,
    ) {
        if let Some(stop_after_handle) = stop_after {
            self.attach_one_node(stop_after_handle, bg, color);
        } else if let Some(stop_before_handle) = stop_before {
            let deps: Vec<Handle> = {
                let dag = self.env.dag.read();
                dag.get_direct_dependencies(stop_before_handle).collect()
            };
            for dep in deps {
                self.attach_one_node(dep, bg, color);
            }
        } else if one_step {
            let ready_node = {
                let dag = self.env.dag.read();
                TopologicalOrderIter::new(&dag, target).next()
            };
            if let Some(ready_node) = ready_node {
                self.attach_one_node(ready_node, bg, color);
            }
        } else {
            self.attach_one_node(target, bg, color);
        }
    }

    fn find_default_target(&self) -> Result<Handle, String> {
        if self.handles.is_empty() {
            return Err("No nodes to run".to_string());
        }
        let dag = self.env.dag.read();
        let terminals: Vec<Handle> = self
            .handles
            .iter()
            .filter(|&&h| dag.get_direct_dependents(h).next().is_none())
            .copied()
            .collect();
        match terminals.as_slice() {
            [] => Err("No terminal nodes found (circular dependencies?)".to_string()),
            [single] => Ok(*single),
            _ => {
                let ids: Vec<_> = terminals.iter().map(|h| h.id().to_string()).collect();
                Err(format!(
                    "Multiple terminal nodes: {}. Specify target explicitly.",
                    ids.join(", ")
                ))
            }
        }
    }

    fn cmd_cat(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: cat <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        let kv = Arc::clone(&self.kv);
        let output = self.ailetos_rt.block_on(async move {
            let path = pipe_path(handle, StdHandle::Stdout as isize);
            match kv.open(&path, OpenMode::Read).await {
                Ok(buffer) => {
                    let guard = buffer.lock();
                    Ok(String::from_utf8_lossy(&guard).into_owned())
                }
                Err(e) => Err(format!("No output available for node {}: {e:?}", handle.id())),
            }
        });
        match output {
            Ok(text) => self.sink.println(&text),
            Err(e) => self.sink.println(&e),
        }
        Ok(())
    }

    pub fn cmd_source(&mut self, args: &[&str]) -> Result<(), String> {
        let path = args.first().ok_or("Usage: source <file>")?;
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read {path}: {e}"))?;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            self.sink.println(&format!("dagsh> {line}"));
            match self.execute(line) {
                Ok(true) => {}
                Ok(false) => return Ok(()),
                Err(e) => self.sink.println(&format!("Error: {e}")),
            }
        }
        Ok(())
    }

    fn cmd_reset(&mut self) {
        shell_input_control::close_all_shell_inputs();
        for &handle in &self.handles {
            self.env.suspension.resume(handle);
        }

        let new_kv = Arc::new(MemKV::new());
        let new_env = make_env(&new_kv);
        let (new_executor, new_events_rx) =
            start_executor_with_bridge(&self.ailetos_rt, Arc::clone(&new_env));

        // Tell the watcher to switch to the new executor's event stream.
        self.watcher_update_tx
            .send(WatcherUpdate {
                events_rx: new_events_rx,
                env: Arc::clone(&new_env),
            })
            .ok();

        self.executor = new_executor;
        self.env = new_env;
        self.kv = new_kv;
        self.handles.clear();
        self.vars.clear();

        self.sink.println("DAG cleared.");
    }

    fn cmd_status(&self, args: &[&str]) -> Result<(), String> {
        let dag = self.env.dag.read();
        if args.is_empty() {
            let mut total = 0;
            let mut running = 0;
            let mut terminated = 0;
            let mut not_started = 0;
            let mut suspended = 0;

            for &handle in &self.handles {
                if let Some(node) = dag.get_node(handle) {
                    total += 1;
                    match node.state {
                        NodeState::Running => running += 1,
                        NodeState::Terminated => terminated += 1,
                        NodeState::NotStarted => not_started += 1,
                        NodeState::Terminating => {}
                    }
                    if self.env.suspension.is_suspended(handle) {
                        suspended += 1;
                    }
                }
            }
            self.sink.println(&format!("Nodes: {total} total, {not_started} pending, {running} running, {suspended} suspended, {terminated} terminated"));
        } else if let Some(handle_str) = args.first() {
            let handle = self
                .parse_handle(handle_str)
                .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
            let hid = handle.id();
            if let Some(node) = dag.get_node(handle) {
                let state = format_state(node.state);
                self.sink
                    .println(&format!("Node {hid}: {} [{state}]", node.idname));
            } else {
                self.sink.println(&format!("Node {hid} not found"));
            }
        }
        Ok(())
    }

    fn cmd_suspend(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: suspend <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        self.env.suspension.suspend(handle);
        self.sink.println(&format!("Suspended node {}", handle.id()));
        Ok(())
    }

    fn cmd_resume(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: resume <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;
        self.env.suspension.resume(handle);
        self.sink.println(&format!("Resumed node {}", handle.id()));
        Ok(())
    }

    fn cmd_wait(&self, args: &[&str]) -> Result<(), String> {
        let condition = args.first().ok_or("Usage: wait <condition> [args]")?;
        match *condition {
            "suspended" => {
                let handle_str = args.get(1).ok_or("Usage: wait suspended <node>")?;
                let handle = self
                    .parse_handle(handle_str)
                    .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

                let timeout = std::time::Duration::from_secs(5);
                let poll_interval = std::time::Duration::from_millis(10);
                let deadline = std::time::Instant::now() + timeout;

                loop {
                    if self.env.suspension.is_suspended(handle) {
                        return Ok(());
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(format!(
                            "Timeout: node {} not suspended after {}s",
                            handle.id(),
                            timeout.as_secs()
                        ));
                    }
                    std::thread::sleep(poll_interval);
                }
            }
            "terminated" => {
                let handle_str = args.get(1).ok_or("Usage: wait terminated <node>")?;
                let handle = self
                    .parse_handle(handle_str)
                    .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

                let timeout = std::time::Duration::from_secs(5);
                let poll_interval = std::time::Duration::from_millis(10);
                let deadline = std::time::Instant::now() + timeout;

                loop {
                    let state = self.env.dag.read().get_node(handle).map(|n| n.state);
                    if matches!(state, Some(NodeState::Terminated)) {
                        return Ok(());
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(format!(
                            "Timeout: node {} not terminated after {}s",
                            handle.id(),
                            timeout.as_secs()
                        ));
                    }
                    std::thread::sleep(poll_interval);
                }
            }
            other => Err(format!("Unknown wait condition: {other}")),
        }
    }

    fn cmd_write(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: write <node> <data>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        let data = parse_quoted_string(args.get(1..).unwrap_or(&[]));

        match shell_input_control::write_to_shell_input(handle, data.into_bytes()) {
            Ok(()) => {
                let hid = handle.id();
                self.sink.println(&format!("Wrote data to node {hid}"));
                Ok(())
            }
            Err(e) => Err(format!("Failed to write: {e}")),
        }
    }

    fn cmd_close(&self, args: &[&str]) -> Result<(), String> {
        let handle_str = args.first().ok_or("Usage: close <node>")?;
        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        match shell_input_control::close_shell_input(handle) {
            Ok(()) => {
                let hid = handle.id();
                self.sink.println(&format!("Closed node {hid}"));
                Ok(())
            }
            Err(e) => Err(format!("Failed to close: {e}")),
        }
    }

    fn cmd_kill(&mut self, args: &[&str]) -> Result<(), String> {
        let handle_str = match args {
            [flag, node] if flag.starts_with('-') => *node,
            [node] => *node,
            _ => return Err("Usage: kill [-N] <node>".to_string()),
        };

        let handle = self
            .parse_handle(handle_str)
            .ok_or_else(|| format!("Invalid handle: {handle_str}"))?;

        if !dbg_control::is_dbg_node(handle) {
            return Err("kill is only supported for dbg nodes".to_string());
        }

        dbg_control::kill_dbg_actor(handle);
        self.env.suspension.resume(handle);

        self.sink.println(&format!("Killed node {}", handle.id()));
        Ok(())
    }

    fn prepare_exit(&mut self) {
        shell_input_control::close_all_shell_inputs();
        for &handle in &self.handles {
            self.env.suspension.resume(handle);
        }
    }
}

impl Default for DagShell {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DagShell {
    fn drop(&mut self) {
        self.prepare_exit();
        // Dropping watcher_update_tx closes the update channel, causing the
        // watcher thread to exit its loop. executor and ailetos_rt then drop
        // in declaration order, closing the job channel and cancelling tasks.
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_explain(args: &[&str]) -> Option<String> {
    let joined = args.join(" ");
    let rest = joined.strip_prefix("--explain=").or_else(|| {
        joined
            .find("--explain=")
            .map(|pos| &joined[pos + "--explain=".len()..])
    })?;
    if let Some(quoted) = rest.strip_prefix('"') {
        if let Some(end) = quoted.find('"') {
            return quoted.get(..end).map(str::to_string);
        }
        return Some(quoted.to_string());
    }
    rest.split_whitespace().next().map(str::to_string)
}

fn parse_bytes_before_pause(args: &[&str]) -> Option<usize> {
    args.iter()
        .find_map(|a| a.strip_prefix("--bytes-before-pause="))
        .and_then(|s| s.parse().ok())
}

fn parse_quoted_string(args: &[&str]) -> String {
    let joined = args.join(" ");
    let value_part = joined.find("--explain=").map_or_else(
        || joined.trim(),
        |pos| joined.get(..pos).unwrap_or("").trim(),
    );
    value_part.trim_matches('"').to_string()
}

fn format_state(state: NodeState) -> &'static str {
    match state {
        NodeState::NotStarted => "⋯ pending",
        NodeState::Running => "⚙ running",
        NodeState::Terminating => "⏳ terminating",
        NodeState::Terminated => "✓ built",
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}
