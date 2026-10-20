mod init;
mod rx;
mod spi;
mod tx;

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use embedded_hal::spi::{Operation, SpiDevice};

use crate::config::RadioConfig;
use crate::error::{ConfigureError, InitError, PayloadError, RegisterError, RxError, TxError};
use crate::registers::*;

/// nRF24L01+ driver owning the SPI device and CE output pin.
///
/// Starts in [`RadioState::Unknown`]. Call [`Self::initialize`]
/// before attempting to transmit or receive.
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
    /// Takes ownership of an `embedded-hal` 1.0 [`SpiDevice`] (including its
    /// CSN management) and a separate CE [`OutputPin`]. Configure SPI Mode 0,
    /// MSB-first, at no more than 10 MHz before calling. No pin level is set.
    /// The initial state is [`RadioState::Unknown`], with no known mode.
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
    /// Valid in every state, including [`RadioState::Faulted`]. Performs no
    /// hardware I/O: CE, power, FIFOs, and configuration are left as they were.
    /// The caller must establish safe hardware conditions before reuse; creating
    /// a new driver around these resources does not reset the transceiver.
    pub fn release(self) -> (SPI, CE) {
        (self.spi, self.ce)
    }

    /// Returns the software state without inspecting hardware or changing CE.
    pub fn state(&self) -> RadioState {
        self.state
    }
}

/// Operational state of the radio driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioState {
    /// No initialization has completed. Call [`Nrf24::initialize`] or release resources.
    Unknown,

    /// Initialization succeeded and the mode is known. After [`Nrf24::initialize`],
    /// receive and transmit are allowed. Legacy [`Nrf24::initialize_tx`] instead
    /// leaves PTX standby, where both data APIs reject calls despite this state.
    Ready,

    /// Hardware state is uncertain after a critical error. Both data APIs and
    /// both initializers reject further use. Inspect the error or call
    /// [`Nrf24::release`]; recovery/reset is the caller's responsibility.
    /// This is a software fail-stop, not a guarantee of CE low or power-down.
    Faulted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OperatingMode {
    Rx,
    Tx,
}
