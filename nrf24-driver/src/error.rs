#[derive(Debug, PartialEq, Eq)]
pub enum PayloadError<E> {
    InvalidLength,
    Spi(E),
}

#[derive(Debug, PartialEq, Eq)]
pub enum RegisterError<E> {
    InvalidLength,
    Spi(E),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    InvalidChannel,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigureError<E> {
    Spi(E),
    Register(RegisterError<E>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum TxError<SpiError, PinError> {
    NotReady,
    Spi(SpiError),
    Pin(PinError),
    Payload(PayloadError<SpiError>),
    Timeout,
    MaxRetries,
    FifoNotEmpty,
    ModeSwitch(InitError<SpiError, PinError>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum InitError<SpiError, PinError> {
    NotReady,
    Configure(ConfigureError<SpiError>),
    Pin(PinError),
    VerificationFailed,
}

/// Receive errors. Only SPI failures fault the driver.
#[derive(Debug, PartialEq, Eq)]
pub enum RxError<SpiError> {
    /// Initialization has not succeeded, the driver is faulted, or it is not in RX.
    NotReady,
    /// The oldest payload remains queued; retry with at least `required` bytes.
    BufferTooSmall { required: usize, available: usize },
    /// Width is outside 1..=32. The RX FIFO was flushed successfully.
    InvalidPayloadWidth(u8),
    /// Hardware state is uncertain, including failure during FIFO recovery.
    Spi(SpiError),
}
