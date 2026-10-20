/// Failure while loading a payload into the TX FIFO.
#[derive(Debug, PartialEq, Eq)]
pub enum PayloadError<E> {
    /// Payload length is outside the supported range of 1–32 bytes.
    ///
    /// When returned by [`crate::Nrf24::transmit`], no hardware access has
    /// occurred and the driver remains usable in RX mode.
    InvalidLength,
    /// The SPI device failed while writing the payload.
    ///
    /// During transmission, this is a critical error: RX restoration is
    /// attempted, but the driver is left faulted even if restoration succeeds.
    Spi(E),
}

/// Error returned by the driver's register-access helpers.
#[derive(Debug, PartialEq, Eq)]
pub enum RegisterError<E> {
    /// A multi-byte register operation was not between 1 and 5 bytes.
    ///
    /// The public initialization workflow uses fixed five-byte addresses, so
    /// this is not expected from it.
    InvalidLength,
    /// The SPI device failed during the register operation.
    Spi(E),
}

/// Failure while constructing or validating an RF configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// The requested channel is outside the nRF24L01+ range `0..=125`.
    InvalidChannel,
}

/// Failure while configuring a radio register.
#[derive(Debug, PartialEq, Eq)]
pub enum ConfigureError<E> {
    /// The SPI device failed during a single-byte register operation.
    Spi(E),
    /// A multi-byte register operation failed or had an invalid length.
    Register(RegisterError<E>),
}

/// Failure while transmitting a payload.
#[derive(Debug, PartialEq, Eq)]
pub enum TxError<SpiError, PinError> {
    /// The radio is not initialized in RX mode, or has faulted.
    ///
    /// This check does not access hardware or change the current state.
    NotReady,
    /// SPI failed during transmission or TX/RX mode handling.
    ///
    /// Once TX mode has been entered, this faults the driver even if RX
    /// restoration succeeds.
    Spi(SpiError),
    /// A CE GPIO operation failed.
    ///
    /// Once TX mode has been entered, the driver attempts RX restoration and
    /// then remains faulted; the physical CE level may be uncertain.
    Pin(PinError),
    /// The payload was invalid or could not be loaded into the TX FIFO.
    Payload(PayloadError<SpiError>),
    /// No TX completion status was observed within the bounded poll loop.
    ///
    /// RX restoration is attempted, then the driver is faulted.
    Timeout,
    /// The radio reported `MAX_RT`.
    ///
    /// RX restoration is attempted, then the driver is faulted. The public
    /// transmit workflow uses NO_ACK, so automatic retry exhaustion is not
    /// expected.
    MaxRetries,
    /// TX completion was reported but the TX FIFO was not empty.
    ///
    /// RX restoration is attempted, then the driver is faulted.
    FifoNotEmpty,
    /// The RX → TX or TX → RX mode switch failed.
    ///
    /// A failed entry switch faults immediately. A failed restoration switch
    /// also faults; if TX had already failed, the original TX error takes
    /// precedence and the restoration error is not returned.
    ModeSwitch(InitError<SpiError, PinError>),
}

/// Failure while initializing or switching the radio operating mode.
#[derive(Debug, PartialEq, Eq)]
pub enum InitError<SpiError, PinError> {
    /// Initialization or mode switching was invalid for the current state.
    ///
    /// A rejected public initialization leaves the existing state unchanged.
    NotReady,
    /// A register configuration or readback transaction failed.
    Configure(ConfigureError<SpiError>),
    /// A CE pin operation failed; the physical pin level may be uncertain.
    Pin(PinError),
    /// Configuration readback did not match the requested settings.
    ///
    /// A failed initialization marks the driver faulted.
    VerificationFailed,
}

/// Receive errors. Only SPI failures fault the driver.
#[derive(Debug, PartialEq, Eq)]
pub enum RxError<SpiError> {
    /// Initialization has not succeeded, the driver is faulted, or it is not in RX.
    ///
    /// No hardware access is performed by this rejection.
    NotReady,
    /// The oldest payload remains queued; retry with at least `required` bytes.
    ///
    /// `available` is the supplied buffer length. This error does not fault the
    /// radio and does not consume or modify the payload or buffer.
    BufferTooSmall { required: usize, available: usize },
    /// Width is outside 1–32. The RX FIFO was flushed successfully, discarding
    /// all queued payloads, including any otherwise valid ones. The radio
    /// remains usable in RX mode.
    InvalidPayloadWidth(u8),
    /// SPI failed. The payload may already have been consumed or the buffer
    /// partially written; the radio is marked faulted and the mode unknown.
    Spi(SpiError),
}
