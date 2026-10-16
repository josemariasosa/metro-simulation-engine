use crate::frame::Frame;
use crate::message::{Command, Message};

#[derive(Debug, PartialEq, Eq)]
pub enum EncodeError {
    InvalidValue,
    BufferTooSmall { required: usize, available: usize },
}

fn common_output_prefix(frame: &Frame, output: &mut [u8]) {
    output[0] = crate::constants::PROTOCOL_VERSION;
    output[1] = frame.message.kind() as u8;
    output[2..4].copy_from_slice(&frame.source.get().to_le_bytes());
    output[4..6].copy_from_slice(&frame.destination.get().to_le_bytes());
    output[6..10].copy_from_slice(&frame.command_id.map_or(0, |id| id.get()).to_le_bytes());
}

pub fn encode(frame: &Frame, output: &mut [u8]) -> Result<usize, EncodeError> {
    let required = frame.message.kind().encoded_len();

    match &frame.message {
        Message::Command(Command::Depart { train_id, from, to }) => {
            if frame.command_id.is_none() || frame.source == frame.destination || from == to {
                return Err(EncodeError::InvalidValue);
            }

            if output.len() < required {
                return Err(EncodeError::BufferTooSmall {
                    required,
                    available: output.len(),
                });
            }

            common_output_prefix(frame, output);
            output[10..12].copy_from_slice(&train_id.get().to_le_bytes());
            output[12..14].copy_from_slice(&from.get().to_le_bytes());
            output[14..16].copy_from_slice(&to.get().to_le_bytes());
        }
        _ => todo!("Other message encoders"),
    }

    Ok(required)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::departure_frame;

    #[test]
    fn encode_depart_matches_spec() {
        let frame = departure_frame();

        let mut buffer = [0u8; 32];
        let result = encode(&frame, &mut buffer);
        assert_eq!(result, Ok(16));
        assert_eq!(
            &buffer[..16],
            &[
                crate::constants::PROTOCOL_VERSION,
                frame.message.kind() as u8,
                1,
                0, // source
                2,
                0, // destination
                4,
                3,
                2,
                1, // command_id
                3,
                0, // train_id
                16,
                0, // from
                32,
                0, // to
            ]
        );
    }

    #[test]
    fn encode_depart_rejects_small_buffer() {
        let frame = departure_frame();

        let mut buffer = [0xAAu8; 15];
        let result = encode(&frame, &mut buffer);
        assert_eq!(
            result,
            Err(EncodeError::BufferTooSmall {
                required: 16,
                available: 15
            })
        );
        assert!(buffer.iter().all(|&b| b == 0xAA));
    }

    #[test]
    fn encode_depart_rejects_invalid_route() {
        let mut frame = departure_frame();
        if let Message::Command(Command::Depart { from, to, .. }) = &mut frame.message {
            *to = *from;
        }

        let mut buffer = [0xAAu8; 32];
        let result = encode(&frame, &mut buffer);
        assert_eq!(result, Err(EncodeError::InvalidValue));
        assert!(buffer.iter().all(|&b| b == 0xAA));
    }
}
