use super::*;

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
    /// Returns whether the RX FIFO contains at least one payload.
    fn rx_available(&mut self) -> Result<bool, SPI::Error> {
        let status = self.read_register(FIFO_STATUS)?;

        Ok(status & RX_EMPTY == 0)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec;

    use super::*;

    use embedded_hal_mock::eh1::{
        digital::Mock as PinMock,
        spi::{Mock as SpiMock, Transaction as SpiTransaction},
    };

    #[test]
    fn rx_available_returns_false_when_fifo_is_empty() {
        let spi = SpiMock::new(&[
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![FIFO_STATUS, 0xFF], vec![0x00, RX_EMPTY]),
            SpiTransaction::transaction_end(),
        ]);

        let ce = PinMock::new(&[]);
        let mut radio = Nrf24::new(spi, ce);

        assert_eq!(radio.rx_available(), Ok(false));

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn rx_available_returns_true_when_fifo_contains_payload() {
        let spi = SpiMock::new(&[
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![FIFO_STATUS, 0xFF], vec![0x00, 0x00]),
            SpiTransaction::transaction_end(),
        ]);

        let ce = PinMock::new(&[]);
        let mut radio = Nrf24::new(spi, ce);

        assert_eq!(radio.rx_available(), Ok(true));

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }
}
