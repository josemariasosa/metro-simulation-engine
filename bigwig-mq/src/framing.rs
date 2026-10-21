use std::io;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const MAX_FRAME_SIZE: usize = 1024;

pub async fn read_frame(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    // Read exactly two bytes for the length.
    let length = stream.read_u16().await? as usize;

    if length > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds maximum size",
        ));
    }

    // Allocate enough memory for the payload.
    let mut payload = vec![0u8; length];

    // Wait until all payload bytes arrive.
    stream.read_exact(&mut payload).await?;

    Ok(payload)
}

pub async fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame exceeds maximum size",
        ));
    }

    stream.write_u16(payload.len() as u16).await?;
    stream.write_all(payload).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn frame_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();

            let message = read_frame(&mut stream).await.unwrap();
            assert_eq!(message, b"HELLO");

            write_frame(&mut stream, &message).await.unwrap();
        });

        let mut client = TcpStream::connect(addr).await.unwrap();

        write_frame(&mut client, b"HELLO").await.unwrap();

        let response = read_frame(&mut client).await.unwrap();
        assert_eq!(response, b"HELLO");

        server.await.unwrap();
    }
}
