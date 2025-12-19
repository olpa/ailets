//! MemPipe CLI Demo
//!
//! Demonstrates the notification queue and mempipe implementation.
//! Equivalent to the Python main() in mempipe.py

use ailetos::mempipe::MemPipe;
use ailetos::notification_queue::{Handle, NotificationQueue, QueueConfig};
use embedded_io_async::Read;
use std::io::{self, BufRead};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Create notification queue
    let queue = NotificationQueue::new(QueueConfig::default());

    // Client creates handles with explicit IDs
    let writer_handle = Handle::new(0);
    let reader1_handle = Handle::new(1);
    let reader2_handle = Handle::new(2);
    let reader3_handle = Handle::new(3);

    // Register handles with queue
    queue.register_handle_with_id(writer_handle);
    queue.register_handle_with_id(reader1_handle);
    queue.register_handle_with_id(reader2_handle);
    queue.register_handle_with_id(reader3_handle);

    // Create mempipe
    let pipe = MemPipe::new(
        writer_handle,
        queue.clone(),
        None,
    );

    // Create readers with explicit handles
    let mut reader1 = pipe.get_reader(reader1_handle);
    let mut reader2 = pipe.get_reader(reader2_handle);
    let mut reader3 = pipe.get_reader(reader3_handle);

    // Spawn writer task
    let writer_task = tokio::spawn(async move {
        println!("Enter text (empty line to quit):");
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();

        while let Some(Ok(line)) = lines.next() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }

            if let Err(e) = pipe.writer().write_sync(trimmed.as_bytes()) {
                eprintln!("Write error: {}", e);
                break;
            }
        }

        pipe.writer().close().ok();
        println!("Writer closed");
    });

    // Spawn reader tasks
    let reader1_task = tokio::spawn(async move {
        read_all("r1", &mut reader1).await;
    });

    let reader2_task = tokio::spawn(async move {
        read_all("r2", &mut reader2).await;
    });

    let reader3_task = tokio::spawn(async move {
        read_all("r3", &mut reader3).await;
    });

    // Wait for all tasks
    let _ = tokio::join!(writer_task, reader1_task, reader2_task, reader3_task);

    // Unregister reader handles first, then writer handle
    queue.unregister_handle(reader1_handle);
    queue.unregister_handle(reader2_handle);
    queue.unregister_handle(reader3_handle);
    queue.unregister_handle(writer_handle);

    println!("All tasks completed");
    Ok(())
}

async fn read_all(name: &str, reader: &mut impl Read) {
    let mut buf = [0u8; 4];

    loop {
        match reader.read(&mut buf).await {
            Ok(0) => {
                println!("({}) EOF", name);
                break;
            }
            Ok(n) => {
                let data = String::from_utf8_lossy(&buf[..n]);
                println!("({}): {}", name, data);
            }
            Err(e) => {
                eprintln!("({}) Error: {}", name, e);
                break;
            }
        }
    }
}
