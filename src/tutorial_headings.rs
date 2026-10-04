use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
struct Input {
    policy: String,
    draft_sha256: String,
    fill: u8,
    hosts: Vec<Heading>,
}
#[derive(Deserialize)]
struct Heading {
    id: String,
    index: usize,
    text: String,
    tiles: usize,
    tile_bytes: usize,
    map: usize,
    source_tiles_sha256: String,
    source_map_sha256: String,
    rows: Vec<String>,
}
fn word(source: &[u8], at: usize) -> usize {
    u16::from_le_bytes(source[at..at + 2].try_into().unwrap()) as usize
}
fn pointer(source: &[u8], at: usize) -> usize {
    u32::from_le_bytes(source[at..at + 4].try_into().unwrap()) as usize - 0x08000000
}

pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    crate::source::Profile::supported()?.verify(source)?;
    let raw = &crate::managed_input::read("assets/fonts/tutorial-heading-masks.json")?;
    let input: Input = serde_json::from_slice(raw)?;
    let draft_raw = &crate::managed_input::read("assets/translations/tutorial-headings.json")?;
    let draft: serde_json::Value = serde_json::from_slice(draft_raw)?;
    ensure!(
        input.policy == "development_selected_headings"
            && draft["policy"] == "development_selected_headings",
        "제목 개발 입력 정책 오류"
    );
    ensure!(
        input.draft_sha256 == crate::source::sha256(draft_raw) && input.fill == 2,
        "제목 문안/색인 결속 오류"
    );
    ensure!(input.hosts.len() == 7, "제목 분모 오류");
    let mut target = source.to_vec();
    let mut reports = Vec::new();
    for (ordinal, host) in input.hosts.iter().enumerate() {
        let index = ordinal + 13;
        ensure!(
            host.index == index
                && host.tiles == pointer(source, 0x5653d4 + index * 4)
                && host.map == pointer(source, 0x565424 + index * 4)
                && host.tile_bytes == word(source, 0x2468a + index * 2)
                && word(source, 0x246b2 + index * 2) == 1280,
            "제목 ROM 경계 불일치"
        );
        ensure!(
            crate::source::sha256(&source[host.tiles..host.tiles + host.tile_bytes])
                == host.source_tiles_sha256
                && crate::source::sha256(&source[host.map..host.map + 1280])
                    == host.source_map_sha256,
            "제목 원본 해시 오류"
        );
        let entry = &draft["entries"][ordinal];
        ensure!(
            entry["id"] == host.id && entry["korean_text"] == host.text && entry["index"] == index,
            "제목 문안 불일치"
        );
        ensure!(
            host.rows.len() == 24
                && host
                    .rows
                    .iter()
                    .all(|r| r.len() == 256 && r.bytes().all(|c| c == b'0' || c == b'1')),
            "제목 마스크 형식 오류"
        );
        ensure!(
            host.rows
                .iter()
                .all(|r| r.as_bytes()[240..].iter().all(|c| *c == b'0')),
            "실화면 밖 제목 획"
        );
        let entries: Vec<_> = (0..640).map(|c| word(source, host.map + c * 2)).collect();
        ensure!(
            entries
                .iter()
                .all(|e| (e & 1023) * 64 + 64 <= host.tile_bytes),
            "제목 타일 범위 오류"
        );
        let mut protected = BTreeSet::from([0usize]);
        for &entry in &entries[96..] {
            let id = entry & 1023;
            ensure!(
                source[host.tiles + id * 64..host.tiles + (id + 1) * 64]
                    .iter()
                    .all(|v| *v == 0),
                "제목 밖 원화 존재"
            );
            protected.insert(id);
        }
        let mut lookup = BTreeMap::new();
        for &id in &protected {
            lookup.insert(
                source[host.tiles + id * 64..host.tiles + (id + 1) * 64].to_vec(),
                id,
            );
        }
        let mut free = (0..host.tile_bytes / 64).filter(|id| !protected.contains(id));
        for (cell, entry) in entries.iter().take(96).enumerate() {
            let mut tile = Vec::with_capacity(64);
            for y in 0..8 {
                for x in 0..8 {
                    tile.push(
                        if host.rows[cell / 32 * 8 + y].as_bytes()[cell % 32 * 8 + x] == b'1' {
                            input.fill
                        } else {
                            0
                        },
                    );
                }
            }
            let id = if let Some(&id) = lookup.get(&tile) {
                id
            } else {
                let id = free.next().context("제목 고유 타일 용량 초과")?;
                target[host.tiles + id * 64..host.tiles + (id + 1) * 64].copy_from_slice(&tile);
                lookup.insert(tile, id);
                id
            };
            let at = host.map + cell * 2;
            target[at..at + 2].copy_from_slice(&((entry & 0xf000 | id) as u16).to_le_bytes());
        }
        reports.push(serde_json::json!({"index":index,"text":host.text,"unique_tiles":lookup.len(),"capacity":host.tile_bytes/64}));
    }
    Ok((
        target,
        serde_json::json!({"headings":reports,"masks_sha256":crate::source::sha256(raw),"distribution_eligible":false}),
    ))
}
