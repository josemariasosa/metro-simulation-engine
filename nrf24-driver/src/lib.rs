//! Minimal `no_std` driver for the nRF24L01+ transceiver.
//!
//! Built on `embedded-hal` SPI, GPIO, and delay traits.
//!
//! Initializes in RX mode and automatically restores RX after transmission.
//! Supports configurable RF settings and dynamic payloads with selective NO_ACK.
//! Initialization is verified and transmission errors use fail-stop handling.
//! Receives one dynamic payload per non-blocking poll into a caller-owned buffer.

#![no_std]

mod config;
mod driver;
mod error;
mod registers;

pub use config::{DataRate, RadioConfig, TxPower};
pub use driver::{Nrf24, RadioState};
pub use error::{
    ConfigError, ConfigureError, InitError, PayloadError, RegisterError, RxError, TxError,
};
