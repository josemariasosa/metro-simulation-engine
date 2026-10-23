#![no_std]
#![no_main]

use core::panic::PanicInfo;

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_time::Timer;

use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::{Builder, Config, UsbDevice};

use static_cell::StaticCell;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

// USB interrupt handler
bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

type UsbDriver = Driver<'static, USB>;
type Usb = UsbDevice<'static, UsbDriver>;

// Embassy USB needs to run continuously.
#[embassy_executor::task]
async fn usb_task(mut usb: Usb) {
    usb.run().await;
}

#[embassy_executor::main(
    executor = "embassy_rp::executor::Executor",
    entry = "cortex_m_rt::entry"
)]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // ---- USB DRIVER ----

    let driver = Driver::new(p.USB, Irqs);

    let mut config = Config::new(0x16c0, 0x27dd);
    config.manufacturer = Some("Le Rove");
    config.product = Some("Metro Station - Embassy");
    config.serial_number = Some("00000002");
    config.max_packet_size_0 = 64;

    // ---- USB BUFFERS ----

    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
    static STATE: StaticCell<State> = StaticCell::new();

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        &mut [],
        CONTROL_BUF.init([0; 64]),
    );

    let mut serial = CdcAcmClass::new(&mut builder, STATE.init(State::new()), 64);

    let usb = builder.build();

    // ---- START USB TASK ----

    spawner.spawn(usb_task(usb).unwrap());

    // ---- MAIN ASYNC LOOP ----

    loop {
        // Wait until macOS opens the serial port.
        serial.wait_connection().await;

        if serial
            .write_packet(b"Le Rove - Embassy USB ready\r\n")
            .await
            .is_err()
        {
            continue;
        }

        loop {
            Timer::after_millis(1000).await;

            if serial.write_packet(b"tick\r\n").await.is_err() {
                break;
            }
        }
    }
}
