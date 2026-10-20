use super::*;

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
    /// Reads a single-byte register via SPI.
    pub(super) fn read_register(&mut self, register: u8) -> Result<u8, SPI::Error> {
        let command = R_REGISTER | (register & REGISTER_MASK);
        let mut buffer = [command, 0xFF];

        self.spi.transfer_in_place(&mut buffer)?;

        Ok(buffer[1])
    }

    /// Writes a single-byte register via SPI.
    pub(super) fn write_register(&mut self, register: u8, value: u8) -> Result<(), SPI::Error> {
        let command = W_REGISTER | (register & REGISTER_MASK);
        let mut buffer = [command, value];

        self.spi.transfer_in_place(&mut buffer)?;

        Ok(())
    }

    /// Reads 1..=5 bytes from a register into `output`.
    ///
    /// Returns `RegisterError::InvalidLength` for invalid lengths.
    pub(super) fn read_register_bytes(
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
    pub(super) fn write_register_bytes(
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
    /// Uses selective NO_ACK; requires FEATURE.EN_DYN_ACK from initialize().
    /// Does not trigger transmission.
    pub(super) fn write_payload(&mut self, payload: &[u8]) -> Result<(), PayloadError<SPI::Error>> {
        if payload.is_empty() || payload.len() > 32 {
            return Err(PayloadError::InvalidLength);
        }

        let mut buffer = [0u8; 33];

        buffer[0] = W_TX_PAYLOAD_NOACK;
        buffer[1..1 + payload.len()].copy_from_slice(payload);

        self.spi
            .write(&buffer[..1 + payload.len()])
            .map_err(PayloadError::Spi)?;

        Ok(())
    }

    /// Discards all pending payloads from the TX FIFO.
    pub(super) fn flush_tx(&mut self) -> Result<(), SPI::Error> {
        self.spi.write(&[FLUSH_TX])
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec;

    use super::*;

    use embedded_hal_mock::eh1::digital::Mock as PinMock;
    use embedded_hal_mock::eh1::spi::{Mock as SpiMock, Transaction as SpiTransaction};

    #[test]
    fn read_rf_channel_register() {
        let expectations = [
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x05, 0xFF], vec![0x0E, 0x02]),
            SpiTransaction::transaction_end(),
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
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x25, 0x32], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
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
            0xB0, 0x01, 0x21, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04,
            0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let expectations = [
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(expected.to_vec()),
            SpiTransaction::transaction_end(),
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
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xE1]),
            SpiTransaction::transaction_end(),
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
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![
                0x30, // W_REGISTER | TX_ADDR (0x10)
                0xE7, 0xE7, 0xE7, 0xE7, 0xE7,
            ]),
            SpiTransaction::transaction_end(),
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
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0x10]),
            SpiTransaction::read_vec(vec![0xE7; 5]),
            SpiTransaction::transaction_end(),
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
}
