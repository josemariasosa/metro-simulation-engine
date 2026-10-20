//! nRF24L01+ register addresses, SPI commands, and bit masks.

// SPI commands
pub(crate) const R_REGISTER: u8 = 0x00;
pub(crate) const W_REGISTER: u8 = 0x20;
pub(crate) const REGISTER_MASK: u8 = 0x1F;

pub(crate) const W_TX_PAYLOAD: u8 = 0xA0;
pub(crate) const FLUSH_TX: u8 = 0xE1;

// Registers
pub(crate) const CONFIG: u8 = 0x00;
pub(crate) const EN_AA: u8 = 0x01;
pub(crate) const EN_RXADDR: u8 = 0x02;
pub(crate) const SETUP_AW: u8 = 0x03;

pub(crate) const RF_CH: u8 = 0x05;
pub(crate) const RF_SETUP: u8 = 0x06;
pub(crate) const STATUS: u8 = 0x07;

pub(crate) const RX_ADDR_P0: u8 = 0x0A;
pub(crate) const TX_ADDR: u8 = 0x10;
pub(crate) const FIFO_STATUS: u8 = 0x17;
pub(crate) const RX_EMPTY: u8 = 1 << 0;

pub(crate) const DYNPD: u8 = 0x1C;
pub(crate) const FEATURE: u8 = 0x1D;

// CONFIG bits
pub(crate) const PWR_UP: u8 = 1 << 1;
pub(crate) const PRIM_RX: u8 = 1 << 0;

// RX configuration bits
pub(crate) const ERX_P0: u8 = 1 << 0;
pub(crate) const DPL_P0: u8 = 1 << 0;
pub(crate) const EN_DPL: u8 = 1 << 2;

// Address width
pub(crate) const ADDRESS_WIDTH_5_BYTES: u8 = 0x03;

// STATUS bits
pub(crate) const TX_DS: u8 = 1 << 5;
pub(crate) const MAX_RT: u8 = 1 << 4;

// FIFO_STATUS bits
pub(crate) const TX_EMPTY: u8 = 1 << 4;

/// Power-up delay in milliseconds.
///
/// With the datasheet's recommended crystal (Ls < 30 mH), power down
/// to standby-I takes up to 1.5 ms. The 2 ms delay assumes that crystal;
/// higher-inductance crystals require a longer startup (Table 16).
pub(crate) const POWER_UP_DELAY_MS: u32 = 2;

/// Radio settling time in microseconds.
///
/// The nRF24L01+ requires up to 130 µs to transition from
/// Standby-I to an active RX or TX state.
pub(crate) const RADIO_SETTLING_US: u32 = 130;

/// Minimum CE pulse duration in microseconds.
///
/// In PTX mode, CE must remain HIGH for at least 10 µs
/// to initiate a transmission.
pub(crate) const MIN_CE_PULSE_US: u32 = 10;
