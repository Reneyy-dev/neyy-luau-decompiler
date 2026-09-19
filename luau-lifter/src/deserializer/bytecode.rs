use nom::{bytes::complete::take, number::complete::le_u8, IResult};

use super::chunk::Chunk;

#[derive(Debug)]
pub enum Bytecode {
    Error(String),
    Chunk(Chunk),
}

impl Bytecode {
    pub fn parse(input: &[u8], encode_key: u8) -> IResult<&[u8], Bytecode> {
        let (input, status_code) = le_u8(input)?;
        match status_code {
            0 => {
                let (input, error_msg) = take(input.len())(input)?;
                Ok((
                    input,
                    Bytecode::Error(String::from_utf8_lossy(error_msg).to_string()),
                ))
            }
            4..=6 => {
                let (input, chunk) = Chunk::parse(input, encode_key, status_code)?;
                Ok((input, Bytecode::Chunk(chunk)))
            }
            _ => panic!("Unsupported bytecode version: {}", status_code),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_12_proto_size_envelope() {
        // v12, types v0, no strings, one proto. The proto is prefixed by
        // protoSize=17 and contains a single RETURN instruction.
        let bytecode = [
            12, 0, 0, 1, 17,
            1, 0, 0, 0, 0,
            0,
            1, 0x16, 0x00, 0x01, 0x00,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];

        let (rest, parsed) = Bytecode::parse(&bytecode, 1).expect("v12 should parse");
        assert!(rest.is_empty());

        match parsed {
            Bytecode::Chunk(chunk) => {
                assert_eq!(chunk.functions.len(), 1);
                assert_eq!(chunk.main, 0);
            }
            other => panic!("expected chunk, got {other:?}"),
        }
    }
}
