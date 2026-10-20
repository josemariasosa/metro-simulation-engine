use super::*;

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
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
    use embedded_hal_mock::eh1::spi::{Mock as SpiMock, Transaction as SpiTransaction};

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
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xE1]),
            SpiTransaction::transaction_end(),
            // Clear STATUS flags.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x27, 0x70], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // Write three-byte payload.
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xA0, 0x01, 0x02, 0x03]),
            SpiTransaction::transaction_end(),
            // STATUS: TX_DS = 1
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x07, 0xFF], vec![0x2E, 0x2E]),
            SpiTransaction::transaction_end(),
            // FIFO_STATUS: TX_EMPTY = 1
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x17, 0xFF], vec![0x2E, 0x11]),
            SpiTransaction::transaction_end(),
            // Clear TX_DS.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x27, 0x20], vec![0x2E, 0x00]),
            SpiTransaction::transaction_end(),
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
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xE1]),
            SpiTransaction::transaction_end(),
            // Clear STATUS flags.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x27, 0x70], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // Load payload.
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xA0, 0x01, 0x02, 0x03]),
            SpiTransaction::transaction_end(),
            // STATUS: MAX_RT = 1.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x07, 0xFF], vec![0x1E, 0x1E]),
            SpiTransaction::transaction_end(),
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
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xE1]),
            SpiTransaction::transaction_end(),
            // Clear interrupt flags
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x27, 0x70], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // Load payload
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xA0, 0x01]),
            SpiTransaction::transaction_end(),
        ];

        // Radio never reports TX_DS or MAX_RT.
        for _ in 0..100 {
            expectations.extend([
                SpiTransaction::transaction_start(),
                SpiTransaction::transfer_in_place(vec![0x07, 0xFF], vec![0x0E, 0x0E]),
                SpiTransaction::transaction_end(),
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
