#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;

use embedded_hal::digital::OutputPin;
use embedded_hal::spi::MODE_0;
use embedded_hal_bus::spi::ExclusiveDevice;

use hal::clocks::Clock;
use hal::fugit::RateExtU32;
use hal::gpio::FunctionSpi;
use heapless::String;

use le_rove_protocol::codec::decode;
use nrf24_driver::{Nrf24, RadioConfig};
use rp235x_hal as hal;

use usb_device::{bus::UsbBusAllocator, prelude::*};
use usbd_serial::{SerialPort, USB_CLASS_CDC};

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;

#[hal::entry]
fn main() -> ! {
    let mut pac = hal::pac::Peripherals::take().unwrap();
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);

    let clocks = hal::clocks::init_clocks_and_plls(
        XOSC_CRYSTAL_FREQ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let mut timer = hal::Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);

    // ---- GPIO ----

    let sio = hal::Sio::new(pac.SIO);

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // ---- SPI0 ----

    let miso = pins.gpio16.into_function::<FunctionSpi>();
    let sck = pins.gpio18.into_function::<FunctionSpi>();
    let mosi = pins.gpio19.into_function::<FunctionSpi>();

    let spi = hal::spi::Spi::<_, _, _, 8>::new(pac.SPI0, (mosi, miso, sck)).init(
        &mut pac.RESETS,
        clocks.peripheral_clock.freq(),
        1.MHz(),
        MODE_0,
    );

    let mut csn = pins.gpio21.into_push_pull_output();
    csn.set_high().unwrap();

    let mut ce = pins.gpio20.into_push_pull_output();
    ce.set_low().unwrap();

    let device = ExclusiveDevice::new_no_delay(spi, csn).unwrap();
    let mut radio = Nrf24::new(device, ce);

    // ---- USB CDC ----

    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USB,
        pac.USB_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));

    let mut serial = SerialPort::new(&usb_bus);

    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x16c0, 0x27dd))
        .strings(&[StringDescriptors::default()
            .manufacturer("Le Rove")
            .product("Metro Station")
            .serial_number("00000001")])
        .unwrap()
        .device_class(USB_CLASS_CDC)
        .build();

    // ---- RADIO INITIALIZATION ----

    let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

    let init_result = radio.initialize(&config, &mut timer);
    let radio_ready = init_result.is_ok();

    // ---- OUTPUT BUFFER ----

    let mut output: String<512> = String::new();
    let mut tx_offset = 0usize;
    let mut tx_pending = true;

    match init_result {
        Ok(()) => {
            let _ = write!(
                output,
                "Le Rove Station - RP2350\r\n\
                 Radio initialized: RX ready\r\n\
                 Waiting for packets...\r\n"
            );
        }
        Err(error) => {
            let _ = write!(output, "Radio initialization failed: {:?}\r\n", error);
        }
    }

    let mut rx_buffer = [0u8; 32];

    // ---- MAIN LOOP ----

    loop {
        usb_dev.poll(&mut [&mut serial]);

        // Non-blocking USB output.
        if tx_pending {
            match serial.write(&output.as_bytes()[tx_offset..]) {
                Ok(n) if n > 0 => {
                    tx_offset += n;

                    if tx_offset >= output.len() {
                        tx_pending = false;
                        tx_offset = 0;
                        output.clear();
                    }
                }
                Ok(_) | Err(usb_device::UsbError::WouldBlock) => {}
                Err(_) => {
                    tx_pending = false;
                    tx_offset = 0;
                    output.clear();
                }
            }
        }

        // Do not overwrite pending USB output.
        if tx_pending || !radio_ready {
            continue;
        }

        // ---- RADIO RECEIVE ----

        match radio.receive(&mut rx_buffer) {
            Ok(Some(length)) => {
                let payload = &rx_buffer[..length];

                match decode(payload) {
                    Ok(message) => {
                        let _ = write!(output, "RX ({} bytes): {:?}\r\n", length, message);
                    }
                    Err(error) => {
                        let _ = write!(output, "Le Rove decode error: {:?}\r\n", error);
                    }
                }

                tx_offset = 0;
                tx_pending = !output.is_empty();
            }

            Ok(None) => {
                // No radio packet available.
            }

            Err(error) => {
                let _ = write!(output, "Radio RX error: {:?}\r\n", error);

                tx_offset = 0;
                tx_pending = !output.is_empty();

                // Stop polling the faulted radio.
                // Keep USB running for diagnostics.
                loop {
                    usb_dev.poll(&mut [&mut serial]);

                    if tx_pending {
                        match serial.write(&output.as_bytes()[tx_offset..]) {
                            Ok(n) if n > 0 => {
                                tx_offset += n;
                                if tx_offset >= output.len() {
                                    tx_pending = false;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
