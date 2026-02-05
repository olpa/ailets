//! Pipe CLI Demo
//!
//! Demonstrates the notification queue and pipe implementation.

use std::cmp::Ordering;

use ailetos::notification_queue::{Handle, NotificationQueueArc};
use ailetos::pipe::{Pipe, Reader};
use ailetos::Buffer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let queue = NotificationQueueArc::new();

    let pipe = Pipe::new(Handle::new(0), queue.clone(), "demo", Buffer::new());

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

async fn write_all(pipe: Pipe) {
    println!("Enter text (empty line to quit):");

    let stdin = tokio::io::stdin();
    let reader = tokio::io::BufReader::new(stdin);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);

    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }

        let result = pipe.writer().write(trimmed.as_bytes());
        if result < 0 {
            eprintln!("Write error: errno={}", pipe.writer().get_error());
            break;
        }
    }

    pipe.writer().close();
    println!("Writer closed");
}

async fn read_all(name: &str, reader: &mut Reader) {
    let mut buf = [0u8; 4];

    loop {
        let result = reader.read(&mut buf).await;

        match result.cmp(&0) {
            Ordering::Equal => {
                println!("({name}) EOF");
                break;
            }
            Ordering::Less => {
                let errno = reader.get_error();
                eprintln!("({name}) Error: errno={errno}");
                break;
            }
            Ordering::Greater => {
                let n = result.cast_unsigned();
                // SAFETY: result > 0 and result is the number of bytes read into buf,
                // so 0 < n <= buf.len(), making buf[..n] always valid
                #[allow(clippy::indexing_slicing)]
                let data = String::from_utf8_lossy(&buf[..n]);
                println!("({name}): {data}");
            }
        }
    }
}
