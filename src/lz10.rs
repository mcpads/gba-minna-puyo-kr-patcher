//! 부트스트랩에서 확인한 BIOS LZ77 WRAM 형식. 출력 한계와 중첩 복사를 검사한다.
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
pub fn decode(input: &[u8], limit: usize) -> Result<(Vec<u8>, usize)> {
    ensure!(input.len() >= 4 && input[0] == 0x10, "LZ10 헤더 오류");
    let size = usize::from(input[1]) | (usize::from(input[2]) << 8) | (usize::from(input[3]) << 16);
    ensure!(size > 0 && size <= limit, "LZ10 출력 크기 초과");
    let mut out = Vec::with_capacity(size);
    let mut at = 4;
    while out.len() < size {
        let flags = *input.get(at).context("잘린 LZ10 플래그")?;
        at += 1;
        for bit in (0..8).rev() {
            if out.len() == size {
                break;
            }
            if flags & (1 << bit) == 0 {
                out.push(*input.get(at).context("잘린 LZ10 literal")?);
                at += 1;
            } else {
                let hi = usize::from(*input.get(at).context("잘린 LZ10 match")?);
                let lo = usize::from(*input.get(at + 1).context("잘린 LZ10 match")?);
                at += 2;
                let len = (hi >> 4) + 3;
                let distance = ((hi & 15) << 8 | lo) + 1;
                ensure!(
                    distance <= out.len() && out.len() + len <= size,
                    "LZ10 역참조/출력 초과"
                );
                for _ in 0..len {
                    out.push(out[out.len() - distance]);
                }
            }
        }
    }
    Ok((out, at))
}
pub fn encode(data: &[u8]) -> Result<Vec<u8>> {
    let size = data.len();
    ensure!(size > 0 && size < 1 << 24, "LZ10 입력 크기 오류");
    let mut history: BTreeMap<[u8; 3], Vec<usize>> = BTreeMap::new();
    let mut matches = vec![(0, 0); size];
    for pos in 0..size {
        let limit = 18.min(size - pos);
        if limit < 3 {
            continue;
        }
        let key: [u8; 3] = data[pos..pos + 3].try_into()?;
        let candidates = history.entry(key).or_default();
        let mut best = 0;
        let mut distance = 0;
        for &previous in candidates.iter().rev() {
            if pos - previous > 4096 {
                break;
            }
            let mut length = 3;
            while length < limit && data[previous + length] == data[pos + length] {
                length += 1;
            }
            if length > best {
                best = length;
                distance = pos - previous;
            }
            if best == limit {
                break;
            }
        }
        matches[pos] = (best, distance);
        candidates.push(pos);
    }
    let mut costs = vec![[0usize; 8]; size + 1];
    let mut choices = vec![[1usize; 8]; size];
    for pos in (0..size).rev() {
        for slot in 0..8 {
            let following = (slot + 1) % 8;
            let mut cost = 1 + costs[pos + 1][following];
            let mut selected = 1;
            for length in (3..=matches[pos].0).rev() {
                let candidate = 2 + costs[pos + length][following];
                if candidate < cost {
                    cost = candidate;
                    selected = length;
                }
            }
            costs[pos][slot] = cost + usize::from(slot == 0);
            choices[pos][slot] = selected;
        }
    }
    let mut out = vec![0x10, size as u8, (size >> 8) as u8, (size >> 16) as u8];
    let mut pos = 0;
    let mut slot = 0;
    let mut flag = 0;
    while pos < size {
        if slot == 0 {
            flag = out.len();
            out.push(0);
        }
        let length = choices[pos][slot];
        if length == 1 {
            out.push(data[pos]);
        } else {
            let distance = matches[pos].1 - 1;
            out[flag] |= 1 << (7 - slot);
            out.push((((length - 3) << 4) | (distance >> 8)) as u8);
            out.push(distance as u8);
        }
        pos += length;
        slot = (slot + 1) % 8;
    }
    ensure!(out.len() == 4 + costs[0][0], "LZ10 토큰 비용 불일치");
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_overlapping_matches_and_rejects_invalid_streams() {
        assert_eq!(
            decode(&[0x10, 19, 0, 0, 0x40, b'A', 0xf0, 0], 19).unwrap(),
            (vec![b'A'; 19], 8)
        );
        for bytes in [
            &[0x10, 3, 0, 0, 0x80, 0, 0][..],
            &[0x10, 2, 0, 0, 0x40, b'A', 0, 0],
            &[0x10, 1, 0, 0, 0],
            &[0x11, 1, 0, 0, 0, b'A'],
        ] {
            assert!(decode(bytes, 32).is_err());
        }
    }
    #[test]
    fn compressor_crosses_flags_and_window_boundaries() {
        let mut data: Vec<_> = (0..4096).map(|n| ((n * 73 + n / 13) % 256) as u8).collect();
        data.extend_from_within(..18);
        data.extend_from_slice(&[42; 100]);
        for input in [&data[..7], &data[..8], &data[..9], &data[..]] {
            let packed = encode(input).unwrap();
            assert_eq!(
                decode(&packed, input.len()).unwrap(),
                (input.to_vec(), packed.len())
            );
        }
        assert!(encode(&[]).is_err());
    }
}
