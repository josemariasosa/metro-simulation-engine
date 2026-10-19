//! Minimal `no_std` driver for the nRF24L01+ transceiver.
//!
//! Built on `embedded-hal` SPI, GPIO, and delay traits.
//!
//! Currently supports PTX (transmit) mode only, with:
//! - 1 Mbps data rate and minimum RF output power.
//! - Auto ACK disabled.
//! - Configurable RF channel and 5-byte TX address.
//! - Verified initialization and fail-stop TX state management.
//!
//! RX support is not implemented yet.

#![no_std]

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use embedded_hal::spi::{Operation, SpiDevice};

const R_REGISTER: u8 = 0x00;
const REGISTER_MASK: u8 = 0x1F;

const W_REGISTER: u8 = 0x20;
const W_TX_PAYLOAD: u8 = 0xA0;

const FLUSH_TX: u8 = 0xE1;

const CONFIG: u8 = 0x00;
const EN_AA: u8 = 0x01;
const RF_CH: u8 = 0x05;
const RF_SETUP: u8 = 0x06;
const TX_ADDR: u8 = 0x10;

const PWR_UP: u8 = 1 << 1;
const PRIM_RX: u8 = 1 << 0;

// **********
// * Errors *
// **********

#[derive(Debug, PartialEq, Eq)]
pub enum PayloadError<E> {
    InvalidLength,
    Spi(E),
}

