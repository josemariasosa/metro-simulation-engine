#![no_std]

use embedded_hal::spi::SpiDevice;

const R_REGISTER: u8 = 0x00;
const REGISTER_MASK: u8 = 0x1F;

const W_REGISTER: u8 = 0x20;

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
}
