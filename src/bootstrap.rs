//! 압축 부트스트랩의 일본어 오류 줄만 교체하고 원래 저장 범위 안에 재압축한다.
use crate::{lz10, source::sha256};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
const PACKED: usize = 0x5670cc;
const END: usize = 0x5696b0;
const TILES: usize = 0x245c;
const MAP: usize = 0x4c5c;
#[derive(Deserialize)]
struct Draft {
    source_id: String,
    policy: String,
    decoded_sha256: String,
    source_text: String,
    korean_text: String,
    status: String,
}
#[derive(Deserialize)]
struct Mask {
    policy: String,
    draft_sha256: String,
    font_sha256: String,
    font_size: usize,
    license: String,
    text: String,
    #[serde(rename = "box")]
    bounds: [usize; 4],
    background: u8,
    fill: u8,
    rows: Vec<String>,
}
pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let dr = &crate::managed_input::read("assets/translations/bootstrap.json")?;
    let mr = &crate::managed_input::read("assets/fonts/bootstrap-mask.json")?;
    let d: Draft = serde_json::from_slice(dr)?;
    let m: Mask = serde_json::from_slice(mr)?;
    let (data, consumed) = lz10::decode(&source[PACKED..END], 0x8000)?;
    ensure!(
        consumed == 9697
            && data.len() == 0x5428
            && sha256(&data) == "f89eaa3ff3e1f30acd2ce7e2786a2a6935b0b7aaad446457d20198bdad8e886b"
            && d.decoded_sha256 == sha256(&data),
        "부트스트랩 원본 경계 오류"
    );
    ensure!(
        d.source_id == "apyj-rev0"
            && d.policy == "development_bootstrap"
            && m.policy == d.policy
            && m.draft_sha256 == sha256(dr)
            && d.source_text == "通信に失敗しました"
            && d.korean_text == m.text
            && !m.text.is_empty()
            && d.status == "needs_review",
        "부트스트랩 문안 결속 오류"
    );
    ensure!(
        m.font_sha256 == "2c709890595668f7bdb6df408420fda957dde0288e95b31a1cc17a2ab98b4b4f"
            && m.font_size == 12
            && m.license == "licenses/fonts/galmuri-v2.40.3-LICENSE.txt"
            && m.bounds == [48, 64, 144, 24]
            && (m.background, m.fill) == (1, 18)
            && m.rows.len() == 24
            && m.rows
                .iter()
                .all(|r| r.len() == 144 && r.bytes().all(|c| b"01".contains(&c))),
        "부트스트랩 폰트/마스크 오류"
    );
    let entries: Vec<_> = data[MAP..MAP + 1200]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u16::from_le_bytes(*v))
        .collect();
    ensure!(
        entries.iter().all(|v| v & 1023 < 160),
        "부트스트랩 타일 범위 오류"
    );
    let mut blocks = BTreeMap::new();
    for cy in 8..11 {
        for cx in 6..24 {
            let mut block = vec![m.background; 64];
            for y in 0..8 {
                for x in 0..8 {
                    if m.rows[cy * 8 + y - 64].as_bytes()[cx * 8 + x - 48] == b'1' {
                        block[y * 8 + x] = m.fill;
                    }
                }
            }
            blocks.insert(cy * 30 + cx, block);
        }
    }
    let protected: BTreeSet<_> = entries
        .iter()
        .enumerate()
        .filter(|(c, _)| !blocks.contains_key(c))
        .map(|(_, v)| usize::from(v & 1023))
        .collect();
    ensure!(
        protected.len() == 105 && blocks.len() == 54,
        "부트스트랩 보호 분모 오류"
    );
    let mut result = data.clone();
    let mut lookup = BTreeMap::new();
    for &id in &protected {
        lookup.insert(data[TILES + id * 64..TILES + (id + 1) * 64].to_vec(), id);
    }
    let mut free = (0..160).filter(|id| !protected.contains(id));
    let mut owned = BTreeSet::new();
    let mut used = 0;
    for (&cell, block) in &blocks {
        let id = if let Some(&id) = lookup.get(block) {
            id
        } else {
            let id = free.next().context("부트스트랩 타일 용량 초과")?;
            let at = TILES + id * 64;
            result[at..at + 64].copy_from_slice(block);
            owned.extend(at..at + 64);
            lookup.insert(block.clone(), id);
            used += 1;
            id
        };
        let at = MAP + cell * 2;
        result[at..at + 2].copy_from_slice(&((entries[cell] & 0xf000) | id as u16).to_le_bytes());
        owned.extend(at..at + 2);
    }
    for cell in 0..600 {
        let entry = u16::from_le_bytes(result[MAP + cell * 2..MAP + cell * 2 + 2].try_into()?);
        for y in 0..8 {
            for x in 0..8 {
                let pixel = |bytes: &[u8], v: u16| {
                    let sx = if v & 0x400 != 0 { 7 - x } else { x };
                    let sy = if v & 0x800 != 0 { 7 - y } else { y };
                    bytes[TILES + usize::from(v & 1023) * 64 + sy * 8 + sx]
                };
                let expected = blocks
                    .get(&cell)
                    .map_or_else(|| pixel(&data, entries[cell]), |b| b[y * 8 + x]);
                ensure!(
                    pixel(&result, entry) == expected,
                    "부트스트랩 최종 픽셀 불일치"
                );
            }
        }
    }
    let mut changed_decoded = 0;
    for (i, (&a, &b)) in data.iter().zip(&result).enumerate() {
        if a != b {
            ensure!(owned.contains(&i), "부트스트랩 코드/보호 자료 변경");
            changed_decoded += 1;
        }
    }
    let packed = lz10::encode(&result)?;
    ensure!(packed.len() <= consumed, "부트스트랩 압축 저장 예산 초과");
    let (restored, read) = lz10::decode(&packed, 0x8000)?;
    ensure!(
        restored == result && read == packed.len(),
        "부트스트랩 압축 왕복 실패"
    );
    let mut target = source.to_vec();
    target[PACKED..PACKED + packed.len()].copy_from_slice(&packed);
    ensure!(
        target[..PACKED] == source[..PACKED]
            && target[PACKED + packed.len()..] == source[PACKED + packed.len()..],
        "부트스트랩 압축 외부 변경"
    );
    Ok((
        target,
        serde_json::json!({"stream_count":1,"decoded_bytes":data.len(),"original_packed_bytes":consumed,"packed_bytes":packed.len(),"packed_sha256":sha256(&packed),"decoded_sha256":sha256(&result),"changed_decoded_bytes":changed_decoded,"allocated_tiles":used,"protected_tiles":105,"masks_sha256":sha256(mr),"self_roundtrip":"pass","source_byte_reproduction":"not_required","target_consumer_compatibility":"pending_final_runtime","distribution_eligible":false}),
    ))
}