#[derive(Debug, PartialEq, Eq)]
pub enum RegisterError<E> {
    InvalidLength,
    Spi(E),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    InvalidChannel,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigureError<E> {
    Spi(E),
    Register(RegisterError<E>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum TxError<SpiError, PinError> {
    NotReady,
    Spi(SpiError),
    Pin(PinError),
    Payload(PayloadError<SpiError>),
    Timeout,
    MaxRetries,
    FifoNotEmpty,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InitError<SpiError, PinError> {
    NotReady,
    Configure(ConfigureError<SpiError>),
    Pin(PinError),
    VerificationFailed,
}

// *****************
// * Configuration *
// *****************

/// Configuration for primary transmitter (PTX) mode.
///
/// Uses a fixed 1 Mbps data rate, minimum RF power,
/// and disables automatic acknowledgments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxConfig {
    channel: u8,
    address: [u8; 5],
}

impl TxConfig {
    /// Creates a TX configuration.
    ///
    /// `channel` must be in the range 0..=125.
    /// `address` is the 5-byte RF destination address.
    ///
    /// Returns `ConfigError::InvalidChannel` for an invalid channel.
    pub fn new(channel: u8, address: [u8; 5]) -> Result<Self, ConfigError> {
        if channel > 125 {
            return Err(ConfigError::InvalidChannel);
        }

        Ok(Self { channel, address })
    }
}

/// Operational state of the radio driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioState {
    /// Hardware has not completed verified initialization.
    Unknown,

    /// TX initialization succeeded and transmissions are permitted.
    Ready,

    /// A critical error occurred; further transmissions are blocked.
    Faulted,
}

// **********
// * Driver *
// **********

/// nRF24L01+ driver owning the SPI device and CE output pin.
///
/// Starts in `RadioState::Unknown`. Call `initialize_tx()`
/// before attempting to transmit.
///
/// SPI chip-select (CSN) is managed by the supplied `SpiDevice`.
pub struct Nrf24<SPI, CE> {
    spi: SPI,
    ce: CE,
    state: RadioState,
}

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
    /// Creates a driver without accessing the hardware.
    ///
    /// The initial state is `RadioState::Unknown`.
    pub fn new(spi: SPI, ce: CE) -> Self {
        Self {
            spi,
            ce,
            state: RadioState::Unknown,
        }
    }

    /// Initializes and verifies the radio in PTX mode.
    ///
    /// Sets CE LOW, configures the radio, waits 2 ms for
    /// stabilization, and verifies the critical TX registers.
    ///
    /// Transitions to `Ready` on success or `Faulted` on failure.
    /// A faulted driver cannot be reinitialized implicitly.
    pub fn initialize_tx<D: DelayNs>(
        &mut self,
        config: &TxConfig,
        delay: &mut D,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        // A faulted radio cannot be reinitialized implicitly.
        if self.state == RadioState::Faulted {
            return Err(InitError::NotReady);
        }

        let result = self.initialize_tx_inner(config, delay);

        self.state = if result.is_ok() {
            RadioState::Ready
        } else {
            RadioState::Faulted
        };

        result
    }

    /// Transmits a single payload in PTX mode.
    ///
    /// Requires `RadioState::Ready` and a payload of 1..=32 bytes.
    ///
    /// Flushes the TX FIFO, clears interrupt flags, loads the payload,
    /// pulses CE, and polls STATUS until completion or timeout.
    ///
    /// On success, verifies that the TX FIFO is empty and clears TX_DS.
    ///
    /// Critical failures transition the driver to `Faulted`.
    /// Invalid payload length preserves the current state.
    ///
    /// TX completion does not guarantee reception by another device,
    /// since automatic acknowledgments are disabled.
    ///
    /// The timeout is based on 100 STATUS polls with 100 us delays,
    /// not an exact wall-clock deadline.
    pub fn transmit<D: DelayNs>(
        &mut self,
        payload: &[u8],
        delay: &mut D,
    ) -> Result<(), TxError<SPI::Error, CE::Error>> {
        if self.state != RadioState::Ready {
            return Err(TxError::NotReady);
        }

        // Validate before any hardware interaction.
        if payload.is_empty() || payload.len() > 32 {
            return Err(TxError::Payload(PayloadError::InvalidLength));
        }

        let result = self.transmit_inner(payload, delay);

        match &result {
            Ok(()) => {}

            // Invalid input: hardware was not accessed.
            Err(TxError::Payload(PayloadError::InvalidLength)) => {}

            // All other transmission failures are treated as critical.
            Err(_) => {
                self.state = RadioState::Faulted;
            }
        }

        result
    }

    /// Consumes the driver and returns ownership of SPI and CE.
    ///
    /// Does not reset or power down the radio.
    pub fn release(self) -> (SPI, CE) {
        (self.spi, self.ce)
    }

    /// Returns the current operational state of the driver.
    pub fn state(&self) -> RadioState {
        self.state
    }

    /// Generates a 20 us CE pulse.
    ///
    /// If setting CE LOW fails, retries once as a best-effort
    /// recovery and returns the original error.
    ///
    /// Does not guarantee the physical pin level after a GPIO error.
    fn pulse_ce<D: DelayNs>(&mut self, delay: &mut D) -> Result<(), CE::Error> {
        self.ce.set_high()?;

        delay.delay_us(20);

        if let Err(error) = self.ce.set_low() {
            // Best-effort recovery: try to return CE to LOW.
            let _ = self.ce.set_low();

            return Err(error);
        }

        Ok(())
    }

    /// Reads a single-byte register via SPI.
    fn read_register(&mut self, register: u8) -> Result<u8, SPI::Error> {
        let command = R_REGISTER | (register & REGISTER_MASK);
        let mut buffer = [command, 0xFF];

        self.spi.transfer_in_place(&mut buffer)?;

        Ok(buffer[1])
    }

    /// Writes a single-byte register via SPI.
    fn write_register(&mut self, register: u8, value: u8) -> Result<(), SPI::Error> {
        let command = W_REGISTER | (register & REGISTER_MASK);
        let mut buffer = [command, value];

        self.spi.transfer_in_place(&mut buffer)?;

        Ok(())
    }

    /// Reads 1..=5 bytes from a register into `output`.
    ///
    /// Returns `RegisterError::InvalidLength` for invalid lengths.
    fn read_register_bytes(
        &mut self,
        register: u8,
        output: &mut [u8],
    ) -> Result<(), RegisterError<SPI::Error>> {
        if output.is_empty() || output.len() > 5 {
            return Err(RegisterError::InvalidLength);
        }

        let command = [R_REGISTER | (register & REGISTER_MASK)];

        self.spi
            .transaction(&mut [Operation::Write(&command), Operation::Read(output)])
            .map_err(RegisterError::Spi)?;

        Ok(())
    }

    /// Writes 1..=5 bytes to a register.
    ///
    /// Returns `RegisterError::InvalidLength` for invalid lengths.
    fn write_register_bytes(
        &mut self,
        register: u8,
        values: &[u8],
    ) -> Result<(), RegisterError<SPI::Error>> {
        if values.is_empty() || values.len() > 5 {
            return Err(RegisterError::InvalidLength);
        }

        let mut buffer = [0u8; 6];

        buffer[0] = W_REGISTER | (register & REGISTER_MASK);
        buffer[1..1 + values.len()].copy_from_slice(values);

        self.spi
            .write(&buffer[..1 + values.len()])
            .map_err(RegisterError::Spi)?;

        Ok(())
    }

    /// Loads a payload of 1..=32 bytes into the TX FIFO.
    ///
    /// Does not trigger transmission.
    fn write_payload(&mut self, payload: &[u8]) -> Result<(), PayloadError<SPI::Error>> {
        if payload.is_empty() || payload.len() > 32 {
            return Err(PayloadError::InvalidLength);
        }

        let mut buffer = [0u8; 33];

        buffer[0] = W_TX_PAYLOAD;
        buffer[1..1 + payload.len()].copy_from_slice(payload);

        self.spi
            .write(&buffer[..1 + payload.len()])
            .map_err(PayloadError::Spi)?;

        Ok(())
    }

    /// Discards all pending payloads from the TX FIFO.
    fn flush_tx(&mut self) -> Result<(), SPI::Error> {
        self.spi.write(&[FLUSH_TX])
    }

    fn configure_tx(&mut self, config: &TxConfig) -> Result<(), ConfigureError<SPI::Error>> {
        // Disable Auto ACK.
        self.write_register(EN_AA, 0x00)
            .map_err(ConfigureError::Spi)?;

        // Configure RF channel.
        self.write_register(RF_CH, config.channel)
            .map_err(ConfigureError::Spi)?;

        // 1 Mbps, minimum output power.
        self.write_register(RF_SETUP, 0x00)
            .map_err(ConfigureError::Spi)?;

        // Configure destination address.
        self.write_register_bytes(TX_ADDR, &config.address)
            .map_err(ConfigureError::Register)?;

        // Enable PTX mode.
        let original = self.read_register(CONFIG).map_err(ConfigureError::Spi)?;

        let tx_config = (original | PWR_UP) & !PRIM_RX;

        self.write_register(CONFIG, tx_config)
            .map_err(ConfigureError::Spi)?;

        Ok(())
    }

    fn initialize_tx_inner<D: DelayNs>(
        &mut self,
        config: &TxConfig,
        delay: &mut D,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        // 1. Keep the radio inactive during configuration.
        self.ce.set_low().map_err(InitError::Pin)?;

        // 2. Configure RF registers and power up in PTX mode.
        self.configure_tx(config).map_err(InitError::Configure)?;

        // 3. Wait for the radio to settle.
        delay.delay_ms(2);

        // 4. Verify the essential configuration.
        self.verify_tx_config(config)?;

        Ok(())
    }

    fn verify_tx_config(
        &mut self,
        config: &TxConfig,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        const CONFIG: u8 = 0x00;
        const EN_AA: u8 = 0x01;
        const RF_CH: u8 = 0x05;
        const RF_SETUP: u8 = 0x06;
        const TX_ADDR: u8 = 0x10;

        const PWR_UP: u8 = 1 << 1;
        const PRIM_RX: u8 = 1 << 0;

        let read = |radio: &mut Self, register| {
            radio
                .read_register(register)
                .map_err(|error| InitError::Configure(ConfigureError::Spi(error)))
        };

        let config_reg = read(self, CONFIG)?;
        let en_aa = read(self, EN_AA)?;
        let channel = read(self, RF_CH)?;
        let rf_setup = read(self, RF_SETUP)?;

        let mut address = [0u8; 5];

        self.read_register_bytes(TX_ADDR, &mut address)
            .map_err(|error| InitError::Configure(ConfigureError::Register(error)))?;

        if config_reg & (PWR_UP | PRIM_RX) != PWR_UP
            || en_aa != 0
            || channel != config.channel
            || rf_setup != 0
            || address != config.address
        {
            return Err(InitError::VerificationFailed);
        }

        Ok(())
    }

    fn transmit_inner<D: DelayNs>(
        &mut self,
        payload: &[u8],
        delay: &mut D,
    ) -> Result<(), TxError<SPI::Error, CE::Error>> {
        const STATUS: u8 = 0x07;
        const FIFO_STATUS: u8 = 0x17;

        const RX_DR: u8 = 1 << 6;
        const TX_DS: u8 = 1 << 5;
        const MAX_RT: u8 = 1 << 4;
        const TX_EMPTY: u8 = 1 << 4;

        // 1. Start with a clean TX FIFO.
        self.flush_tx().map_err(TxError::Spi)?;

        // 2. Clear previous interrupt flags.
        self.write_register(STATUS, RX_DR | TX_DS | MAX_RT)
            .map_err(TxError::Spi)?;

        // 3. Load exactly one payload.
        self.write_payload(payload).map_err(TxError::Payload)?;

        // 4. Trigger RF transmission.
        self.pulse_ce(delay).map_err(TxError::Pin)?;

        // 5. Poll STATUS with a bounded timeout.
        for _ in 0..100 {
            let status = self.read_register(STATUS).map_err(TxError::Spi)?;

            if status & MAX_RT != 0 {
                return Err(TxError::MaxRetries);
            }

            if status & TX_DS != 0 {
                // 6. Confirm that the payload left the FIFO.
                let fifo = self.read_register(FIFO_STATUS).map_err(TxError::Spi)?;

                if fifo & TX_EMPTY == 0 {
                    return Err(TxError::FifoNotEmpty);
                }

                // 7. Clear TX completion flag.
                self.write_register(STATUS, TX_DS).map_err(TxError::Spi)?;

                return Ok(());
            }

            delay.delay_us(100);
        }

        Err(TxError::Timeout)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::io::ErrorKind;
    use std::vec;

    use super::*;

    use embedded_hal_mock::eh1::MockError;
    use embedded_hal_mock::eh1::delay::NoopDelay;
    use embedded_hal_mock::eh1::digital::{Mock as PinMock, State, Transaction as PinTransaction};
    use embedded_hal_mock::eh1::spi::{Mock as SpiMock, Transaction};

    #[test]
    fn read_rf_channel_register() {
        let expectations = [
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x05, 0xFF], vec![0x0E, 0x02]),
            Transaction::transaction_end(),
        ];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);
        let mut radio = Nrf24::new(spi, ce);

