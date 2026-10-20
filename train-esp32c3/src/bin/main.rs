#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embedded_hal::digital::OutputPin;
use embedded_hal::spi::SpiDevice;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::{
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::{Duration, Rate},
};
use le_rove_protocol::{
    codec::{decode, encode},
    frame::Frame,
    identity::{NodeId, TrainId},
    message::{Event, Location, Message, StopReason},
};
use log::info;
use nrf24_driver::{Nrf24, RadioConfig};

esp_bootloader_esp_idf::esp_app_desc!();

fn configure_radio<SPI, CE>(radio: &mut Nrf24<SPI, CE>, delay: &mut Delay)
where
    SPI: SpiDevice,
    CE: OutputPin,
    SPI::Error: core::fmt::Debug,
    CE::Error: core::fmt::Debug,
{
    let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

    radio.initialize_tx(&config, delay).unwrap();

    info!("Radio initialized: {:?}", radio.state());
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

    let ce = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());

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
    let mut radio = Nrf24::new(device, ce);
    let mut delay = Delay::new();

    info!("Train ESP32-C3 initialized");

    configure_radio(&mut radio, &mut delay);

    let (buffer, written) = protocol_self_test();

    radio.transmit(&buffer[..written], &mut delay).unwrap();

    info!("Transmission completed");

    loop {
        delay.delay(Duration::from_millis(500));
    }
}
