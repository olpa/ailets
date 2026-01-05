//! MemPipe CLI Demo
//!
//! Demonstrates the notification queue and mempipe implementation.
//! Equivalent to the Python main() in mempipe.py

use ailetos::mempipe::{MemPipe, PipeBuffer, Reader};
use ailetos::notification_queue::{Handle, NotificationQueueArc};
use std::io::{self, BufRead};

// Wrapper type for Vec<u8> to implement PipeBuffer
struct VecBuffer(Vec<u8>);

impl VecBuffer {
    fn new() -> Self {
        Self(Vec::new())
    }
}

impl PipeBuffer for VecBuffer {
    fn write(&mut self, data: &[u8]) -> isize {
        self.0.extend_from_slice(data);
        data.len() as isize
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let queue = NotificationQueueArc::new();

    let pipe = MemPipe::new(Handle::new(0), queue.clone(), "demo", VecBuffer::new());

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

async fn write_all(pipe: MemPipe<VecBuffer>) {
    println!("Enter text (empty line to quit):");
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    while let Some(Ok(line)) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }

        let result = pipe.writer().write_sync(trimmed.as_bytes());
        if result < 0 {
            eprintln!("Write error: errno={}", pipe.writer().get_error());
            break;
        }
    }

    pipe.writer().close();
    println!("Writer closed");
}

async fn read_all(name: &str, reader: &mut Reader<VecBuffer>) {
    let mut buf = [0u8; 4];

    loop {
        let result = reader.read(&mut buf).await;

        if result == 0 {
            println!("({}) EOF", name);
            break;
        } else if result < 0 {
            eprintln!("({}) Error: errno={}", name, reader.get_error());
            break;
        } else {
            let n = result as usize;
            let data = String::from_utf8_lossy(&buf[..n]);
            println!("({}): {}", name, data);
        }
    }
}
