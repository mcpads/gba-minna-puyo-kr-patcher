//! 대상 ROM의 제어 인자 폭으로 스트림을 파싱한다. 다음 포인터를 종결자로 쓰지 않는다.
use anyhow::{Result, bail, ensure};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Token {
    pub offset: usize,
    pub code: u16,
    pub args: Vec<u16>,
}

impl Token {
    pub fn text_or_newline(&self) -> bool {
        self.code < 0xff00 || self.code == 0xff80
    }
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.code.to_le_bytes());
        for arg in &self.args {
            out.extend_from_slice(&arg.to_le_bytes());
        }
    }
}

pub fn parse(data: &[u8], start: usize, limit: usize) -> Result<(Vec<Token>, usize)> {
    ensure!(start <= limit && limit <= data.len(), "스트림 경계 오류");
    let mut pos = start;
    let mut tokens = Vec::new();
    while pos + 2 <= limit {
        let offset = pos;
        let code = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        let width = match code {
            0..=0xfeff | 0xff07 | 0xff08 | 0xff0d | 0xff0f | 0xff10 | 0xff80 | 0xff81 => 0,
            0xff01..=0xff06 | 0xff09..=0xff0c | 0xff0e | 0xff11..=0xff16 => 1,
            _ => bail!("미해석 제어 {code:04X} @ {offset:X}"),
        };
        ensure!(pos + width * 2 <= limit, "제어 인자 잘림");
        let args = data[pos..pos + width * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect();
        pos += width * 2;
        tokens.push(Token { offset, code, args });
        if code == 0xff81 {
            let mut encoded = Vec::new();
            for token in &tokens {
                token.encode(&mut encoded);
            }
            ensure!(encoded == data[start..pos], "원본 토큰 왕복 불일치");
            return Ok((tokens, pos));
        }
    }
    bail!("종료 제어 없음 @ {start:X}")
}

pub fn protected(tokens: &[Token]) -> Vec<(u16, Vec<u16>)> {
    tokens
        .iter()
        .filter(|t| !t.text_or_newline())
        .map(|t| (t.code, t.args.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argument_looks_like_terminator_but_does_not_end_stream() {
        let data = [0x01, 0xff, 0x81, 0xff, 0x23, 0x00, 0x81, 0xff, 0x55, 0x55];
        let (tokens, end) = parse(&data, 0, data.len()).unwrap();
        assert_eq!(end, 8);
        assert_eq!(tokens[0].args, vec![0xff81]);
        let mut encoded = Vec::new();
        for token in tokens {
            token.encode(&mut encoded);
        }
        assert_eq!(encoded, data[..8]);
    }
    #[test]
    fn malformed_controls_fail_instead_of_becoming_glyphs() {
        for data in [
            vec![0x01, 0xff],
            vec![0x17, 0xff],
            vec![0x01, 0x00],
            vec![0x81],
        ] {
            assert!(parse(&data, 0, data.len()).is_err());
        }
    }
}
