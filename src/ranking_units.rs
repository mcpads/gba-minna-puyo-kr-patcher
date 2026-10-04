//! 영문 이름 글리프와 공유하지 않는 랭킹 단위8타일을4bpp로 작성한다.
use crate::source::sha256;
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::collections::BTreeSet;
#[derive(Deserialize)]
struct Draft {
    source_id: String,
    policy: String,
    entries: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    id: String,
    source_text: String,
    korean_text: String,
    status: String,
    map_cells: Vec<usize>,
    tile_ids: Vec<usize>,
    source_sha256: Vec<String>,
}
#[derive(Deserialize)]
struct Masks {
    policy: String,
    draft_sha256: String,
    font_sha256: String,
    font_size: usize,
    license: String,
    entries: Vec<Mask>,
}
#[derive(Deserialize)]
struct Mask {
    id: String,
    text: String,
    rows: Vec<String>,
}
pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let dr = &crate::managed_input::read("assets/translations/ranking-units.json")?;
    let mr = &crate::managed_input::read("assets/fonts/ranking-unit-masks.json")?;
    let d: Draft = serde_json::from_slice(dr)?;
    let m: Masks = serde_json::from_slice(mr)?;
    ensure!(
        d.source_id == "apyj-rev0"
            && d.policy == "development"
            && m.policy == "development"
            && m.draft_sha256 == sha256(dr)
            && d.entries.len() == 2
            && m.entries.len() == 2
            && m.font_sha256 == "3882bd35066c26b0392cd4963ff9b3c151041dec34adc9d5633d137d1d9b9855"
            && m.font_size == 8
            && m.license == "licenses/fonts/galmuri-v2.40.3-LICENSE.txt",
        "랭킹 단위 입력 결속 오류"
    );
    let map: Vec<_> = source[0x106568..0x1066e8]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
        .collect();
    let mut target = source.to_vec();
    let mut owned = BTreeSet::new();
    for (n, (e, mask)) in d.entries.iter().zip(&m.entries).enumerate() {
        let (id, original, x, ids) = if n == 0 {
            ("chain", "れんさ", 4, [49, 50, 59, 60])
        } else {
            ("task", "問", 10, [54, 55, 63, 64])
        };
        let cells = [128 + x, 129 + x, 160 + x, 161 + x];
        ensure!(
            e.id == id
                && mask.id == id
                && e.source_text == original
                && e.korean_text == mask.text
                && !mask.text.is_empty()
                && e.status == "needs_review"
                && e.map_cells == cells
                && e.tile_ids == ids
                && e.source_sha256.len() == 4
                && mask.rows.len() == 16
                && mask
                    .rows
                    .iter()
                    .all(|r| r.len() == 16 && r.bytes().all(|c| b"012".contains(&c))),
            "랭킹 단위 문구/타일 경계 오류"
        );
        for (i, &t) in ids.iter().enumerate() {
            ensure!(
                map[cells[i]] == t
                    && map
                        .iter()
                        .enumerate()
                        .all(|(c, v)| cells.contains(&c) || v & 1023 != t),
                "이름/다른 문구 타일 공유"
            );
            let at = 0x105b68 + t * 32;
            ensure!(
                sha256(&source[at..at + 32]) == e.source_sha256[i],
                "랭킹 단위 원본 변경"
            );
            for y in 0..8 {
                for pair in 0..4 {
                    let x = i % 2 * 8 + pair * 2;
                    let row = mask.rows[i / 2 * 8 + y].as_bytes();
                    let p = at + y * 4 + pair;
                    ensure!(owned.insert(p), "랭킹 단위 작성 중복");
                    target[p] = (row[x] - b'0') | ((row[x + 1] - b'0') << 4);
                }
            }
        }
    }
    for (i, (&a, &b)) in source.iter().zip(&target).enumerate() {
        ensure!(a == b || owned.contains(&i), "랭킹 단위 보호 범위 변경");
    }
    Ok((
        target,
        serde_json::json!({"owned_bytes":owned.len(),"masks_sha256":sha256(mr),"distribution_eligible":false,"runtime_verification":"deferred_until_cumulative_insertion","final_write_audit":"pass"}),
    ))
}
