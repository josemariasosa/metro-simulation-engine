use super::*;

impl<SPI, CE> Nrf24<SPI, CE>
where
    SPI: SpiDevice,
    CE: OutputPin,
{
    /// Initializes the transceiver in receive-first mode.
    ///
    /// On success, the radio enters RX mode and is ready to receive.
    ///
    /// Transmissions switch temporarily to TX and
    /// restore RX automatically.
    ///
    /// Any initialization failure transitions the driver to Faulted.
    pub fn initialize<D: DelayNs>(
        &mut self,
        config: &RadioConfig,
        delay: &mut D,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        if self.state != RadioState::Unknown {
            return Err(InitError::NotReady);
        }

        let result = self.initialize_rx_inner(config, delay);

        match result {
            Ok(()) => {
                self.state = RadioState::Ready;
                self.mode = Some(OperatingMode::Rx);
                Ok(())
            }
            Err(error) => {
                self.state = RadioState::Faulted;
                self.mode = None;
                Err(error)
            }
        }
    }

    /// Initializes and verifies the radio in PTX mode.
    ///
    /// Sets CE LOW, configures the radio, waits 2 ms for
    /// stabilization, and verifies the critical TX registers.
    ///
    /// Transitions to `Ready` on success or `Faulted` on failure.
    /// A faulted driver cannot be reinitialized implicitly.
    /// This legacy initializer leaves CE low in PTX standby; `transmit()`
    /// requires RX mode established by `initialize()` on a new driver.
    pub fn initialize_tx<D: DelayNs>(
        &mut self,
        config: &RadioConfig,
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

        self.mode = if result.is_ok() {
            Some(OperatingMode::Tx)
        } else {
            None
        };

        result
    }

    // ******************
    // * Switch to mode *
    // ******************

    fn switch_to_rx_inner<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        self.ce.set_low().map_err(InitError::Pin)?;

        let config = self
            .read_register(CONFIG)
            .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

        self.write_register(CONFIG, config | PRIM_RX)
            .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

        self.ce.set_high().map_err(InitError::Pin)?;

        delay.delay_us(RADIO_SETTLING_US);

        Ok(())
    }

    pub(super) fn switch_to_rx<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        if self.state != RadioState::Ready || self.mode != Some(OperatingMode::Tx) {
            return Err(InitError::NotReady);
        }

        let result = self.switch_to_rx_inner(delay);

        match result {
            Ok(()) => {
                self.mode = Some(OperatingMode::Rx);
                Ok(())
            }
            Err(error) => {
                self.state = RadioState::Faulted;
                self.mode = None;
                Err(error)
            }
        }
    }

    fn switch_to_tx_inner(&mut self) -> Result<(), InitError<SPI::Error, CE::Error>> {
        self.ce.set_low().map_err(InitError::Pin)?;

        let config = self
            .read_register(CONFIG)
            .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

        // CE is low: this selects PTX in standby-I. TX settling starts
        // with the CE pulse, not with this register write (datasheet §6.1.7).
        self.write_register(CONFIG, config & !PRIM_RX)
            .map_err(|e| InitError::Configure(ConfigureError::Spi(e)))?;

        Ok(())
    }

    pub(super) fn switch_to_tx(&mut self) -> Result<(), InitError<SPI::Error, CE::Error>> {
        if self.state != RadioState::Ready || self.mode != Some(OperatingMode::Rx) {
            return Err(InitError::NotReady);
        }

        let result = self.switch_to_tx_inner();

        match result {
            Ok(()) => {
                self.mode = Some(OperatingMode::Tx);
                Ok(())
            }
            Err(error) => {
                self.state = RadioState::Faulted;
                self.mode = None;
                Err(error)
            }
        }
    }

    fn initialize_rx_inner<D: DelayNs>(
        &mut self,
        config: &RadioConfig,
        delay: &mut D,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        // 1. Disable radio activity during configuration.
        self.ce.set_low().map_err(InitError::Pin)?;

        // 2. Configure the radio as PRX.
        self.configure_rx(config).map_err(InitError::Configure)?;

        // 3. Allow the radio to power up and stabilize.
        delay.delay_ms(POWER_UP_DELAY_MS);

        // 4. Verify hardware configuration.
        self.verify_rx_config(config)?;

        // 5. Begin listening.
        self.ce.set_high().map_err(InitError::Pin)?;

        // 6. Allow RX mode to settle.
        delay.delay_us(RADIO_SETTLING_US);

        Ok(())
    }

    fn configure_tx(&mut self, config: &RadioConfig) -> Result<(), ConfigureError<SPI::Error>> {
        // Disable Auto ACK.
        self.write_register(EN_AA, 0x00)
            .map_err(ConfigureError::Spi)?;

        // Configure RF channel.
        self.write_register(RF_CH, config.channel())
            .map_err(ConfigureError::Spi)?;

        // Configure data rate and TX output power.
        self.write_register(RF_SETUP, config.rf_setup())
            .map_err(ConfigureError::Spi)?;

        // Configure destination address.
        self.write_register_bytes(TX_ADDR, &config.address())
            .map_err(ConfigureError::Register)?;

        // Enable PTX mode.
        let original = self.read_register(CONFIG).map_err(ConfigureError::Spi)?;

        let tx_config = (original | PWR_UP) & !PRIM_RX;

        self.write_register(CONFIG, tx_config)
            .map_err(ConfigureError::Spi)?;

        Ok(())
    }

    fn configure_rx(&mut self, config: &RadioConfig) -> Result<(), ConfigureError<SPI::Error>> {
        // DPL requires ENAA_P0. Packets sent with W_TX_PAYLOAD_NOACK
        // suppress acknowledgments without disabling dynamic payload framing.
        self.write_register(EN_AA, ERX_P0)
            .map_err(ConfigureError::Spi)?;

        // 2. Enable reception on pipe 0.
        self.write_register(EN_RXADDR, ERX_P0)
            .map_err(ConfigureError::Spi)?;

        // 3. Configure the RF channel.
        self.write_register(RF_CH, config.channel())
            .map_err(ConfigureError::Spi)?;

        // 4. Configure data rate and TX power.
        self.write_register(RF_SETUP, config.rf_setup())
            .map_err(ConfigureError::Spi)?;

        // 5. Configure the address width to 5 bytes.
        self.write_register(SETUP_AW, ADDRESS_WIDTH_5_BYTES)
            .map_err(ConfigureError::Spi)?;

        // 6. Configure the receive address.
        self.write_register_bytes(RX_ADDR_P0, &config.address())
            .map_err(ConfigureError::Register)?;

        // Keep TX_ADDR equal to RX_ADDR_P0 so a later RX -> TX switch is ready.
        self.write_register_bytes(TX_ADDR, &config.address())
            .map_err(ConfigureError::Register)?;

        // 7. Enable Dynamic Payload Length on pipe 0.
        self.write_register(FEATURE, EN_DPL | EN_DYN_ACK)
            .map_err(ConfigureError::Spi)?;

        self.write_register(DYNPD, DPL_P0)
            .map_err(ConfigureError::Spi)?;

        // 8. Power up the radio in primary receiver mode.
        let original = self.read_register(CONFIG).map_err(ConfigureError::Spi)?;

        // Use the same explicit one-byte CRC on both peers, even after MCU-only reset.
        let rx_config = (original | EN_CRC | PWR_UP | PRIM_RX) & !CRCO;

        self.write_register(CONFIG, rx_config)
            .map_err(ConfigureError::Spi)?;

        Ok(())
    }

    fn initialize_tx_inner<D: DelayNs>(
        &mut self,
        config: &RadioConfig,
        delay: &mut D,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        // 1. Keep the radio inactive during configuration.
        self.ce.set_low().map_err(InitError::Pin)?;

        // 2. Configure RF registers and power up in PTX mode.
        self.configure_tx(config).map_err(InitError::Configure)?;

        // 3. Wait for the radio to settle.
        delay.delay_ms(POWER_UP_DELAY_MS);

        // 4. Verify the essential configuration.
        self.verify_tx_config(config)?;

        Ok(())
    }

    fn verify_tx_config(
        &mut self,
        config: &RadioConfig,
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
            || channel != config.channel()
            || rf_setup != config.rf_setup()
            || address != config.address()
        {
            return Err(InitError::VerificationFailed);
        }

        Ok(())
    }

    fn verify_rx_config(
        &mut self,
        config: &RadioConfig,
    ) -> Result<(), InitError<SPI::Error, CE::Error>> {
        let read = |radio: &mut Self, register| {
            radio
                .read_register(register)
                .map_err(|error| InitError::Configure(ConfigureError::Spi(error)))
        };

        let config_reg = read(self, CONFIG)?;
        let en_aa = read(self, EN_AA)?;
        let en_rxaddr = read(self, EN_RXADDR)?;
        let channel = read(self, RF_CH)?;
        let rf_setup = read(self, RF_SETUP)?;
        let address_width = read(self, SETUP_AW)?;
        let feature = read(self, FEATURE)?;
        let dynpd = read(self, DYNPD)?;

        let mut address = [0u8; 5];

        self.read_register_bytes(RX_ADDR_P0, &mut address)
            .map_err(|error| InitError::Configure(ConfigureError::Register(error)))?;

        let mut tx_address = [0u8; 5];

        self.read_register_bytes(TX_ADDR, &mut tx_address)
            .map_err(|error| InitError::Configure(ConfigureError::Register(error)))?;

        if config_reg & (EN_CRC | CRCO | PWR_UP | PRIM_RX) != (EN_CRC | PWR_UP | PRIM_RX)
            || en_aa != ERX_P0
            || en_rxaddr != ERX_P0
            || channel != config.channel()
            || rf_setup != config.rf_setup()
            || address_width != ADDRESS_WIDTH_5_BYTES
            || feature != (EN_DPL | EN_DYN_ACK)
            || dynpd & DPL_P0 == 0
            || address != config.address()
            || tx_address != config.address()
        {
            return Err(InitError::VerificationFailed);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::io::ErrorKind;
    use std::vec;
    use std::vec::Vec;

    use super::*;

    use crate::config::{DataRate, TxPower};

    use embedded_hal_mock::eh1::{
        MockError,
        delay::{CheckedDelay as DelayMock, NoopDelay, Transaction as DelayTransaction},
        digital::{Mock as PinMock, State as PinState, State, Transaction as PinTransaction},
        spi::{Mock as SpiMock, Transaction as SpiTransaction},
    };

    use crate::error::ConfigError;

    fn configure_rx_expectations() -> Vec<SpiTransaction<u8>> {
        let expectations = [
            // Enable pipe 0 Enhanced ShockBurst; TX packets select NO_ACK.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x21, 0x01], vec![0x00, 0x00]),
            SpiTransaction::transaction_end(),
            // Enable RX pipe 0.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x22, 0x01], vec![0x00, 0x00]),
            SpiTransaction::transaction_end(),
            // RF channel = 50.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x25, 0x32], vec![0x00, 0x00]),
            SpiTransaction::transaction_end(),
            // RF_SETUP = 1 Mbps, minimum power.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x26, 0x00], vec![0x00, 0x00]),
            SpiTransaction::transaction_end(),
            // SETUP_AW = 5-byte address width.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x23, 0x03], vec![0x00, 0x00]),
            SpiTransaction::transaction_end(),
            // RX_ADDR_P0 = [0xE7; 5].
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0x2A, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7]),
            SpiTransaction::transaction_end(),
            // TX_ADDR = [0xE7; 5].
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0x30, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7]),
            SpiTransaction::transaction_end(),
            // FEATURE = EN_DPL | EN_DYN_ACK.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x3D, 0x05], vec![0x00, 0x00]),
            SpiTransaction::transaction_end(),
            // DYNPD = DPL_P0.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x3C, 0x01], vec![0x00, 0x00]),
            SpiTransaction::transaction_end(),
            // Read CONFIG: return 0x08.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x00, 0xFF], vec![0x00, 0x08]),
            SpiTransaction::transaction_end(),
            // Write CONFIG = 0x08 | PWR_UP | PRIM_RX.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x20, 0x0B], vec![0x00, 0x00]),
            SpiTransaction::transaction_end(),
        ];

        expectations.into()
    }

    fn verify_rx_expectations(config_value: u8) -> Vec<SpiTransaction<u8>> {
        verify_rx_expectations_with_tx_addr(config_value, [0xE7; 5])
    }

    fn verify_rx_expectations_with_tx_addr(
        config_value: u8,
        tx_addr: [u8; 5],
    ) -> Vec<SpiTransaction<u8>> {
        let registers = [
            (CONFIG, config_value),
            (EN_AA, 0x01),
            (EN_RXADDR, 0x01),
            (RF_CH, 0x32),
            (RF_SETUP, 0x00),
            (SETUP_AW, 0x03),
            (FEATURE, 0x05),
            (DYNPD, 0x01),
        ];

        let mut expectations = Vec::new();

        for (register, value) in registers {
            expectations.extend([
                SpiTransaction::transaction_start(),
                SpiTransaction::transfer_in_place(vec![register, 0xFF], vec![0x00, value]),
                SpiTransaction::transaction_end(),
            ]);
        }

        // read_register_bytes(RX_ADDR_P0, ...)
        expectations.extend([
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![RX_ADDR_P0]),
            SpiTransaction::read_vec(vec![0xE7; 5]),
            SpiTransaction::transaction_end(),
        ]);

        // read_register_bytes(TX_ADDR, ...)
        expectations.extend([
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![TX_ADDR]),
            SpiTransaction::read_vec(tx_addr.to_vec()),
            SpiTransaction::transaction_end(),
        ]);

        expectations
    }

    #[test]
    fn accept_valid_tx_config() {
        let config = RadioConfig::new(50, [0xE7; 5]);

        assert!(config.is_ok());
    }

    #[test]
    fn reject_invalid_tx_channel() {
        let config = RadioConfig::new(126, [0xE7; 5]);

        assert_eq!(config, Err(ConfigError::InvalidChannel));
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
    fn configure_tx_writes_expected_registers() {
        let expectations = [
            // Disable Auto ACK.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x21, 0x00], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // Set RF channel to 50.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x25, 0x32], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 1 Mbps, minimum output power.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x26, 0x00], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // Set TX address.
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0x30, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7]),
            SpiTransaction::transaction_end(),
            // Read CONFIG (original = 0x08).
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x00, 0xFF], vec![0x0E, 0x08]),
            SpiTransaction::transaction_end(),
            // Write CONFIG = 0x0A.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x20, 0x0A], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
        ];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);
        let mut radio = Nrf24::new(spi, ce);

        let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

        radio.configure_tx(&config).unwrap();

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn initialize_tx_transitions_from_unknown_to_ready() {
        let expectations = [
            // 1. Disable Auto ACK.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x21, 0x00], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 2. RF_CH = 50.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x25, 0x32], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 3. RF_SETUP = 0x00.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x26, 0x00], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 4. TX_ADDR = [0xE7; 5].
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0x30, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7]),
            SpiTransaction::transaction_end(),
            // 5. Read original CONFIG = 0x08.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x00, 0xFF], vec![0x0E, 0x08]),
            SpiTransaction::transaction_end(),
            // 6. Power up in PTX mode: CONFIG = 0x0A.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x20, 0x0A], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 7. Verify CONFIG.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x00, 0xFF], vec![0x0E, 0x0A]),
            SpiTransaction::transaction_end(),
            // 8. Verify EN_AA.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x01, 0xFF], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 9. Verify RF_CH.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x05, 0xFF], vec![0x0E, 0x32]),
            SpiTransaction::transaction_end(),
            // 10. Verify RF_SETUP.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x06, 0xFF], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 11. Verify TX_ADDR.
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0x10]),
            SpiTransaction::read_vec(vec![0xE7; 5]),
            SpiTransaction::transaction_end(),
        ];

        let ce_expectations = [PinTransaction::set(State::Low)];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);
        let mut delay = NoopDelay::new();

        let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

        assert_eq!(radio.state(), RadioState::Unknown);

        radio.initialize_tx(&config, &mut delay).unwrap();

        assert_eq!(radio.mode, Some(OperatingMode::Tx));
        assert_eq!(radio.state(), RadioState::Ready);

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn initialize_tx_verification_failure_sets_faulted() {
        let expectations = [
            // 1. Disable Auto ACK.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x21, 0x00], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 2. RF_CH = 50.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x25, 0x32], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 3. RF_SETUP = 0x00.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x26, 0x00], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 4. TX_ADDR = [0xE7; 5].
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0x30, 0xE7, 0xE7, 0xE7, 0xE7, 0xE7]),
            SpiTransaction::transaction_end(),
            // 5. Read original CONFIG = 0x08.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x00, 0xFF], vec![0x0E, 0x08]),
            SpiTransaction::transaction_end(),
            // 6. Power up in PTX mode: CONFIG = 0x0A.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x20, 0x0A], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 7. Verify CONFIG.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x00, 0xFF], vec![0x0E, 0x0A]),
            SpiTransaction::transaction_end(),
            // 8. Verify EN_AA.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x01, 0xFF], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 9. Expected channel 50, but hardware reports channel 42.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x05, 0xFF], vec![0x0E, 0x2A]),
            SpiTransaction::transaction_end(),
            // 10. Verify RF_SETUP.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![0x06, 0xFF], vec![0x0E, 0x00]),
            SpiTransaction::transaction_end(),
            // 11. Verify TX_ADDR.
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![0x10]),
            SpiTransaction::read_vec(vec![0xE7; 5]),
            SpiTransaction::transaction_end(),
        ];

        let ce_expectations = [PinTransaction::set(State::Low)];

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&ce_expectations);

        let mut radio = Nrf24::new(spi, ce);
        let mut delay = NoopDelay::new();

        let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

        assert_eq!(radio.state(), RadioState::Unknown);

        let result = radio.initialize_tx(&config, &mut delay);

        assert_eq!(result, Err(InitError::VerificationFailed));

        assert_eq!(radio.state(), RadioState::Faulted);

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
    }

    #[test]
    fn configure_rx_writes_expected_registers() {
        let expectations = configure_rx_expectations();

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);
        let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

        assert_eq!(radio.configure_rx(&config), Ok(()));

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn verify_rx_rejects_wrong_tx_addr() {
        let config = RadioConfig::new(50, [0xE7; 5]).unwrap();
        let expectations = verify_rx_expectations_with_tx_addr(0x0B, [0xC2; 5]);

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);

        assert_eq!(
            radio.verify_rx_config(&config),
            Err(InitError::VerificationFailed)
        );

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn initialize_rx_starts_listening_after_verification() {
        let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

        let mut expectations = configure_rx_expectations();
        expectations.extend(verify_rx_expectations(0x0B));

        let spi = SpiMock::new(&expectations);

        let ce = PinMock::new(&[
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
        ]);

        let mut delay = DelayMock::new(&[
            DelayTransaction::delay_ms(POWER_UP_DELAY_MS),
            DelayTransaction::delay_us(RADIO_SETTLING_US),
        ]);

        let mut radio = Nrf24::new(spi, ce);

        assert_eq!(radio.initialize_rx_inner(&config, &mut delay), Ok(()));

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
        delay.done();
    }

    #[test]
    fn initialize_rx_does_not_listen_when_verification_fails() {
        let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

        let mut expectations = configure_rx_expectations();

        // PRIM_RX is not set.
        expectations.extend(verify_rx_expectations(0x0A));

        let spi = SpiMock::new(&expectations);

        // CE must never go HIGH.
        let ce = PinMock::new(&[PinTransaction::set(PinState::Low)]);

        let mut delay = DelayMock::new(&[DelayTransaction::delay_ms(POWER_UP_DELAY_MS)]);

        let mut radio = Nrf24::new(spi, ce);

        assert_eq!(
            radio.initialize_rx_inner(&config, &mut delay),
            Err(InitError::VerificationFailed)
        );

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
        delay.done();
    }

    #[test]
    fn initialize_transitions_to_ready() {
        let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

        let mut expectations = configure_rx_expectations();
        expectations.extend(verify_rx_expectations(0x0B));

        let spi = SpiMock::new(&expectations);

        let ce = PinMock::new(&[
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
        ]);

        let mut delay = DelayMock::new(&[
            DelayTransaction::delay_ms(POWER_UP_DELAY_MS),
            DelayTransaction::delay_us(RADIO_SETTLING_US),
        ]);

        let mut radio = Nrf24::new(spi, ce);

        assert_eq!(radio.initialize(&config, &mut delay), Ok(()));
        assert_eq!(radio.state(), RadioState::Ready);
        assert_eq!(radio.mode, Some(OperatingMode::Rx));

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
        delay.done();
    }

    #[test]
    fn initialize_transitions_to_faulted_on_error() {
        let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

        let mut expectations = configure_rx_expectations();

        // PRIM_RX is not set.
        expectations.extend(verify_rx_expectations(0x0A));

        let spi = SpiMock::new(&expectations);

        // CE must never go HIGH.
        let ce = PinMock::new(&[PinTransaction::set(PinState::Low)]);

        let mut delay = DelayMock::new(&[DelayTransaction::delay_ms(POWER_UP_DELAY_MS)]);

        let mut radio = Nrf24::new(spi, ce);

        assert_eq!(
            radio.initialize(&config, &mut delay),
            Err(InitError::VerificationFailed)
        );
        assert_eq!(radio.state(), RadioState::Faulted);
        assert_eq!(radio.mode, None);

        let (mut spi, mut ce) = radio.release();

        spi.done();
        ce.done();
        delay.done();
    }

    #[test]
    fn verify_tx_accepts_custom_rf_setup() {
        let config = RadioConfig::new(50, [0xE7; 5])
            .unwrap()
            .with_data_rate(DataRate::Mbps2)
            .with_tx_power(TxPower::High);

        let registers = [
            (CONFIG, 0x0A),   // PWR_UP = 1, PRIM_RX = 0
            (EN_AA, 0x00),    // Auto ACK disabled
            (RF_CH, 0x32),    // Channel 50
            (RF_SETUP, 0x0C), // 2 Mbps + High power
        ];

        let mut expectations = Vec::new();

        for (register, value) in registers {
            expectations.extend([
                SpiTransaction::transaction_start(),
                SpiTransaction::transfer_in_place(vec![register, 0xFF], vec![0x00, value]),
                SpiTransaction::transaction_end(),
            ]);
        }

        // Verify TX_ADDR = [0xE7; 5].
        expectations.extend([
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![TX_ADDR]),
            SpiTransaction::read_vec(vec![0xE7; 5]),
            SpiTransaction::transaction_end(),
        ]);

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);

        assert_eq!(radio.verify_tx_config(&config), Ok(()));

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    #[test]
    fn verify_tx_fails_custom_rf_setup() {
        let config = RadioConfig::new(50, [0xE7; 5])
            .unwrap()
            .with_data_rate(DataRate::Mbps2)
            .with_tx_power(TxPower::High);

        let registers = [
            (CONFIG, 0x0A),   // PWR_UP = 1, PRIM_RX = 0
            (EN_AA, 0x00),    // Auto ACK disabled
            (RF_CH, 0x32),    // Channel 50
            (RF_SETUP, 0x00), // Incorrect: expected 0x0C
        ];

        let mut expectations = Vec::new();

        for (register, value) in registers {
            expectations.extend([
                SpiTransaction::transaction_start(),
                SpiTransaction::transfer_in_place(vec![register, 0xFF], vec![0x00, value]),
                SpiTransaction::transaction_end(),
            ]);
        }

        // Verify TX_ADDR = [0xE7; 5].
        expectations.extend([
            SpiTransaction::transaction_start(),
            SpiTransaction::write_vec(vec![TX_ADDR]),
            SpiTransaction::read_vec(vec![0xE7; 5]),
            SpiTransaction::transaction_end(),
        ]);

        let spi = SpiMock::new(&expectations);
        let ce = PinMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);

        assert_eq!(
            radio.verify_tx_config(&config),
            Err(InitError::VerificationFailed)
        );

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
    }

    fn switch_expectations(original: u8, updated: u8) -> Vec<SpiTransaction<u8>> {
        vec![
            // Read CONFIG.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![CONFIG, 0xFF], vec![0x00, original]),
            SpiTransaction::transaction_end(),
            // Write CONFIG.
            SpiTransaction::transaction_start(),
            SpiTransaction::transfer_in_place(vec![W_REGISTER | CONFIG, updated], vec![0x00, 0x00]),
            SpiTransaction::transaction_end(),
        ]
    }

    #[test]
    fn switch_to_tx_clears_prim_rx() {
        let spi = SpiMock::new(&switch_expectations(0x0B, 0x0A));

        let ce = PinMock::new(&[PinTransaction::set(PinState::Low)]);

        let mut delay = DelayMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);

        // Simulate an already initialized receiver.
        radio.state = RadioState::Ready;
        radio.mode = Some(OperatingMode::Rx);

        assert_eq!(radio.switch_to_tx(), Ok(()));

        assert_eq!(radio.state(), RadioState::Ready);
        assert_eq!(radio.mode, Some(OperatingMode::Tx));

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
        delay.done();
    }

    #[test]
    fn switch_to_rx_sets_prim_rx() {
        let spi = SpiMock::new(&switch_expectations(0x0A, 0x0B));

        let ce = PinMock::new(&[
            PinTransaction::set(PinState::Low),
            PinTransaction::set(PinState::High),
        ]);

        let mut delay = DelayMock::new(&[DelayTransaction::delay_us(RADIO_SETTLING_US)]);

        let mut radio = Nrf24::new(spi, ce);

        radio.state = RadioState::Ready;
        radio.mode = Some(OperatingMode::Tx);

        assert_eq!(radio.switch_to_rx(&mut delay), Ok(()));

        assert_eq!(radio.state(), RadioState::Ready);
        assert_eq!(radio.mode, Some(OperatingMode::Rx));

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
        delay.done();
    }

    #[test]
    fn switch_to_tx_rejects_invalid_mode() {
        let spi = SpiMock::new(&[]);
        let ce = PinMock::new(&[]);
        let mut delay = DelayMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);

        radio.state = RadioState::Ready;
        radio.mode = Some(OperatingMode::Tx);

        assert_eq!(radio.switch_to_tx(), Err(InitError::NotReady));

        assert_eq!(radio.state(), RadioState::Ready);
        assert_eq!(radio.mode, Some(OperatingMode::Tx));

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
        delay.done();
    }

    #[test]
    fn switch_to_rx_rejects_invalid_mode() {
        let spi = SpiMock::new(&[]);
        let ce = PinMock::new(&[]);
        let mut delay = DelayMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);

        radio.state = RadioState::Ready;
        radio.mode = Some(OperatingMode::Rx);

        assert_eq!(radio.switch_to_rx(&mut delay), Err(InitError::NotReady));

        assert_eq!(radio.state(), RadioState::Ready);
        assert_eq!(radio.mode, Some(OperatingMode::Rx));

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
        delay.done();
    }

    #[test]
    fn switch_failure_faults_driver() {
        let spi = SpiMock::new(&[]);

        let ce = PinMock::new(&[
            PinTransaction::set(PinState::Low).with_error(MockError::Io(ErrorKind::Other))
        ]);

        let mut delay = DelayMock::new(&[]);

        let mut radio = Nrf24::new(spi, ce);

        radio.state = RadioState::Ready;
        radio.mode = Some(OperatingMode::Rx);

        let result = radio.switch_to_tx();

        assert!(matches!(result, Err(InitError::Pin(_))));

        // Fail-stop invariant.
        assert_eq!(radio.state(), RadioState::Faulted);
        assert_eq!(radio.mode, None);

        let (mut spi, mut ce) = radio.release();
        spi.done();
        ce.done();
        delay.done();
    }

    #[test]
    fn configure_rx_normalizes_crc_after_mcu_only_reset() {
        for original in [0x00, 0x0C, 0x7C] {
            let mut e = configure_rx_expectations();
            let n = e.len();
            e[n - 5] = SpiTransaction::transfer_in_place(vec![0x00, 0xFF], vec![0, original]);
            e[n - 2] = SpiTransaction::transfer_in_place(
                vec![0x20, (original | 0x0B) & !0x04],
                vec![0, 0],
            );
            let mut radio = Nrf24::new(SpiMock::new(&e), PinMock::new(&[]));
            radio
                .configure_rx(&RadioConfig::new(50, [0xE7; 5]).unwrap())
                .unwrap();
            let (mut spi, mut ce) = radio.release();
            spi.done();
            ce.done();
        }
    }

    #[test]
    fn verify_rx_rejects_wrong_crc_and_disabled_dpl() {
        for (index, register, value) in
            [(0, 0x00, 0x03), (0, 0x00, 0x0F), (6, 0x1D, 0), (7, 0x1C, 0)]
        {
            let mut e = verify_rx_expectations(0x0B);
            e[index * 3 + 1] =
                SpiTransaction::transfer_in_place(vec![register, 0xFF], vec![0, value]);
            let mut radio = Nrf24::new(SpiMock::new(&e), PinMock::new(&[]));
            assert_eq!(
                radio.verify_rx_config(&RadioConfig::new(50, [0xE7; 5]).unwrap()),
                Err(InitError::VerificationFailed)
            );
            let (mut spi, mut ce) = radio.release();
            spi.done();
            ce.done();
        }
    }
}