        let channel = radio.read_register(0x05).unwrap();

        assert_eq!(channel, 0x02);

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn write_rf_channel_register() {
        let expectations = [
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x25, 0x32], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
        ];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);
        let mut radio = Nrf24::new(spi, ce);

        radio.write_register(0x05, 0x32).unwrap();

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn write_tx_payload() {
        let payload = [
            0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];

        let expected = [
            0xA0, 0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04,
            0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let expectations = [
            Transaction::transaction_start(),
            Transaction::write_vec(expected.to_vec()),
            Transaction::transaction_end(),
        ];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);
        let mut radio = Nrf24::new(spi, ce);

        radio.write_payload(&payload).unwrap();

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn reject_invalid_payload_lengths() {
        for size in [0, 33] {
            // No SPI transactions should occur.
            let spi = SpiMock::new(&[]);
            let ce = PinMock::new(&[]);
            let mut radio = Nrf24::new(spi, ce);

            let payload = [0u8; 33];

            let result = radio.write_payload(&payload[..size]);

            assert!(matches!(result, Err(PayloadError::InvalidLength)));

            let (mut spi, mut ce) = radio.release();
            spi.done();
            ce.done();
        }
    }

    #[test]
    fn flush_tx_fifo() {
        let expectations = [
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0xE1]),
            Transaction::transaction_end(),
        ];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);
        let mut radio = Nrf24::new(spi, ce);

        radio.flush_tx().unwrap();

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn write_tx_address() {
        let expectations = [
            Transaction::transaction_start(),
            Transaction::write_vec(vec![
                0x30, // W_REGISTER | TX_ADDR (0x10)
                0xE7, 0xE7, 0xE7, 0xE7, 0xE7,
            ]),
            Transaction::transaction_end(),
        ];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);
        let mut radio = Nrf24::new(spi, ce);

        radio.write_register_bytes(0x10, &[0xE7; 5]).unwrap();

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn read_tx_address() {
        let expectations = [
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0x10]),
            Transaction::read_vec(vec![0xE7; 5]),
            Transaction::transaction_end(),
        ];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);
        let mut radio = Nrf24::new(spi, ce);

        let mut address = [0u8; 5];

        radio.read_register_bytes(0x10, &mut address).unwrap();

        assert_eq!(address, [0xE7; 5]);

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn accept_valid_tx_config() {
        let config = TxConfig::new(50, [0xE7; 5]);

        assert!(config.is_ok());
    }

    #[test]
    fn reject_invalid_tx_channel() {
        let config = TxConfig::new(126, [0xE7; 5]);

        assert_eq!(config, Err(ConfigError::InvalidChannel));
    }

    #[test]
    fn configure_tx_writes_expected_registers() {
        let expectations = [
            // Disable Auto ACK.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x21, 0x00], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // Set RF channel to 50.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x25, 0x32], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 1 Mbps, minimum output power.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x26, 0x00], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // Set TX address.
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0x30, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7]),
            Transaction::transaction_end(),
            // Read CONFIG (original = 0x08).
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x00, 0xFF], vec![0x0E, 0x08]),
            Transaction::transaction_end(),
            // Write CONFIG = 0x0A.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x20, 0x0A], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
        ];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);
        let mut radio = Nrf24::new(spi, ce);

        let config = TxConfig::new(50, [0xE7; 5]).unwrap();

        radio.configure_tx(&config).unwrap();

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn pulse_ce_high_then_low() {
        let spi = SpiMock::new(&[]);

        let ce_expectations = [
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low),
        ];

        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Ready;

        let mut delay = NoopDelay::new();

        radio.pulse_ce(&mut delay).unwrap();

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn transmit_success() {
        let expectations = [
            // FLUSH_TX
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0xE1]),
            Transaction::transaction_end(),
            // Clear STATUS flags.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x27, 0x70], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // Write three-byte payload.
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0xA0, 0x01, 0x02, 0x03]),
            Transaction::transaction_end(),
            // STATUS: TX_DS = 1
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x07, 0xFF], vec![0x2E, 0x2E]),
            Transaction::transaction_end(),
            // FIFO_STATUS: TX_EMPTY = 1
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x17, 0xFF], vec![0x2E, 0x11]),
            Transaction::transaction_end(),
            // Clear TX_DS.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x27, 0x20], vec![0x2E, 0x00]),
            Transaction::transaction_end(),
        ];

        let ce_expectations = [
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low),
        ];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Ready;

        let mut delay = NoopDelay::new();

        radio.transmit(&[0x01, 0x02, 0x03], &mut delay).unwrap();

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn transmit_max_retries() {
        let expectations = [
            // FLUSH_TX
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0xE1]),
            Transaction::transaction_end(),
            // Clear STATUS flags.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x27, 0x70], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // Load payload.
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0xA0, 0x01, 0x02, 0x03]),
            Transaction::transaction_end(),
            // STATUS: MAX_RT = 1.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x07, 0xFF], vec![0x1E, 0x1E]),
            Transaction::transaction_end(),
        ];

        let ce_expectations = [
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low),
        ];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Ready;

        let mut delay = NoopDelay::new();

        let result = radio.transmit(&[0x01, 0x02, 0x03], &mut delay);

        assert_eq!(result, Err(TxError::MaxRetries));

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn transmit_timeout() {
        let mut expectations = vec![
            // FLUSH_TX
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0xE1]),
            Transaction::transaction_end(),
            // Clear interrupt flags
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x27, 0x70], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // Load payload
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0xA0, 0x01]),
            Transaction::transaction_end(),
        ];

        // Radio never reports TX_DS or MAX_RT.
        for _ in 0..100 {
            expectations.extend([
                Transaction::transaction_start(),
                Transaction::transfer_in_place(vec![0x07, 0xFF], vec![0x0E, 0x0E]),
                Transaction::transaction_end(),
            ]);
        }

        let ce_expectations = [
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low),
        ];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Ready;

        let mut delay = NoopDelay::new();

        let result = radio.transmit(&[0x01], &mut delay);

        assert_eq!(result, Err(TxError::Timeout));
        assert_eq!(radio.state(), RadioState::Faulted);

        // A second transmission must be rejected immediately.
        let second = radio.transmit(&[0x01], &mut delay);

        assert_eq!(second, Err(TxError::NotReady));
        assert_eq!(radio.state(), RadioState::Faulted);

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn pulse_ce_fails_when_set_high_fails() {
        let spi = SpiMock::new(&[]);

        let error = MockError::Io(ErrorKind::Other);

        let ce_expectations = [PinTransaction::set(State::High).with_error(error.clone())];

        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Ready;

        let mut delay = NoopDelay::new();

        let result = radio.pulse_ce(&mut delay);

        assert_eq!(result, Err(error));

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn pulse_ce_fails_when_set_low_fails() {
        let spi = SpiMock::new(&[]);

        let error = MockError::Io(ErrorKind::Other);

        let ce_expectations = [
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low).with_error(error.clone()),
            PinTransaction::set(State::Low),
        ];

        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Ready;

        let mut delay = NoopDelay::new();

        let result = radio.pulse_ce(&mut delay);

        assert_eq!(result, Err(error));

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn pulse_ce_retries_set_low_after_failure() {
        let spi = SpiMock::new(&[]);
        let error = MockError::Io(ErrorKind::Other);

        let ce_expectations = [
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low).with_error(error.clone()),
            PinTransaction::set(State::Low),
        ];

        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);

        // Test fixture: initialization is tested separately.
        radio.state = RadioState::Ready;

        let mut delay = NoopDelay::new();

        let result = radio.pulse_ce(&mut delay);

        assert_eq!(result, Err(error));

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn new_radio_starts_in_unknown_state() {
        let spi = SpiMock::new(&[]);
        let ce = PinMock::new(&[]);

        let radio = Nrf24::new(spi, ce);

        assert_eq!(radio.state(), RadioState::Unknown);

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn initialize_tx_transitions_from_unknown_to_ready() {
        let expectations = [
            // 1. Disable Auto ACK.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x21, 0x00], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 2. RF_CH = 50.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x25, 0x32], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 3. RF_SETUP = 0x00.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x26, 0x00], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 4. TX_ADDR = [0xE7; 5].
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0x30, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7]),
            Transaction::transaction_end(),
            // 5. Read original CONFIG = 0x08.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x00, 0xFF], vec![0x0E, 0x08]),
            Transaction::transaction_end(),
            // 6. Power up in PTX mode: CONFIG = 0x0A.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x20, 0x0A], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 7. Verify CONFIG.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x00, 0xFF], vec![0x0E, 0x0A]),
            Transaction::transaction_end(),
            // 8. Verify EN_AA.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x01, 0xFF], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 9. Verify RF_CH.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x05, 0xFF], vec![0x0E, 0x32]),
            Transaction::transaction_end(),
            // 10. Verify RF_SETUP.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x06, 0xFF], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 11. Verify TX_ADDR.
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0x10]),
            Transaction::read_vec(vec![0xE7; 5]),
            Transaction::transaction_end(),
        ];

        let ce_expectations = [PinTransaction::set(State::Low)];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);
        let mut delay = NoopDelay::new();

        let config = TxConfig::new(50, [0xE7; 5]).unwrap();

        assert_eq!(radio.state(), RadioState::Unknown);

        radio.initialize_tx(&config, &mut delay).unwrap();

        assert_eq!(radio.state(), RadioState::Ready);

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn initialize_tx_verification_failure_sets_faulted() {
        let expectations = [
            // 1. Disable Auto ACK.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x21, 0x00], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 2. RF_CH = 50.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x25, 0x32], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 3. RF_SETUP = 0x00.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x26, 0x00], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 4. TX_ADDR = [0xE7; 5].
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0x30, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7]),
            Transaction::transaction_end(),
            // 5. Read original CONFIG = 0x08.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x00, 0xFF], vec![0x0E, 0x08]),
            Transaction::transaction_end(),
            // 6. Power up in PTX mode: CONFIG = 0x0A.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x20, 0x0A], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 7. Verify CONFIG.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x00, 0xFF], vec![0x0E, 0x0A]),
            Transaction::transaction_end(),
            // 8. Verify EN_AA.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x01, 0xFF], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 9. Expected channel 50, but hardware reports channel 42.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x05, 0xFF], vec![0x0E, 0x2A]),
            Transaction::transaction_end(),
            // 10. Verify RF_SETUP.
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x06, 0xFF], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
            // 11. Verify TX_ADDR.
            Transaction::transaction_start(),
            Transaction::write_vec(vec![0x10]),
            Transaction::read_vec(vec![0xE7; 5]),
            Transaction::transaction_end(),
        ];

        let ce_expectations = [PinTransaction::set(State::Low)];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);
        let mut delay = NoopDelay::new();

        let config = TxConfig::new(50, [0xE7; 5]).unwrap();

        assert_eq!(radio.state(), RadioState::Unknown);

        let result = radio.initialize_tx(&config, &mut delay);

        assert_eq!(result, Err(InitError::VerificationFailed));

        assert_eq!(radio.state(), RadioState::Faulted);

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn transmit_rejects_unknown_radio() {
        let spi = SpiMock::new(&[]);
        let ce = PinMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);
        let mut delay = NoopDelay::new();

        assert_eq!(radio.state(), RadioState::Unknown);

        let result = radio.transmit(&[0x01], &mut delay);

        assert_eq!(result, Err(TxError::NotReady));
        assert_eq!(radio.state(), RadioState::Unknown);

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn transmit_rejects_invalid_payload_without_hardware_access() {
        let spi = SpiMock::new(&[]);
        let ce = PinMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);
        radio.state = RadioState::Ready;

        let mut delay = NoopDelay::new();

        let empty = radio.transmit(&[], &mut delay);

        assert_eq!(empty, Err(TxError::Payload(PayloadError::InvalidLength)));

        let oversized = radio.transmit(&[0u8; 33], &mut delay);

        assert_eq!(
            oversized,
            Err(TxError::Payload(PayloadError::InvalidLength))
        );

        assert_eq!(radio.state(), RadioState::Ready);

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }
}
