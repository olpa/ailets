//! Flush coordinator for serializing write operations
//!
//! This module provides a channel-based coordinator that serializes flush operations
//! to prevent race conditions. It spawns a single writer task that processes flush
//! requests sequentially via a bounded mpsc channel.
//!
//! # Architecture
//!
//! ```text
//! Multiple callers → FlushCoordinator (mpsc channel) → Single Writer Task → Backend
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! let flush_fn = |path: String, data: Vec<u8>| -> Result<(), String> {
//!     // Perform actual write operation
//!     write_to_backend(path, data)
//! };
//!
//! let coordinator = FlushCoordinator::new(100, flush_fn);
//!
//! // Submit flush requests (non-blocking for caller)
//! coordinator.flush("path/to/file".into(), vec![1, 2, 3]).await?;
//!
//! // Gracefully shutdown before drop
//! coordinator.shutdown().await?;
//! ```

use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, trace, warn};

/// Request to flush data to persistent storage
struct FlushRequest {
    /// Path to flush to
    path: String,
    /// Data to write
    data: Vec<u8>,
    /// Channel to send result back to caller
    response: oneshot::Sender<Result<(), String>>,
}

/// Trait for flush functions - allows different backends (`SQLite`, S3, etc.)
///
/// Implementors must be thread-safe and callable multiple times.
pub trait FlushFn: Fn(String, Vec<u8>) -> Result<(), String> + Send + Sync {}

// Blanket implementation for all compatible closures
impl<T> FlushFn for T where T: Fn(String, Vec<u8>) -> Result<(), String> + Send + Sync {}

/// Coordinates flush operations to ensure serialization
///
/// This coordinator spawns a single writer task that processes flush
/// requests sequentially via a bounded mpsc channel. This eliminates
/// race conditions by ensuring only one flush operation executes at a time.
///
/// # Lifecycle
///
/// 1. Create with `new()` - spawns background writer task
/// 2. Use `flush()` to submit flush requests
/// 3. Call `shutdown()` before drop to gracefully drain pending requests
/// 4. If dropped without shutdown, logs error but doesn't panic
///
/// # Thread Safety
///
/// The coordinator can be shared across threads when wrapped in `Arc`.
/// The flush function must be `Send + Sync`.
pub struct FlushCoordinator<F>
where
    F: FlushFn + 'static,
{
    /// Channel to send flush requests to writer task
    request_tx: Option<mpsc::Sender<FlushRequest>>,
    /// Join handle for writer task (for shutdown)
    writer_task: Option<tokio::task::JoinHandle<()>>,
    /// Function to perform actual flush operation (kept alive via Arc)
    #[allow(dead_code)]
    flush_fn: Arc<F>,
}

/// Errors that can occur during coordinator operations
#[derive(Debug)]
pub enum CoordinatorError {
    /// Channel is full (backpressure)
    ChannelFull,
    /// Coordinator has been shut down
    Shutdown,
    /// Flush operation failed
    FlushFailed(String),
}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChannelFull => write!(f, "Flush coordinator channel full"),
            Self::Shutdown => write!(f, "Flush coordinator shut down"),
            Self::FlushFailed(msg) => write!(f, "Flush failed: {msg}"),
        }
    }
}

impl std::error::Error for CoordinatorError {}

