//! MemPipe CLI Demo
//!
//! Demonstrates the notification queue and mempipe implementation.
//! Equivalent to the Python main() in mempipe.py

use ailetos::mempipe::MemPipe;
use ailetos::notification_queue::{NotificationQueue, QueueConfig};
use embedded_io_async::Read;
use std::io::{self, BufRead};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Create notification queue
    let queue = NotificationQueue::new(QueueConfig::default());

    // Register writer handle
    let writer_guard = queue.register_handle("writer");

    // Create mempipe
    let pipe = MemPipe::new(
        writer_guard.handle().clone(),
        queue.clone(),
        "main",
        None,
    );

    // Create readers (they all use the writer's handle for notifications)
    let mut reader1 = pipe.create_reader("r1");
    let mut reader2 = pipe.create_reader("r2");
    let mut reader3 = pipe.create_reader("r3");

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
                print!("({}): {}", name, data);
                io::Write::flush(&mut io::stdout()).ok();
            }
            Err(e) => {
                eprintln!("({}) Error: {}", name, e);
                break;
            }
        }
    }
}
