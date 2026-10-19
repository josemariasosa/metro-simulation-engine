#![no_std]

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

// *****************
// * Configuration *
// *****************

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxConfig {
    channel: u8,
    address: [u8; 5],
}

impl TxConfig {
    pub fn new(channel: u8, address: [u8; 5]) -> Result<Self, ConfigError> {
        if channel > 125 {
            return Err(ConfigError::InvalidChannel);
        }

        Ok(Self { channel, address })
    }
}

// **********
// * Driver *
// **********

pub struct Nrf24<SPI> {
    spi: SPI,
}

impl<SPI> Nrf24<SPI>
where
    SPI: SpiDevice,
{
    pub fn new(spi: SPI) -> Self {
        Self { spi }
    }

    pub fn configure_tx(&mut self, config: &TxConfig) -> Result<(), ConfigureError<SPI::Error>> {
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

    pub fn read_register(&mut self, register: u8) -> Result<u8, SPI::Error> {
        let command = R_REGISTER | (register & REGISTER_MASK);
        let mut buffer = [command, 0xFF];

        self.spi.transfer_in_place(&mut buffer)?;

        Ok(buffer[1])
    }

    pub fn write_register(&mut self, register: u8, value: u8) -> Result<(), SPI::Error> {
        let command = W_REGISTER | (register & REGISTER_MASK);
        let mut buffer = [command, value];

        self.spi.transfer_in_place(&mut buffer)?;

        Ok(())
    }

    pub fn read_register_bytes(
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

    pub fn write_register_bytes(
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

    pub fn write_payload(&mut self, payload: &[u8]) -> Result<(), PayloadError<SPI::Error>> {
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

    pub fn flush_tx(&mut self) -> Result<(), SPI::Error> {
        self.spi.write(&[FLUSH_TX])
    }

    pub fn release(self) -> SPI {
        self.spi
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec;

    use super::*;

    use embedded_hal_mock::eh1::spi::{Mock as SpiMock, Transaction};

    #[test]
    fn read_rf_channel_register() {
        let expectations = [
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x05, 0xFF], vec![0x0E, 0x02]),
            Transaction::transaction_end(),
        ];

        let spi = SpiMock::new(&expectations);
        let mut radio = Nrf24::new(spi);

        let channel = radio.read_register(0x05).unwrap();

        assert_eq!(channel, 0x02);

        let mut spi = radio.release();
        spi.done();
    }

    #[test]
    fn write_rf_channel_register() {
        let expectations = [
            Transaction::transaction_start(),
            Transaction::transfer_in_place(vec![0x25, 0x32], vec![0x0E, 0x00]),
            Transaction::transaction_end(),
        ];

        let spi = SpiMock::new(&expectations);
        let mut radio = Nrf24::new(spi);

        radio.write_register(0x05, 0x32).unwrap();

        let mut spi = radio.release();
        spi.done();
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
        let mut radio = Nrf24::new(spi);

        radio.write_payload(&payload).unwrap();

        let mut spi = radio.release();
        spi.done();
    }

    #[test]
    fn reject_invalid_payload_lengths() {
        for size in [0, 33] {
            // No SPI transactions should occur.
            let spi = SpiMock::new(&[]);
            let mut radio = Nrf24::new(spi);

            let payload = [0u8; 33];

            let result = radio.write_payload(&payload[..size]);

            assert!(matches!(result, Err(PayloadError::InvalidLength)));

            let mut spi = radio.release();
            spi.done();
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
        let mut radio = Nrf24::new(spi);

        radio.flush_tx().unwrap();

        let mut spi = radio.release();
        spi.done();
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
        let mut radio = Nrf24::new(spi);

        radio.write_register_bytes(0x10, &[0xE7; 5]).unwrap();

        let mut spi = radio.release();
        spi.done();
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
        let mut radio = Nrf24::new(spi);

        let mut address = [0u8; 5];

        radio.read_register_bytes(0x10, &mut address).unwrap();

        assert_eq!(address, [0xE7; 5]);

        let mut spi = radio.release();
        spi.done();
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
        let mut radio = Nrf24::new(spi);

        let config = TxConfig::new(50, [0xE7; 5]).unwrap();

        radio.configure_tx(&config).unwrap();

        let mut spi = radio.release();
        spi.done();
    }
}