impl<F> FlushCoordinator<F>
where
    F: FlushFn + 'static,
{
    /// Create new coordinator with bounded channel
    ///
    /// Spawns a background writer task that processes flush requests sequentially.
    ///
    /// # Parameters
    /// - `capacity`: Channel capacity (controls backpressure)
    /// - `flush_fn`: Function to perform actual flush operation
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let flush_fn = |path: String, data: Vec<u8>| -> Result<(), String> {
    ///     std::fs::write(&path, &data)
    ///         .map_err(|e| format!("write error: {e}"))
    /// };
    ///
    /// let coordinator = FlushCoordinator::new(100, flush_fn);
    /// ```
    pub fn new(capacity: usize, flush_fn: F) -> Self {
        trace!(capacity, "FlushCoordinator::new: creating, will store request_tx, writer_task, flush_fn");
        let (request_tx, request_rx) = mpsc::channel(capacity);
        let flush_fn = Arc::new(flush_fn);

        // Spawn writer task
        let writer_flush_fn = Arc::clone(&flush_fn);
        let writer_task = tokio::spawn(async move {
            Self::writer_loop(request_rx, writer_flush_fn).await;
        });

        debug!(capacity, "FlushCoordinator created");

        Self {
            request_tx: Some(request_tx),
            writer_task: Some(writer_task),
            flush_fn,
        }
    }

    /// Writer task event loop - processes flush requests sequentially
    ///
    /// This runs in a background tokio task and processes requests until
    /// the channel is closed (when all senders are dropped).
    async fn writer_loop(mut request_rx: mpsc::Receiver<FlushRequest>, flush_fn: Arc<F>) {
        debug!("Writer task started");
        trace!("FlushCoordinator::writer_loop: entering request_rx loop");

        let mut requests_processed = 0;

        // Process requests until channel closes
        while let Some(request) = request_rx.recv().await {
            trace!("FlushCoordinator::writer_loop: received request from request_rx");
            requests_processed += 1;

            debug!(
                path = %request.path,
                data_len = request.data.len(),
                request_num = requests_processed,
                "Processing flush request"
            );

            // Perform blocking flush operation
            let result = tokio::task::spawn_blocking({
                let path = request.path.clone();
                let data = request.data.clone();
                let flush_fn = Arc::clone(&flush_fn);
                move || flush_fn(path, data)
            })
            .await;

            // Handle task panic or flush error
            let flush_result = match result {
                Ok(Ok(())) => {
                    debug!(path = %request.path, "Flush completed successfully");
                    Ok(())
                }
                Ok(Err(e)) => {
                    error!(path = %request.path, error = %e, "Flush operation failed");
                    Err(e)
                }
                Err(panic_err) => {
                    error!(
                        path = %request.path,
                        error = %panic_err,
                        "Flush task panicked"
                    );
                    Err("flush task panicked".to_string())
                }
            };

            // Send result back (ignore if receiver dropped - caller gave up waiting)
            let _ = request.response.send(flush_result);
        }

        trace!("FlushCoordinator::writer_loop: exited request_rx loop");
        debug!(requests_processed, "Writer task exiting - channel closed");
    }

    /// Submit a flush request
    ///
    /// Sends a flush request to the writer task and awaits the result.
    /// This method returns quickly for the caller - the actual flush happens
    /// in the background writer task.
    ///
    /// # Parameters
    /// - `path`: Path to flush to
    /// - `data`: Data to write
    ///
    /// # Returns
    /// - `Ok(Ok(()))` - Flush succeeded
    /// - `Ok(Err(msg))` - Flush failed with error message
    /// - `Err(CoordinatorError)` - Channel error (full, shutdown, etc.)
    ///
    /// # Errors
    /// - `CoordinatorError::ChannelFull` if channel capacity reached (backpressure)
    /// - `CoordinatorError::Shutdown` if coordinator shut down
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// match coordinator.flush("path".into(), vec![1, 2, 3]).await {
    ///     Ok(Ok(())) => println!("Flushed successfully"),
    ///     Ok(Err(e)) => eprintln!("Flush failed: {}", e),
    ///     Err(CoordinatorError::ChannelFull) => {
    ///         // Backpressure - wait and retry
    ///         tokio::time::sleep(Duration::from_millis(100)).await;
    ///     }
    ///     Err(e) => eprintln!("Coordinator error: {}", e),
    /// }
    /// ```
    pub async fn flush(
        &self,
        path: String,
        data: Vec<u8>,
    ) -> Result<Result<(), String>, CoordinatorError> {
        let request_tx = self.request_tx.as_ref().ok_or(CoordinatorError::Shutdown)?;

        let (response_tx, response_rx) = oneshot::channel();

        let request = FlushRequest {
            path: path.clone(),
            data,
            response: response_tx,
        };

        // Try to send (fails if channel full or closed)
        request_tx
            .send(request)
            .await
            .map_err(|_| CoordinatorError::ChannelFull)?;

        debug!(path = %path, "Flush request submitted");

        // Wait for response from writer task
        response_rx.await.map_err(|_| CoordinatorError::Shutdown)
    }

    /// Gracefully shutdown coordinator
    ///
    /// Closes the request channel to prevent new requests, then waits up to
    /// 5 seconds for pending requests to complete. If the timeout expires,
    /// returns an error.
    ///
    /// This consumes the coordinator to prevent use after shutdown.
    ///
    /// # Returns
    /// - `Ok(())` - Shutdown completed successfully
    /// - `Err(msg)` - Shutdown failed (timeout or task panic)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// coordinator.shutdown().await.expect("shutdown failed");
    /// ```
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Writer task doesn't complete within 5 seconds
    /// - Writer task panicked
    pub async fn shutdown(mut self) -> Result<(), String> {
        debug!("Shutdown initiated");

        // Drop sender to close channel (signals writer to exit after draining)
        drop(self.request_tx.take());

        // Wait for writer task with 5-second timeout
        let writer_task = self
            .writer_task
            .take()
            .ok_or("writer task already completed")?;

        match timeout(Duration::from_secs(5), writer_task).await {
            Ok(Ok(())) => {
                debug!("Shutdown completed successfully");
                Ok(())
            }
            Ok(Err(join_error)) => {
                error!(error = %join_error, "Writer task panicked during shutdown");
                Err(format!("writer task panicked: {join_error}"))
            }
            Err(_timeout) => {
                warn!("Shutdown timeout after 5 seconds - pending requests may be lost");
                Err("shutdown timeout after 5 seconds".to_string())
            }
        }
    }

    /// Check if coordinator is shut down
    ///
    /// Returns `true` if shutdown has been called or is in progress.
    #[must_use]
    pub fn is_shutdown(&self) -> bool {
        self.request_tx.is_none()
    }
}

impl<F> Drop for FlushCoordinator<F>
where
    F: FlushFn + 'static,
{
    fn drop(&mut self) {
        trace!("FlushCoordinator: destroying (drop)");
        // Check if shutdown was called (both fields should be None after shutdown)
        if self.request_tx.is_some() || self.writer_task.is_some() {
            error!(
                "FlushCoordinator dropped without calling shutdown(). \
                 Pending requests may be lost. This is a bug in the caller. \
                 Always call shutdown() before dropping the coordinator."
            );

            // Best effort cleanup: drop sender to signal writer to stop
            drop(self.request_tx.take());

            // Cannot await in Drop, so abort the task
            if let Some(task) = self.writer_task.take() {
                task.abort();
                warn!("Writer task aborted due to drop without shutdown");
            }
        }
    }
}
