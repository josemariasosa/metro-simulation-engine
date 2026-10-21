mod framing;
mod server;

use std::io;

use tokio::{net::TcpListener, sync::mpsc};

// #[tokio::main]
// async fn main() -> io::Result<()> {
//     let listener = TcpListener::bind("127.0.0.1:5000").await?;

//     println!("BigwigMQ listening on 127.0.0.1:5000");

//     let address = listener.local_addr()?;

//     let (tx, mut rx) = mpsc::channel::<Vec<u8>>(32);
//     crate::server::serve(listener, tx).await;
// }

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:5000").await?;

    println!("BigwigMQ listening on {}", listener.local_addr()?);

    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(32);

    // Temporary Brain task.
    tokio::spawn(async move {
        while let Some(payload) = rx.recv().await {
            println!("BRAIN RX: {payload:02X?}");
        }
    });

    server::serve(listener, tx).await
}
