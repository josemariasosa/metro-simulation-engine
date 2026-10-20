use crate::error::ConfigError;

/// RF settings consumed by [`crate::Nrf24::initialize`].
///
/// The channel and address are required. The default data rate is 1 Mbps, the
/// default nominal output power is minimum (−18 dBm), and outgoing packets use
/// selective NO_ACK. Initialization fixes the address width at five bytes,
/// CRC at one byte, and dynamic payloads at 1–32 bytes on pipe 0. The driver
/// does not expose acknowledged TX, retries, other pipes, CRC selection, or
/// an auto-acknowledgment setter. Builders change this value only; they do not
/// reconfigure a radio that has already been initialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioConfig {
    channel: u8,
    address: [u8; 5],
    data_rate: DataRate,
    tx_power: TxPower,
    auto_ack: bool,
}

impl RadioConfig {
    /// Creates settings for RF channel `0..=125` (2400 + channel MHz).
    ///
    /// `address` is used for both TX and RX pipe 0, in SPI transfer order
    /// (least-significant address byte first). All five-byte values are accepted;
    /// callers must choose an appropriate address shared by their peers.
    /// Defaults to [`DataRate::Mbps1`], [`TxPower::Min`], and NO_ACK transmission.
    /// Returns [`ConfigError::InvalidChannel`] above 125 without hardware access.
    pub fn new(channel: u8, address: [u8; 5]) -> Result<Self, ConfigError> {
        if channel > 125 {
            return Err(ConfigError::InvalidChannel);
        }

        Ok(Self {
            channel,
            address,
            data_rate: DataRate::Mbps1,
            tx_power: TxPower::Min,
            auto_ack: false,
        })
    }

    /// Returns the configured RF channel, in `0..=125`.
    pub const fn channel(&self) -> u8 {
        self.channel
    }

    /// Returns the shared TX/RX pipe-0 address in SPI transfer order.
    pub const fn address(&self) -> [u8; 5] {
        self.address
    }

    /// Returns the air data rate (default: 1 Mbps).
    pub const fn data_rate(&self) -> DataRate {
        self.data_rate
    }

    /// Whether transmitted packets request an ACK (currently always false).
    ///
    /// ENAA_P0 remains enabled for DPL; selective NO_ACK suppresses ACKs for
    /// outgoing packets. This is not a getter for EN_AA: incoming ACK-requesting
    /// packets may still cause hardware ACKs. There is no auto-ACK setter.
    pub const fn auto_ack(&self) -> bool {
        self.auto_ack
    }

    pub(crate) const fn rf_setup(&self) -> u8 {
        self.data_rate.bits() | self.tx_power.bits()
    }

    /// Selects the air data rate; communicating peers must use the same rate.
    /// Default: 1 Mbps.
    pub fn with_data_rate(mut self, rate: DataRate) -> Self {
        self.data_rate = rate;
        self
    }

    /// Selects the nominal chip TX power. Default: minimum (−18 dBm).
    ///
    /// This changes the nRF24L01+'s RF power field only. An external PA/LNA
    /// module can have different output power; this does not change receiver
    /// sensitivity.
    pub fn with_tx_power(mut self, power: TxPower) -> Self {
        self.tx_power = power;
        self
    }
}

/// Supported over-air bit rates, encoded in RF_SETUP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataRate {
    /// 250 kbit/s; requires a plus-compatible radio at both ends.
    Kbps250,
    /// 1 Mbit/s; the configuration default.
    Mbps1,
    /// 2 Mbit/s.
    Mbps2,
}

impl DataRate {
    const fn bits(self) -> u8 {
        match self {
            Self::Kbps250 => 1 << 5,
            Self::Mbps1 => 0,
            Self::Mbps2 => 1 << 3,
        }
    }
}

/// Nominal nRF24L01+ output power; PA/LNA module output can differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxPower {
    /// −18 dBm, the configuration default.
    Min,
    /// −12 dBm.
    Low,
    /// −6 dBm.
    High,
    /// 0 dBm.
    Max,
}

impl TxPower {
    const fn bits(self) -> u8 {
        match self {
            Self::Min => 0b00 << 1,
            Self::Low => 0b01 << 1,
            Self::High => 0b10 << 1,
            Self::Max => 0b11 << 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rf_setup_defaults_to_1mbps_and_min_power() {
        let config = RadioConfig::new(50, [0xE7; 5]).unwrap();

        assert_eq!(config.rf_setup(), 0x00);
    }

    #[test]
    fn rf_setup_combines_data_rate_and_tx_power() {
        let config = RadioConfig::new(50, [0xE7; 5])
            .unwrap()
            .with_data_rate(DataRate::Mbps2)
            .with_tx_power(TxPower::High);

        // RF_DR_HIGH (bit 3) | RF_PWR (bits 2:1)
        // 0b0000_1000 | 0b0000_0100
        assert_eq!(config.rf_setup(), 0x0C);
    }

    #[test]
    fn rf_setup_encodes_all_supported_combinations() {
        let cases = [
            (DataRate::Kbps250, TxPower::Min, 0x20),
            (DataRate::Kbps250, TxPower::Low, 0x22),
            (DataRate::Kbps250, TxPower::High, 0x24),
            (DataRate::Kbps250, TxPower::Max, 0x26),
            (DataRate::Mbps1, TxPower::Min, 0x00),
            (DataRate::Mbps1, TxPower::Low, 0x02),
            (DataRate::Mbps1, TxPower::High, 0x04),
            (DataRate::Mbps1, TxPower::Max, 0x06),
            (DataRate::Mbps2, TxPower::Min, 0x08),
            (DataRate::Mbps2, TxPower::Low, 0x0A),
            (DataRate::Mbps2, TxPower::High, 0x0C),
            (DataRate::Mbps2, TxPower::Max, 0x0E),
        ];

        for (rate, power, expected) in cases {
            let config = RadioConfig::new(50, [0xE7; 5])
                .unwrap()
                .with_data_rate(rate)
                .with_tx_power(power);

            assert_eq!(
                config.rf_setup(),
                expected,
                "Unexpected RF_SETUP for {rate:?} / {power:?}"
            );
        }
    }
}
