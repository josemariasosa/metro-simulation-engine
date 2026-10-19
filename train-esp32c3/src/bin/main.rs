#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embedded_hal::spi::SpiDevice;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::{
        master::{Config as SpiConfig, Spi},
        Mode,
    },
    time::{Duration, Instant, Rate},
};
use le_rove_protocol::{
    codec::{decode, encode},
    frame::Frame,
    identity::{NodeId, TrainId},
    message::{Event, Location, Message, StopReason},
};
use log::info;
use nrf24_driver::{Nrf24, TxConfig};

esp_bootloader_esp_idf::esp_app_desc!();

// nRF24L01+ registers
const FIFO_STATUS: u8 = 0x17;
const STATUS: u8 = 0x07;

// Register bits
const RX_DR: u8 = 1 << 6;
const TX_DS: u8 = 1 << 5;
const MAX_RT: u8 = 1 << 4;
const TX_EMPTY: u8 = 1 << 4;

fn delay(duration: Duration) {
    let start = Instant::now();
    while start.elapsed() < duration {}
}

fn configure_radio<SPI>(radio: &mut Nrf24<SPI>)
where
    SPI: SpiDevice,
    SPI::Error: core::fmt::Debug,
{
    let config = TxConfig::new(50, [0xE7; 5]).unwrap();

    radio.configure_tx(&config).unwrap();

    // PWR_UP requires up to 1.5 ms before TX.
    delay(Duration::from_millis(2));

    info!("RF configured: channel=50, PTX, 1Mbps");
}

fn protocol_self_test() -> ([u8; 32], usize) {
    // SPEC-001 fixture; not a real observed train event.
    let original = Frame {
        source: NodeId::new(2).unwrap(),
        destination: NodeId::new(1).unwrap(),
        command_id: None,
        message: Message::Event(Event::Stopped {
            train_id: TrainId::new(3).unwrap(),
            reason: StopReason::Unspecified,
            location: Location::Unknown,
        }),
    };

    let mut buffer = [0u8; 32];
    let written = encode(&original, &mut buffer).unwrap();

    // Round-trip verification.
    let recovered = decode(&buffer[..written]).unwrap();
    assert_eq!(original, recovered);

    // Independent SPEC-001 fixed vector.
    const EXPECTED: [u8; 18] = [
        0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04, 0x00, 0x00,
        0x00, 0x00, 0x00,
    ];

    assert_eq!(&buffer[..written], &EXPECTED);

    info!("SPEC-001 encode/decode: PASS");
    info!("Payload: {:02X?}", &buffer[..written]);

    (buffer, written)
}

fn transmit<SPI>(radio: &mut Nrf24<SPI>, ce: &mut Output<'_>, payload: &[u8])
where
    SPI: SpiDevice,
    SPI::Error: core::fmt::Debug,
{
    // Start with a clean TX FIFO.
    radio.flush_tx().unwrap();

    let fifo = radio.read_register(FIFO_STATUS).unwrap();
    assert_ne!(fifo & TX_EMPTY, 0);

    // Clear stale interrupt flags (write-one-to-clear).
    radio
        .write_register(STATUS, RX_DR | TX_DS | MAX_RT)
        .unwrap();

    // Load exactly one payload.
    radio.write_payload(payload).unwrap();

    let fifo = radio.read_register(FIFO_STATUS).unwrap();
    assert_eq!(fifo & TX_EMPTY, 0);

    info!("TX FIFO loaded: {} bytes", payload.len());

    // Trigger TX with a CE pulse.
    ce.set_high();
    delay(Duration::from_micros(20));
    ce.set_low();

    // Allow the radio to complete the packet.
    delay(Duration::from_millis(2));

    let status = radio.read_register(STATUS).unwrap();
    let tx_complete = status & TX_DS != 0;
    let max_retries = status & MAX_RT != 0;

    info!("STATUS after TX: 0x{:02X}", status);
    info!("TX_DS: {}", tx_complete);
    info!("MAX_RT: {}", max_retries);

    // No ACK required in this experiment.
    assert!(tx_complete);
    assert!(!max_retries);

    // Confirm the payload has left the TX FIFO.
    let fifo = radio.read_register(FIFO_STATUS).unwrap();
    assert_ne!(fifo & TX_EMPTY, 0);

    // Clear TX completion flag.
    radio.write_register(STATUS, TX_DS).unwrap();

    info!("RF transmission completed: PASS");
}

#[allow(
    clippy::large_stack_frames,
    reason = "Embedded initialization and fixed-size buffers"
)]
#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());

    let peripherals = esp_hal::init(config);

    // Radio control pins
    let csn = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());

    let mut ce = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());

    // SPI bus
    let spi_config = SpiConfig::default()
        .with_frequency(Rate::from_khz(500))
        .with_mode(Mode::_0);

    let spi = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO8)
        .with_mosi(peripherals.GPIO10)
        .with_miso(peripherals.GPIO21);

    // SPI device and portable RF driver
    let device = ExclusiveDevice::new_no_delay(spi, csn).unwrap();
    let mut radio = Nrf24::new(device);

    info!("Train ESP32-C3 initialized");

    configure_radio(&mut radio);

    let (buffer, written) = protocol_self_test();

    transmit(&mut radio, &mut ce, &buffer[..written]);

    info!("Transmission completed");

    loop {
        delay(Duration::from_millis(500));
    }
}
