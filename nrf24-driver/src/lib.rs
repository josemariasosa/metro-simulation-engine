// //! Minimal, platform-independent, no_std driver
// //! for nRF24L01+ compatible transceivers.

// #![no_std]

// #[cfg(test)]
// mod tests;

//! Minimal `no_std` driver for the nRF24L01+ transceiver.
//!
//! Built on `embedded-hal` SPI, GPIO, and delay traits.
//!
//! Initializes in RX mode and automatically restores RX after transmission.
//! Supports configurable RF settings and optional automatic acknowledgments.
//! Initialization is verified and transmission errors use fail-stop handling.
//! Reading received payloads is not implemented yet.

#![no_std]

mod config;
mod driver;
mod error;
mod registers;

pub use config::{DataRate, RadioConfig, TxPower};
pub use driver::{Nrf24, RadioState};
pub use error::{ConfigError, ConfigureError, InitError, PayloadError, RegisterError, TxError};
