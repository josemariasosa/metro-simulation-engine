use crate::error::ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioConfig {
    channel: u8,
    address: [u8; 5],
    data_rate: DataRate,
    tx_power: TxPower,
    auto_ack: bool,
}

impl RadioConfig {
    /// Creates a Radio configuration.
    ///
    /// `channel` must be in the range 0..=125.
    /// `address` is the 5-byte RF destination address.
    ///
    /// Returns `ConfigError::InvalidChannel` for an invalid channel.
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

    pub const fn channel(&self) -> u8 {
        self.channel
    }

    pub const fn address(&self) -> [u8; 5] {
        self.address
    }

    pub const fn data_rate(&self) -> DataRate {
        self.data_rate
    }

    pub const fn auto_ack(&self) -> bool {
        self.auto_ack
    }

    pub(crate) const fn rf_setup(&self) -> u8 {
        self.data_rate.bits() | self.tx_power.bits()
    }

    pub fn with_data_rate(mut self, rate: DataRate) -> Self {
        self.data_rate = rate;
        self
    }

    pub fn with_tx_power(mut self, power: TxPower) -> Self {
        self.tx_power = power;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataRate {
    Kbps250,
    Mbps1,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxPower {
    Min,
    Low,
    High,
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
