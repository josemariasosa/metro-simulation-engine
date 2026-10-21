use std::io;

use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use crate::framing::read_frame;

pub async fn serve(listener: TcpListener, tx: mpsc::Sender<Vec<u8>>) -> io::Result<()> {
    loop {
        let (socket, peer) = listener.accept().await?;
        let tx = tx.clone();

        tokio::spawn(async move {
            if let Err(err) = handle_connection(socket, tx).await {
                eprintln!("{peer}: {err}");
            }
        });
    }
}

async fn handle_connection(mut socket: TcpStream, tx: mpsc::Sender<Vec<u8>>) -> io::Result<()> {
    loop {
        let payload = read_frame(&mut socket).await?;

        println!("Received frame: {payload:?}");

        tx.send(payload).await.map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "Brain receiver disconnected")
        })?;
    }
}
