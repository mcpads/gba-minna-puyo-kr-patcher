//! 저장 중 안내의 팔레트 전송과 아이콘을 보존하며 한글 타일을 재배정한다.
use crate::source::sha256;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
const TILES: usize = 0xea692;
const MAP: usize = 0xec292;
#[derive(Deserialize)]
struct Draft {
    source_id: String,
    policy: String,
    source_sha256: String,
    entries: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    id: String,
    source_text: String,
    korean_text: String,
    status: String,
}
#[derive(Deserialize)]
struct Masks {
    policy: String,
    draft_sha256: String,
    font_sha256: String,
    font_size: usize,
    license: String,
    background: u8,
    fill: u8,
    surfaces: Vec<Surface>,
}
#[derive(Deserialize)]
struct Surface {
    id: String,
    text: String,
    #[serde(rename = "box")]
    bounds: [usize; 4],
    rows: Vec<String>,
}
pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let dr = &crate::managed_input::read("assets/translations/saving.json")?;
    let mr = &crate::managed_input::read("assets/fonts/saving-masks.json")?;
    let d: Draft = serde_json::from_slice(dr)?;
    let m: Masks = serde_json::from_slice(mr)?;
    ensure!(
        d.source_id == "apyj-rev0"
            && d.policy == "development_saving"
            && m.policy == d.policy
            && m.draft_sha256 == sha256(dr)
            && d.source_sha256 == sha256(&source[0xea640..0xec742])
            && d.entries.len() == 2
            && m.surfaces.len() == 2,
        "저장 안내 입력 결속 오류"
    );
    ensure!(
        m.font_sha256 == "d61b60eccb731f8ca9c7da582e4a05a94db66b570471809950aa9a7261b941d6"
            && m.font_size == 16
            && m.license == "licenses/fonts/neodgm-v1.600-OFL.txt"
            && (m.background, m.fill) == (1, 18),
        "저장 안내 폰트/색인 오류"
    );
    for (ptr, expected) in [(0x1eb20, TILES), (0x1eb24, MAP), (0x1eb10, 0xea640)] {
        ensure!(
            u32::from_le_bytes(source[ptr..ptr + 4].try_into()?) as usize == 0x8000000 + expected,
            "저장 안내 공급 포인터 오류"
        );
    }
    let entries: Vec<_> = source[MAP..MAP + 1200]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u16::from_le_bytes(*v))
        .collect();
    ensure!(
        entries.iter().all(|v| usize::from(v & 1023) < 112),
        "저장 안내 타일 참조 범위 오류"
    );
    let mut blocks = BTreeMap::new();
    for (i, (e, s)) in d.entries.iter().zip(&m.surfaces).enumerate() {
        let expected = [[80, 40, 80, 32], [16, 72, 200, 24]][i];
        let [left, top, w, h] = s.bounds;
        ensure!(
            s.bounds == expected
                && e.id == ["saving.progress", "saving.keep-power-on"][i]
                && e.id == s.id
                && e.source_text == ["セーブ中", "電源を切らないでね！"][i]
                && e.korean_text == s.text
                && !s.text.is_empty()
                && e.status == "needs_review"
                && s.rows.len() == h
                && s.rows
                    .iter()
                    .all(|r| r.len() == w && r.bytes().all(|c| b"01".contains(&c))),
            "저장 안내 문안/경계 오류"
        );
        for cy in top / 8..(top + h) / 8 {
            for cx in left / 8..(left + w) / 8 {
                let mut block = vec![m.background; 64];
                for y in 0..8 {
                    for x in 0..8 {
                        if s.rows[cy * 8 + y - top].as_bytes()[cx * 8 + x - left] == b'1' {
                            block[y * 8 + x] = m.fill;
                        }
                    }
                }
                ensure!(
                    blocks.insert(cy * 30 + cx, block).is_none(),
                    "저장 안내 편집 셀 중복"
                );
            }
        }
    }
    let mut reserved: BTreeSet<_> = (0..7).collect();
    reserved.extend(
        entries
            .iter()
            .enumerate()
            .filter(|(c, _)| !blocks.contains_key(c))
            .map(|(_, v)| usize::from(v & 1023)),
    );
    ensure!(
        reserved.len() == 14 && blocks.len() == 115,
        "저장 안내 보호 타일/셀 분모 변경"
    );
    let mut lookup = BTreeMap::new();
    for &id in &reserved {
        lookup.insert(source[TILES + id * 64..TILES + (id + 1) * 64].to_vec(), id);
    }
    let mut free = (0..112).filter(|id| !reserved.contains(id));
    let mut target = source.to_vec();
    let mut owned = BTreeSet::new();
    let mut used = 0;
    for (&cell, block) in &blocks {
        let id = if let Some(&id) = lookup.get(block) {
            id
        } else {
            let id = free.next().context("저장 안내 전용 타일 용량 초과")?;
            let at = TILES + id * 64;
            target[at..at + 64].copy_from_slice(block);
            owned.extend(at..at + 64);
            lookup.insert(block.clone(), id);
            used += 1;
            id
        };
        let at = MAP + cell * 2;
        target[at..at + 2].copy_from_slice(&((entries[cell] & 0xf000) | id as u16).to_le_bytes());
        owned.extend(at..at + 2);
    }
    // 원본 맵의 뒤집기를 포함해 보호 픽셀과 모든 작성 픽셀을 최종 결과에서 읽는다.
    for cell in 0..600 {
        let entry = u16::from_le_bytes(target[MAP + cell * 2..MAP + cell * 2 + 2].try_into()?);
        for y in 0..8 {
            for x in 0..8 {
                let pixel = |rom: &[u8], v: u16| {
                    let sx = if v & 0x400 != 0 { 7 - x } else { x };
                    let sy = if v & 0x800 != 0 { 7 - y } else { y };
                    rom[TILES + usize::from(v & 1023) * 64 + sy * 8 + sx]
                };
                let expected = blocks
                    .get(&cell)
                    .map_or_else(|| pixel(source, entries[cell]), |b| b[y * 8 + x]);
                ensure!(
                    pixel(&target, entry) == expected,
                    "저장 안내 최종 픽셀 불일치"
                );
            }
        }
    }
    ensure!(
        target[0xea640..0xea840] == source[0xea640..0xea840],
        "저장 안내 팔레트 전송 변경"
    );
    let mut changed = 0;
    for (i, (&a, &b)) in source.iter().zip(&target).enumerate() {
        if a != b {
            ensure!(owned.contains(&i), "저장 안내 보호 바이트 변경");
            changed += 1;
        }
    }
    Ok((
        target,
        serde_json::json!({"phrases":2,"editable_cells":115,"reserved_tiles":14,"available_tiles":98,"used_tiles":used,"changed_bytes":changed,"masks_sha256":sha256(mr),"pixel_roundtrip":"pass","final_write_audit":"pass","distribution_eligible":false,"runtime_verification":"deferred_until_cumulative_insertion"}),
    ))
}
