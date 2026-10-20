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
