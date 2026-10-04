//! 소비 미확정 일본어 OBJ 제목7조각을 원래 크기로 작성한다.
use crate::source::sha256;
use anyhow::{Result, ensure};
use serde::Deserialize;
#[derive(Deserialize)]
struct Masks {
    source_id: String,
    policy: String,
    draft_sha256: String,
    source_wording_sha256: String,
    font_sha256: String,
    font_size: usize,
    license: String,
    edge: u8,
    fill: u8,
    consumer_status: String,
    headings: Vec<Heading>,
}
#[derive(Deserialize)]
struct Heading {
    id: String,
    indices: Vec<usize>,
    source_offsets: Vec<usize>,
    source_sha256: String,
    source_text: String,
    korean_text: String,
    status: String,
    width: usize,
    height: usize,
    rows: Vec<String>,
}
pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let raw = &crate::managed_input::read("assets/fonts/ranking-obj-heading-masks.json")?;
    let draft = &crate::managed_input::read("assets/translations/ranking-headings.json")?;
    let wording = &crate::managed_input::read("config/ranking-obj-source-text.json")?;
    let m: Masks = serde_json::from_slice(raw)?;
    let d: serde_json::Value = serde_json::from_slice(draft)?;
    let w: serde_json::Value = serde_json::from_slice(wording)?;
    ensure!(
        m.source_id == "apyj-rev0"
            && m.policy == "development_ranking_obj_headings"
            && m.draft_sha256 == sha256(draft)
            && m.source_wording_sha256 == sha256(wording)
            && m.font_sha256 == "2c709890595668f7bdb6df408420fda957dde0288e95b31a1cc17a2ab98b4b4f"
            && m.font_size == 12
            && m.license == "licenses/fonts/galmuri-v2.40.3-LICENSE.txt"
            && m.edge == 44
            && m.fill == 43
            && m.consumer_status == "unresolved"
            && m.headings.len() == 2,
        "랭킹 OBJ 제목 입력 결속 오류"
    );
    let mut target = source.to_vec();
    for (i, h) in m.headings.iter().enumerate() {
        let expected: Vec<usize> = if i == 0 {
            (24..27).collect()
        } else {
            (28..32).collect()
        };
        let offsets: Vec<_> = expected.iter().map(|n| 0x1b5168 + n * 512).collect();
        ensure!(
            h.id == w["units"][8 + i]["id"]
                && h.indices == expected
                && h.source_offsets == offsets
                && h.source_text == w["units"][8 + i]["ja"]
                && h.source_text == d["entries"][i]["source_text"]
                && h.korean_text == d["entries"][i]["korean_text"]
                && h.status == "needs_review"
                && h.width == expected.len() * 32
                && h.height == 16
                && h.rows.len() == 16
                && h.rows
                    .iter()
                    .all(|r| r.len() == h.width && r.bytes().all(|c| b"012".contains(&c))),
            "랭킹 OBJ 제목 범위/문안 오류"
        );
        let original: Vec<_> = offsets
            .iter()
            .flat_map(|&o| source[o..o + 512].iter().copied())
            .collect();
        ensure!(
            sha256(&original) == h.source_sha256,
            "랭킹 OBJ 제목 원본 오류"
        );
        for (&n, &offset) in expected.iter().zip(&offsets) {
            let p = 0x54beb8 + n * 4;
            ensure!(
                u32::from_le_bytes(source[p..p + 4].try_into()?) as usize == 0x8000000 + offset,
                "랭킹 OBJ 제목 포인터 오류"
            );
            for en in 0..33 {
                let p = 0x54bf40 + en * 4;
                let start = u32::from_le_bytes(source[p..p + 4].try_into()?) as usize - 0x8000000;
                ensure!(
                    offset + 512 <= start || start + 512 <= offset,
                    "영어 조각 중첩"
                );
            }
        }
        for (y, row) in h.rows.iter().enumerate() {
            for (x, c) in row.bytes().enumerate() {
                let offset = offsets[x / 32] + (y / 8 * 4 + x % 32 / 8) * 64 + y % 8 * 8 + x % 8;
                target[offset] = match c {
                    b'0' => 0,
                    b'1' => m.edge,
                    _ => m.fill,
                };
            }
        }
    }
    let changed = source.iter().zip(&target).filter(|(a, b)| a != b).count();
    Ok((
        target,
        serde_json::json!({"headings":2,"pieces":7,"owned_pixels":3584,"changed_bytes":changed,
        "masks_sha256":sha256(raw),"consumer_status":"unresolved","distribution_eligible":false,
        "runtime_verification":"deferred_until_cumulative_insertion"}),
    ))
}
