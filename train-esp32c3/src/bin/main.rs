#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_hal::{
    clock::CpuClock,
    main,
    time::{Duration, Instant},
};
use log::info;

use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::{spi::Mode, time::Rate};

use embedded_hal::spi::SpiBus;

use esp_backtrace as _;

use le_rove_protocol::{
    codec::{decode, encode},
    frame::Frame,
    identity::{NodeId, TrainId},
    message::{Event, Location, Message, StopReason},
};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

fn protocol_self_test() {
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

    info!("Encoded {} bytes", written);
    info!("Payload: {:02X?}", &buffer[..written]);

    let recovered = decode(&buffer[..written]).unwrap();

    assert_eq!(original, recovered);

    info!("SPEC-001 encode/decode: PASS");

    const EXPECTED: [u8; 18] = [
        0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04, 0x00, 0x00,
        0x00, 0x00, 0x00,
    ];

    assert_eq!(&buffer[..written], &EXPECTED);

    info!("SPEC-001 equal expected coded: PASS");
}

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {
    // generator version: 1.4.0
    // generator parameters: -o esp32c3 -o esp32c3-mini-1 -o log -o esp-backtrace -o stable-aarch64-apple-darwin

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    // let peripherals = esp_hal::init(config);

    // ---

    let peripherals = esp_hal::init(config);

    let mut csn = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());

    let mut ce = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());

    info!("RF control pins initialized");

    let spi_config = SpiConfig::default()
        .with_frequency(Rate::from_khz(500))
        .with_mode(Mode::_0);

    let mut spi = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO8)
        .with_mosi(peripherals.GPIO10)
        .with_miso(peripherals.GPIO21);

    info!("SPI initialized");

    let mut buffer = [0x00, 0xFF];

    // Comenzamos la transacción
    csn.set_low();

    // Escribimos y leemos simultáneamente
    spi.transfer_in_place(&mut buffer).unwrap();

    // Terminamos la transacción
    csn.set_high();

    info!("SPI STATUS: 0x{:02X}", buffer[0]);
    info!("RF CONFIG: 0x{:02X}", buffer[1]);

    // RF_CH register address
    const RF_CH: u8 = 0x05;

    // nRF24L01+ SPI commands
    const R_REGISTER: u8 = 0x00;
    const W_REGISTER: u8 = 0x20;

    // 1. Read original channel
    let mut read = [R_REGISTER | RF_CH, 0xFF];

    csn.set_low();
    spi.transfer_in_place(&mut read).unwrap();
    csn.set_high();

    let original_channel = read[1];
    info!("Original RF_CH: 0x{:02X}", original_channel);

    // 2. Write channel 50
    let mut write = [W_REGISTER | RF_CH, 0x32];

    csn.set_low();
    spi.transfer_in_place(&mut write).unwrap();
    csn.set_high();

    // 3. Read back
    let mut verify = [R_REGISTER | RF_CH, 0xFF];

    csn.set_low();
    spi.transfer_in_place(&mut verify).unwrap();
    csn.set_high();

    info!("Modified RF_CH: 0x{:02X}", verify[1]);

    // 4. Restore original channel
    let mut restore = [W_REGISTER | RF_CH, original_channel];

    csn.set_low();
    spi.transfer_in_place(&mut restore).unwrap();
    csn.set_high();

    // 5. Verify restoration
    let mut restored = [R_REGISTER | RF_CH, 0xFF];

    csn.set_low();
    spi.transfer_in_place(&mut restored).unwrap();
    csn.set_high();

    assert_eq!(verify[1], 0x32);
    assert_eq!(restored[1], original_channel);

    info!("RF_CH restored: 0x{:02X}", restored[1]);
    info!("nRF24L01+ SPI read/write: PASS");

    info!("Train ESP32-C3 initialized");

    protocol_self_test();
    // ---

    // // The following pins are used to bootstrap the chip. They are available
    // // for use, but check the datasheet of the module for more information on them.
    // // - GPIO2
    // // - GPIO8
    // // - GPIO9
    // // These GPIO pins are in use by some feature of the module and should not be used.
    // let _gpio11 = peripherals.GPIO11;
    // let _gpio12 = peripherals.GPIO12;
    // let _gpio13 = peripherals.GPIO13;
    // let _gpio14 = peripherals.GPIO14;
    // let _gpio15 = peripherals.GPIO15;
    // let _gpio16 = peripherals.GPIO16;
    // let _gpio17 = peripherals.GPIO17;

    loop {
        info!("Hello world!");
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(500) {}
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.2.2/examples
}
