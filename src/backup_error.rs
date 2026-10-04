use anyhow::{Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
const REGION: usize = 0x900000;
const TILES: usize = REGION + 0x100;
const MAP: usize = TILES + 0x3000;
const END: usize = MAP + 1200;
#[derive(Deserialize)]
struct Mask {
    policy: String,
    draft_sha256: String,
    source_tiles_sha256: String,
    source_map_sha256: String,
    text: String,
    #[serde(rename = "box")]
    bounds: [usize; 4],
    background: u8,
    fill: u8,
    rows: Vec<String>,
}
pub fn apply(source: &[u8], target: &mut [u8], dialogue_end: usize) -> Result<serde_json::Value> {
    crate::source::Profile::supported()?.verify(source)?;
    ensure!(
        source.len() <= REGION && dialogue_end <= REGION && END <= target.len(),
        "저장 오류 확장 배치 충돌"
    );
    ensure!(
        target[REGION..END].iter().all(|&b| b == 0xff),
        "저장 오류 확장 영역 작성 충돌"
    );
    let raw = &crate::managed_input::read("assets/fonts/backup-error-masks.json")?;
    let mask: Mask = serde_json::from_slice(raw)?;
    let words_raw = &crate::managed_input::read("assets/translations/backup-error.json")?;
    let words: serde_json::Value = serde_json::from_slice(words_raw)?;
    let (t, m) = (0xe7190, 0xea190);
    ensure!(
        mask.policy == "development_backup_error"
            && words["policy"] == mask.policy
            && mask.draft_sha256 == crate::source::sha256(words_raw)
            && words["source_sha256"]
                == crate::source::sha256(&crate::managed_input::read(
                    "config/link-error-source-text.json"
                )?)
            && words["korean_text"] == mask.text,
        "저장 오류 관리 입력 결속 오류"
    );
    ensure!(
        mask.source_tiles_sha256 == crate::source::sha256(&source[t..t + 0x3000])
            && mask.source_map_sha256 == crate::source::sha256(&source[m..m + 1200]),
        "저장 오류 원본 변경"
    );
    ensure!(
        mask.bounds == [40, 56, 160, 40]
            && (mask.background, mask.fill) == (1, 18)
            && mask.rows.len() == 40
            && mask
                .rows
                .iter()
                .all(|r| r.len() == 160 && r.bytes().all(|c| c == b'0' || c == b'1')),
        "저장 오류 마스크 범위 오류"
    );
    let entries: Vec<_> = source[m..m + 1200]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect();
    ensure!(
        entries.iter().all(|e| e & 1023 < 192),
        "저장 오류 원본 타일 범위 오류"
    );
    let cells: BTreeSet<_> = (7..12)
        .flat_map(|y| (5..25).map(move |x| y * 30 + x))
        .collect();
    let protected: BTreeSet<_> = entries
        .iter()
        .enumerate()
        .filter(|(c, _)| !cells.contains(c))
        .map(|(_, e)| usize::from(e & 1023))
        .collect();
    let mut lookup = BTreeMap::new();
    for &id in &protected {
        lookup.insert(source[t + id * 64..t + (id + 1) * 64].to_vec(), id);
    }
    let mut new_tiles = source[t..t + 0x3000].to_vec();
    let mut new_map = source[m..m + 1200].to_vec();
    let mut free = (0..192).filter(|id| !protected.contains(id));
    let mut added = 0;
    for cell in cells {
        let mut tile = Vec::with_capacity(64);
        for y in 0..8 {
            for x in 0..8 {
                tile.push(
                    if mask.rows[(cell / 30 - 7) * 8 + y].as_bytes()[(cell % 30 - 5) * 8 + x]
                        == b'1'
                    {
                        18
                    } else {
                        1
                    },
                );
            }
        }
        let id = if let Some(&id) = lookup.get(&tile) {
            id
        } else {
            let id = free
                .next()
                .ok_or_else(|| anyhow::anyhow!("저장 오류 조판 용량 초과"))?;
            new_tiles[id * 64..(id + 1) * 64].copy_from_slice(&tile);
            lookup.insert(tile, id);
            added += 1;
            id
        };
        new_map[cell * 2..cell * 2 + 2]
            .copy_from_slice(&((entries[cell] & 0xf000) | id as u16).to_le_bytes());
    }
    let mut programs = Vec::new();
    for (at, expected, origin, korean, map) in [
        (
            0x1f31c,
            vec![0x18, 0x48, 0xc0, 0x21, 0xc9, 0x04, 0xc0, 0x22],
            0x08900000,
            0x08000000 + TILES as u32,
            false,
        ),
        (
            0x1f32a,
            vec![0x16, 0x48, 0x16, 0x49, 0x14, 0x22, 0x00, 0x92, 0x00, 0x22],
            0x08900040,
            0x08000000 + MAP as u32,
            true,
        ),
    ] {
        ensure!(
            source[at..at + expected.len()] == expected
                && target[at..at + expected.len()] == expected,
            "저장 오류 설치 지점/작성 충돌"
        );
        let entry = crate::backup_error_hook::entry(0x08000000 + at as u32, origin)?;
        let selector = crate::backup_error_hook::selector(origin, korean, map)?;
        ensure!(
            entry.bytes().len() == expected.len() && selector.bytes().len() <= 0x40,
            "저장 오류 훅 배치 크기 오류"
        );
        let offset = (origin - 0x08000000) as usize;
        target[at..at + entry.bytes().len()].copy_from_slice(entry.bytes());
        target[offset..offset + selector.bytes().len()].copy_from_slice(selector.bytes());
        programs.push(crate::backup_error_hook::report(&entry));
        programs.push(crate::backup_error_hook::report(&selector));
    }
    target[TILES..MAP].copy_from_slice(&new_tiles);
    target[MAP..END].copy_from_slice(&new_map);
    ensure!(
        target[t..t + 0x3000] == source[t..t + 0x3000]
            && target[m..m + 1200] == source[m..m + 1200],
        "저장 오류 영어 원본 보존 위반"
    );
    Ok(
        serde_json::json!({"masks_sha256":crate::source::sha256(raw),"region_start":REGION,"region_end":END,"tiles":TILES,"map":MAP,"protected_tiles":protected.len(),"new_tiles":added,"programs":programs,"distribution_eligible":false}),
    )
}
