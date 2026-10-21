use std::time::Duration;

use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
    time::timeout,
};

// Public API that our test expects.
use bigwig_mq::{framing::write_frame, server::serve};

use le_rove_protocol::fixtures::{STOPPED_UNKNOWN, stopped_unknown};
use le_rove_protocol::frame::Frame;

/// payload: Train transmits a valid SPEC-001 payload.
async fn deliver_via_bigwig(payload: Vec<u8>) -> Vec<u8> {
    // Arrange: BigwigMQ server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

    let address = listener.local_addr().unwrap();

    let (brain_tx, mut brain_rx) = mpsc::channel::<Vec<u8>>(8);

    let server = tokio::spawn(serve(listener, brain_tx));

    // Arrange: simulated RF
    let (rf_tx, mut rf_rx) = mpsc::channel::<Vec<u8>>(8);

    rf_tx.send(payload).await.unwrap();

    // Station receives RF and forwards it over TCP.
    let mut station = TcpStream::connect(address).await.unwrap();

    let rf_payload = rf_rx.recv().await.unwrap();

    write_frame(&mut station, &rf_payload).await.unwrap();

    // Brain receives the original payload.
    let received = timeout(Duration::from_secs(2), brain_rx.recv())
        .await
        .expect("Brain timed out")
        .expect("Brain channel closed");

    server.abort();

    received
}

#[tokio::test]
async fn train_frame_reaches_brain_unchanged() {
    let received = deliver_via_bigwig(STOPPED_UNKNOWN.to_vec()).await;

    assert_eq!(received, STOPPED_UNKNOWN);
}

#[tokio::test]
async fn brain_decodes_stopped_event() {
    let received = deliver_via_bigwig(STOPPED_UNKNOWN.to_vec()).await;
    let frame = Frame::try_from(received.as_slice()).expect("Failed to decode frame");
    println!("Decoded frame: {:?}", frame);

    assert_eq!(frame, stopped_unknown());
}
