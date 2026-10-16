mod decode;
mod encode;

pub use decode::{DecodeError, Field, decode};
pub use encode::{EncodeError, encode};

#[cfg(test)]
mod tests;
