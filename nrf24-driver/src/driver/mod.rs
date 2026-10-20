mod init;
mod rx;
mod spi;
mod tx;

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use embedded_hal::spi::{Operation, SpiDevice};

use crate::config::RadioConfig;
use crate::error::{ConfigureError, InitError, PayloadError, RegisterError, TxError};
use crate::registers::*;

/// nRF24L01+ driver owning the SPI device and CE output pin.
///
/// Starts in `RadioState::Unknown`. Call `initialize()`
/// before attempting to transmit.
///
/// SPI chip-select (CSN) is managed by the supplied `SpiDevice`.
pub struct Nrf24<SPI, CE> {
    spi: SPI,
    ce: CE,
    state: RadioState,
    mode: Option<OperatingMode>,
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
            mode: None,
        }
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
}

/// Operational state of the radio driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioState {
    /// Hardware has not completed verified initialization.
    Unknown,

    /// Verified initialization succeeded; the operating mode is known.
    Ready,

    /// A critical error occurred; further transmissions are blocked.
    Faulted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OperatingMode {
    Rx,
    Tx,
}
