//! PipePool - manages output pipes for actors
//!
//! Each actor has one output pipe. Readers are created on-demand
//! when consuming actors need to read from dependencies.

use crate::idgen::{Handle, IdGen};
use crate::io::{KVBuffers, OpenMode};
use crate::notification_queue::NotificationQueueArc;
use crate::pipe::{Pipe, Reader, Writer};

/// Pool of output pipes, one per actor (identified by Handle)
pub struct PipePool<K: KVBuffers> {
    /// Output pipes indexed by producing actor's handle
    pipes: Vec<(Handle, Pipe)>,
    /// Shared notification queue for all pipes
    notification_queue: NotificationQueueArc,
    /// Key-value store for pipe buffers
    kv: K,
}

impl<K: KVBuffers> PipePool<K> {
    /// Create a new empty pipe pool
    #[must_use]
    pub fn new(kv: K, notification_queue: NotificationQueueArc) -> Self {
        Self {
            pipes: Vec::new(),
            notification_queue,
            kv,
        }
    }

    /// Create an output pipe for the given actor
    ///
    /// # Panics
    /// Panics if the actor already has an output pipe
    pub async fn create_output_pipe(&mut self, actor_handle: Handle, name: &str, id_gen: &IdGen) {
        // Check if actor already has a pipe
        if self.pipes.iter().any(|(h, _)| *h == actor_handle) {
            panic!("Actor {actor_handle:?} already has an output pipe");
        }

        let writer_handle = Handle::new(id_gen.get_next());

        // Get buffer from KV store
        let buffer = self
            .kv
            .open(name, OpenMode::Write)
            .await
            .expect("Failed to create buffer in KV store");

        let pipe = Pipe::new(writer_handle, self.notification_queue.clone(), name, buffer);
        self.pipes.push((actor_handle, pipe));
    }

    /// Check if a pipe exists for the given actor
    #[must_use]
    pub fn has_pipe(&self, actor_handle: Handle) -> bool {
        self.pipes.iter().any(|(h, _)| *h == actor_handle)
    }

    /// Get the pipe for the given actor
    ///
    /// # Panics
    /// Panics if the actor doesn't have an output pipe
    #[must_use]
    pub fn get_pipe(&self, actor_handle: Handle) -> &Pipe {
        self.pipes
            .iter()
            .find(|(h, _)| *h == actor_handle)
            .map(|(_, pipe)| pipe)
            .unwrap_or_else(|| panic!("Actor {actor_handle:?} doesn't have an output pipe"))
    }

    /// Open a reader for the given actor's output pipe
    ///
    /// Creates a new Reader instance. Multiple readers can be created
    /// for the same pipe.
    ///
    /// # Panics
    /// Panics if the actor doesn't have an output pipe
    #[must_use]
    pub fn open_reader(&self, actor_handle: Handle, id_gen: &IdGen) -> Reader {
        let pipe = self.get_pipe(actor_handle);
        let reader_handle = Handle::new(id_gen.get_next());
        pipe.get_reader(reader_handle)
    }

    /// Create a standalone Writer backed by KV storage (not wrapped in a Pipe).
    ///
    /// Used to create merge writers for actors with multiple dependencies.
    pub async fn create_merge_writer(&mut self, name: &str, id_gen: &IdGen) -> Writer {
        let writer_handle = Handle::new(id_gen.get_next());
        let buffer = self
            .kv
            .open(name, OpenMode::Write)
            .await
            .expect("Failed to create merge buffer in KV store");
        Writer::new(writer_handle, self.notification_queue.clone(), name, buffer)
    }

    /// Flush the buffer for the given actor's pipe
    ///
    /// # Errors
    /// Returns an error if flushing fails
    pub fn flush_buffer(&self, actor_handle: Handle) -> Result<(), crate::io::KVError> {
        let pipe = self.get_pipe(actor_handle);
        let buffer = pipe.writer().buffer();
        self.kv.flush_buffer(&buffer)
    }
}
