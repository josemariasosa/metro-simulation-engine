use super::*;

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
    /// Polls for one payload, returning its exact length or `None` if empty.
    ///
    /// Requires verified initialization in RX mode. A short buffer leaves the
    /// payload queued and can be retried. Zero or oversized widths flush the
    /// entire RX FIFO and return `InvalidPayloadWidth`; successful recovery
    /// preserves Ready/RX because no configuration or mode was changed.
    ///
    /// Any SPI failure faults the driver and invalidates the known mode. After
    /// an error the buffer may have been partially written; only use its data
    /// on `Ok(Some(length))`. No GPIO changes, allocation, or wait for an RF
    /// packet occurs; SPI transactions themselves are synchronous and may block.
    ///
    /// `Ok(None)` means the FIFO was empty at inspection; the buffer is untouched.
    /// `Ok(Some(length))` writes only `buffer[..length]`, leaving its tail intact.
    /// A 32-byte buffer accepts every supported payload; smaller buffers are
    /// allowed if the oldest packet fits. An undersized buffer is untouched.
    /// Reading the full payload under one CSN assertion removes that FIFO entry
    /// when CSN rises, before RX_DR is cleared. If the subsequent clear fails,
    /// the method returns an error although the packet was already consumed.
    /// A failed payload transaction may also have consumed data; it is not retryable.
    /// Invalid-width recovery discards every queued packet, including valid ones.
    /// Peers must use NO_ACK: recovery does not coordinate with hardware ACK TX.
    ///
    /// Availability always comes from FIFO_STATUS, not RX_DR. Clearing RX_DR
    /// acknowledges a notification, not a FIFO entry. A concurrently arriving
    /// payload remains discoverable on the next poll even if its flag is cleared.
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<Option<usize>, RxError<SPI::Error>> {
        if self.state != RadioState::Ready || self.mode != Some(OperatingMode::Rx) {
            return Err(RxError::NotReady);
        }

        let result = self.receive_inner(buffer);
        if matches!(result, Err(RxError::Spi(_))) {
            self.state = RadioState::Faulted;
            self.mode = None;
        }
        result
    }

    fn receive_inner(&mut self, buffer: &mut [u8]) -> Result<Option<usize>, RxError<SPI::Error>> {
        if !self.rx_available().map_err(RxError::Spi)? {
            // Retire any stale notification, without touching TX flags.
            self.write_register(STATUS, RX_DR).map_err(RxError::Spi)?;
            return Ok(None);
        }

        let width = self.read_payload_width().map_err(RxError::Spi)?;
        if !(1..=32).contains(&width) {
            // Datasheet table 20 mandates FLUSH_RX for widths >32. Treat zero
            // similarly: it is outside this driver's supported payload range.
            self.spi.write(&[FLUSH_RX]).map_err(RxError::Spi)?;
            self.write_register(STATUS, RX_DR).map_err(RxError::Spi)?;
            return Err(RxError::InvalidPayloadWidth(width));
        }
        let length = usize::from(width);
        if buffer.len() < length {
            return Err(RxError::BufferTooSmall {
                required: length,
                available: buffer.len(),
            });
        }

        self.read_payload(&mut buffer[..length])
            .map_err(RxError::Spi)?;
        // Write-one-to-clear: never read/modify/write STATUS (TX flags may be set).
        self.write_register(STATUS, RX_DR).map_err(RxError::Spi)?;
        Ok(Some(length))
    }

    fn read_payload_width(&mut self) -> Result<u8, SPI::Error> {
        let mut command = [R_RX_PL_WID, 0xFF];
        self.spi.transfer_in_place(&mut command)?;
        Ok(command[1])
    }

    // Called only with the validated, full payload length. One CSN assertion
    // covers command and data; CSN rising removes exactly this FIFO entry.
    fn read_payload(&mut self, buffer: &mut [u8]) -> Result<(), SPI::Error> {
        self.spi
            .transaction(&mut [Operation::Write(&[R_RX_PAYLOAD]), Operation::Read(buffer)])
    }

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

    use embedded_hal::spi::ErrorKind;
    use std::vec::Vec;

    // Match the TX tests' adapter: embedded-hal-mock 0.11 cannot inject SPI errors.
    struct FailingSpi {
        mock: SpiMock<u8>,
        fail_at: usize,
        calls: usize,
    }

    impl embedded_hal::spi::ErrorType for FailingSpi {
        type Error = ErrorKind;
    }

    impl SpiDevice for FailingSpi {
        fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), ErrorKind> {
            self.mock.transaction(operations)?;
            let fail = self.calls == self.fail_at;
            self.calls += 1;
            if fail { Err(ErrorKind::Other) } else { Ok(()) }
        }
    }

    fn transfer(command: u8, value: u8, response: u8) -> Vec<SpiTransaction<u8>> {
        vec![
            SpiTransaction::transaction_start(),
            // All IRQ bits may be set; only RX_DR may be acknowledged.
            SpiTransaction::transfer_in_place(vec![command, value], vec![0x70, response]),
            SpiTransaction::transaction_end(),
        ]
    }

    fn clear_rx() -> Vec<SpiTransaction<u8>> {
        transfer(0x27, 0x40, 0)
    }

    fn pending(width: u8) -> Vec<SpiTransaction<u8>> {
        let mut e = transfer(0x17, 0xFF, 0);
        e.extend(transfer(0x60, 0xFF, width));
        e
    }

    fn payload(bytes: &[u8]) -> Vec<SpiTransaction<u8>> {
        vec![
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0x61]),
            SpiTransaction::read_vec(bytes.to_vec()),
            SpiTransaction::transaction_end(),
        ]
    }

    fn reception(bytes: &[u8]) -> Vec<SpiTransaction<u8>> {
        let mut e = pending(bytes.len() as u8);
        e.extend(payload(bytes));
        e.extend(clear_rx());
        e
    }

    fn recovery(width: u8) -> Vec<SpiTransaction<u8>> {
        let mut e = pending(width);
        e.extend([
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xE2]),
            SpiTransaction::transaction_end(),
        ]);
        e.extend(clear_rx());
        e
    }

    fn ready(e: &[SpiTransaction<u8>]) -> Nrf24<SpiMock<u8>, PinMock> {
        let mut radio = Nrf24::new(SpiMock::new(e), PinMock::new(&[]));
        radio.state = RadioState::Ready;
        radio.mode = Some(OperatingMode::Rx);
        radio
    }

    fn done(radio: Nrf24<SpiMock<u8>, PinMock>) {
        assert_eq!(radio.state(), RadioState::Ready);
        assert_eq!(radio.mode, Some(OperatingMode::Rx));
        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn receive_empty_retires_stale_rx_flag() {
        let mut e = transfer(0x17, 0xFF, 1);
        e.extend(clear_rx());
        let mut radio = ready(&e);
        let mut buffer = [0xAA; 32];
        assert_eq!(radio.receive(&mut buffer), Ok(None));
        assert_eq!(buffer, [0xAA; 32]);
        done(radio);
    }

    #[test]
    fn receive_exact_bytes_and_length_without_touching_tail() {
        let bytes = [0, 0xFF, 0x81, 0x42];
        let mut radio = ready(&reception(&bytes));
        let mut buffer = [0xAA; 8];
        assert_eq!(radio.receive(&mut buffer), Ok(Some(4)));
        assert_eq!(&buffer[..4], &bytes);
        assert_eq!(&buffer[4..], &[0xAA; 4]);
        done(radio);
    }

    #[test]
    fn receive_two_queued_payloads_even_without_rx_dr() {
        let mut e = reception(&[0x12]);
        let mut second = reception(&[0x34, 0x56]);
        // RX_DR was cleared by the first receive; the FIFO still has data.
        second[1] = SpiTransaction::transfer_in_place(vec![0x17, 0xFF], vec![0x30, 0]);
        e.extend(second);
        e.extend(transfer(0x17, 0xFF, 1));
        e.extend(clear_rx());
        let mut radio = ready(&e);
        let mut buffer = [0; 32];
        assert_eq!(radio.receive(&mut buffer), Ok(Some(1)));
        assert_eq!(buffer[0], 0x12);
        assert_eq!(radio.receive(&mut buffer), Ok(Some(2)));
        assert_eq!(&buffer[..2], &[0x34, 0x56]);
        assert_eq!(radio.receive(&mut buffer), Ok(None));
        done(radio);
    }

    #[test]
    fn receive_maximum_payload() {
        let bytes = core::array::from_fn::<_, 32, _>(|i| i as u8);
        let mut radio = ready(&reception(&bytes));
        let mut buffer = [0; 32];
        assert_eq!(radio.receive(&mut buffer), Ok(Some(32)));
        assert_eq!(buffer, bytes);
        done(radio);
    }

    #[test]
    fn short_buffer_preserves_payload_for_retry() {
        for capacity in [0, 2] {
            let mut e = pending(3);
            e.extend(reception(&[1, 2, 3]));
            let mut radio = ready(&e);
            let mut buffer = [0xAA; 3];
            assert_eq!(
                radio.receive(&mut buffer[..capacity]),
                Err(RxError::BufferTooSmall {
                    required: 3,
                    available: capacity,
                })
            );
            assert_eq!(buffer, [0xAA; 3]);
            assert_eq!(radio.state(), RadioState::Ready);
            assert_eq!(radio.receive(&mut buffer), Ok(Some(3)));
            assert_eq!(buffer, [1, 2, 3]);
            done(radio);
        }
    }

    #[test]
    fn invalid_width_flushes_and_allows_subsequent_reception() {
        for width in [0, 33, 255] {
            let mut e = recovery(width);
            e.extend(reception(&[0x42]));
            let mut radio = ready(&e);
            let mut buffer = [0xAA; 32];
            assert_eq!(
                radio.receive(&mut buffer),
                Err(RxError::InvalidPayloadWidth(width))
            );
            assert_eq!(buffer, [0xAA; 32]);
            assert_eq!(radio.state(), RadioState::Ready);
            assert_eq!(radio.receive(&mut buffer), Ok(Some(1)));
            assert_eq!(buffer[0], 0x42);
            done(radio);
        }
    }

    fn spi_failure(e: Vec<SpiTransaction<u8>>, fail_at: usize) {
        let spi = FailingSpi {
            mock: SpiMock::new(&e),
            fail_at,
            calls: 0,
        };
        let mut radio = Nrf24::new(spi, PinMock::new(&[]));
        radio.state = RadioState::Ready;
        radio.mode = Some(OperatingMode::Rx);
        let mut buffer = [0; 32];
        assert_eq!(
            radio.receive(&mut buffer),
            Err(RxError::Spi(ErrorKind::Other))
        );
        assert_eq!(radio.state(), RadioState::Faulted);
        assert_eq!(radio.mode, None);
        assert_eq!(radio.receive(&mut buffer), Err(RxError::NotReady));
        let (mut spi, mut ce) = radio.release();
        spi.mock.done();
        ce.done();
    }

    #[test]
    fn fifo_status_spi_failure_faults() {
        spi_failure(transfer(0x17, 0xFF, 0), 0);
    }

    #[test]
    fn width_spi_failure_faults() {
        spi_failure(pending(1), 1);
    }

    #[test]
    fn payload_spi_failure_faults() {
        let mut e = pending(1);
        e.extend(payload(&[0x42]));
        spi_failure(e, 2);
    }

    #[test]
    fn rx_flag_clear_spi_failure_faults() {
        spi_failure(reception(&[0x42]), 3);
    }

    #[test]
    fn empty_flag_clear_spi_failure_faults() {
        let mut e = transfer(0x17, 0xFF, 1);
        e.extend(clear_rx());
        spi_failure(e, 1);
    }

    #[test]
    fn invalid_width_flush_failure_faults() {
        let mut e = recovery(33);
        e.truncate(e.len() - 3);
        spi_failure(e, 2);
    }

    #[test]
    fn invalid_width_flag_clear_failure_faults() {
        spi_failure(recovery(33), 3);
    }

    #[test]
    fn receive_rejects_uninitialized_faulted_and_tx_without_hardware_access() {
        for (state, mode) in [
            (RadioState::Unknown, None),
            (RadioState::Faulted, None),
            (RadioState::Ready, Some(OperatingMode::Tx)),
            (RadioState::Ready, None),
        ] {
            let mut radio = Nrf24::new(SpiMock::new(&[]), PinMock::new(&[]));
            radio.state = state;
            radio.mode = mode;
            assert_eq!(radio.receive(&mut [0; 32]), Err(RxError::NotReady));
            assert_eq!(radio.state(), state);
            assert_eq!(radio.mode, mode);
            let (mut spi, mut ce) = radio.release();
            spi.done();
            ce.done();
        }
    }

    #[test]
    fn pending_rx_payload_survives_transmit_and_is_received_in_rx_mode() {
        use embedded_hal_mock::eh1::{
            delay::{CheckedDelay, Transaction as DelayTransaction},
            digital::{State, Transaction as PinTransaction},
        };
        let mut e = transfer(0x17, 0xFF, 0); // RX payload queued before TX.
        e.extend(transfer(0x00, 0xFF, 0x0B));
        e.extend(transfer(0x20, 0x0A, 0));
        e.extend([
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xE1]), // Only FLUSH_TX is allowed.
            SpiTransaction::transaction_end(),
        ]);
        e.extend(transfer(0x27, 0x30, 0));
        e.extend([
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0xB0, 0x99]),
            SpiTransaction::transaction_end(),
        ]);
        e.extend(transfer(0x07, 0xFF, 0x60)); // TX_DS and RX_DR.
        e.extend(transfer(0x17, 0xFF, 0x10)); // TX empty, RX not empty.
        e.extend(transfer(0x27, 0x20, 0));
        e.extend(transfer(0x00, 0xFF, 0x0A));
        e.extend(transfer(0x20, 0x0B, 0));
        e.extend(reception(&[0x12, 0x34]));
        let ce = PinMock::new(&[
            PinTransaction::set(State::Low),
            PinTransaction::set(State::High),
            PinTransaction::set(State::Low),
            PinTransaction::set(State::Low),
            PinTransaction::set(State::High),
        ]);
        let mut radio = Nrf24::new(SpiMock::new(&e), ce);
        radio.state = RadioState::Ready;
        radio.mode = Some(OperatingMode::Rx);
        assert_eq!(radio.rx_available(), Ok(true));
        let mut delay = CheckedDelay::new(&[
            DelayTransaction::delay_us(20),
            DelayTransaction::delay_us(130),
        ]);
        assert_eq!(radio.transmit(&[0x99], &mut delay), Ok(()));
        let mut buffer = [0; 32];
        assert_eq!(radio.receive(&mut buffer), Ok(Some(2)));
        assert_eq!(&buffer[..2], &[0x12, 0x34]);
        delay.done();
        done(radio);
    }
}
