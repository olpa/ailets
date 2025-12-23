//! MemPipe CLI Demo
//!
//! Demonstrates the notification queue and mempipe implementation.
//! Equivalent to the Python main() in mempipe.py

use ailetos::mempipe::MemPipe;
use ailetos::notification_queue::{Handle, NotificationQueueArc};
use embedded_io_async::Read;
use std::io::{self, BufRead};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let queue = NotificationQueueArc::new();

    let pipe = MemPipe::new(
        Handle::new(0),
        queue.clone(),
        None,
    );

    let mut reader1 = pipe.get_reader(Handle::new(1));
    let mut reader2 = pipe.get_reader(Handle::new(2));
    let mut reader3 = pipe.get_reader(Handle::new(3));

    let writer_task = tokio::spawn(async move {
        write_all(pipe).await;
    });

    let reader1_task = tokio::spawn(async move {
        read_all("r1", &mut reader1).await;
    });

    let reader2_task = tokio::spawn(async move {
        read_all("r2", &mut reader2).await;
    });

    let reader3_task = tokio::spawn(async move {
        read_all("r3", &mut reader3).await;
    });

    let _ = tokio::join!(writer_task, reader1_task, reader2_task, reader3_task);

    println!("All tasks completed");
    Ok(())
}

async fn write_all(pipe: MemPipe) {
    println!("Enter text (empty line to quit):");
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    while let Some(Ok(line)) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }

        if let Err(e) = pipe.writer().write(trimmed.as_bytes()) {
            eprintln!("Write error: {}", e);
            break;
        }
    }

    pipe.writer().close().expect("Failed to close writer");
    println!("Writer closed");
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
