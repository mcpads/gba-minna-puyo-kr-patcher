use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
struct Input {
    policy: String,
    draft_sha256: String,
    hosts: Vec<Host>,
}
#[derive(Deserialize)]
struct Host {
    id: String,
    source_tiles_sha256: String,
    source_map_sha256: String,
    regions: Vec<Region>,
}
#[derive(Deserialize)]
struct Region {
    id: String,
    text: String,
    #[serde(rename = "box")]
    bounds: [usize; 4],
    rows: Vec<String>,
}
fn pixel(source: &[u8], t: usize, m: usize, x: usize, y: usize) -> u8 {
    let pos = m + (y / 8 * 30 + x / 8) * 2;
    let e = u16::from_le_bytes([source[pos], source[pos + 1]]);
    let px = if e & 0x400 != 0 { 7 - x % 8 } else { x % 8 };
    let py = if e & 0x800 != 0 { 7 - y % 8 } else { y % 8 };
    source[t + usize::from(e & 1023) * 64 + py * 8 + px]
}
pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    crate::source::Profile::supported()?.verify(source)?;
    let raw = &crate::managed_input::read("assets/fonts/link-messages-masks.json")?;
    let input: Input = serde_json::from_slice(raw)?;
    let words_raw = &crate::managed_input::read("assets/translations/link-messages.json")?;
    let words: serde_json::Value = serde_json::from_slice(words_raw)?;
    ensure!(
        input.policy == "development_link_messages"
            && words["policy"] == input.policy
            && input.draft_sha256 == crate::source::sha256(words_raw)
            && words["source_sha256"]
                == crate::source::sha256(&crate::managed_input::read(
                    "config/link-error-source-text.json"
                )?)
            && input.hosts.len() == 2,
        "종료/오류 입력 결속 오류"
    );
    let layouts = [
        (
            "exit",
            0x800,
            [
                (0xeea48, 0xef248, 0xee848),
                (5788944, 5790992, 5788432),
                (6016588, 6018636, 6016076),
            ],
        ),
        (
            "error",
            0x1000,
            [
                (0xf03a8, 0xf13a8, 0xf01a8),
                (5792704, 5796800, 5792192),
                (6020348, 6024444, 6019836),
            ],
        ),
    ];
    let mut target = source.to_vec();
    let mut reports = Vec::new();
    for (host, (name, size, copies)) in input.hosts.iter().zip(layouts) {
        let (t, m, p) = copies[0];
        ensure!(
            host.id == name && host.regions.len() == if name == "exit" { 1 } else { 2 },
            "종료/오류 분모 변경"
        );
        ensure!(
            crate::source::sha256(&source[t..t + size]) == host.source_tiles_sha256
                && crate::source::sha256(&source[m..m + 1200]) == host.source_map_sha256,
            "종료/오류 원본 변경"
        );
        let entries: Vec<_> = source[m..m + 1200]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect();
        ensure!(
            entries.iter().all(|e| usize::from(e & 1023) < size / 64),
            "종료/오류 타일 범위 오류"
        );
        let mut seen = BTreeSet::new();
        let mut blocks = BTreeMap::new();
        for region in &host.regions {
            ensure!(seen.insert(&region.id), "종료/오류 문구 중복");
            let bounds = match (name, region.id.as_str()) {
                ("exit", "farewell") => [88, 72, 64, 16],
                ("error", "communication-failure") => [88, 56, 64, 16],
                ("error", "power-cycle") => [40, 72, 160, 16],
                _ => anyhow::bail!("종료/오류 문구 범위 오류"),
            };
            let e = words["entries"]
                .as_array()
                .context("문안 없음")?
                .iter()
                .find(|e| e["id"] == region.id)
                .context("문안 ID 없음")?;
            ensure!(
                region.bounds == bounds && e["korean_text"] == region.text,
                "종료/오류 문안 불일치"
            );
            let [x, y, w, h] = bounds;
            ensure!(
                region.rows.len() == h && region.rows.iter().all(|r| r.len() == w),
                "종료/오류 조판 크기 오류"
            );
            for cy in 0..h / 8 {
                for cx in 0..w / 8 {
                    let mut tile = Vec::with_capacity(64);
                    for py in 0..8 {
                        for px in 0..8 {
                            let (xx, yy) = (x + cx * 8 + px, y + cy * 8 + py);
                            let c = region.rows[cy * 8 + py].as_bytes()[cx * 8 + px];
                            let preserve = region.id == "power-cycle"
                                && (xx - 1..=xx + 1).any(|gx| {
                                    (yy - 1..=yy + 1).any(|gy| {
                                        (40..60).contains(&gx)
                                            && (72..88).contains(&gy)
                                            && pixel(source, t, m, gx, gy) == 60
                                    })
                                });
                            ensure!((c == b'p') == preserve, "GBA 보호 마스크 변경");
                            tile.push(match c {
                                b'0' => 0,
                                b'1' => 51,
                                b'2' => 60,
                                b'p' => pixel(source, t, m, xx, yy),
                                _ => anyhow::bail!("종료/오류 조판 코드 오류"),
                            });
                        }
                    }
                    ensure!(
                        blocks
                            .insert((y / 8 + cy) * 30 + x / 8 + cx, tile)
                            .is_none(),
                        "종료/오류 편집 겹침"
                    );
                }
            }
        }
        let protected: BTreeSet<_> = entries
            .iter()
            .enumerate()
            .filter(|(c, _)| !blocks.contains_key(c))
            .map(|(_, e)| usize::from(e & 1023))
            .collect();
        let mut lookup = BTreeMap::new();
        for &id in &protected {
            lookup.insert(source[t + id * 64..t + (id + 1) * 64].to_vec(), id);
        }
        let mut free = (0..size / 64).filter(|id| !protected.contains(id));
        let mut added = 0;
        for (cell, tile) in blocks {
            let id = if let Some(&id) = lookup.get(&tile) {
                id
            } else {
                let id = free.next().context("종료/오류 용량 초과")?;
                target[t + id * 64..t + (id + 1) * 64].copy_from_slice(&tile);
                lookup.insert(tile, id);
                added += 1;
                id
            };
            target[m + cell * 2..m + cell * 2 + 2]
                .copy_from_slice(&((entries[cell] & 0xf000) | id as u16).to_le_bytes());
        }
        let changed_tiles = target[t..t + size].to_vec();
        let changed_map = target[m..m + 1200].to_vec();
        for (ct, cm, cp) in copies {
            ensure!(
                source[t..t + size] == source[ct..ct + size]
                    && source[m..m + 1200] == source[cm..cm + 1200]
                    && source[p..p + 512] == source[cp..cp + 512],
                "본체/수신 복제 관계 변경"
            );
            target[ct..ct + size].copy_from_slice(&changed_tiles);
            target[cm..cm + 1200].copy_from_slice(&changed_map);
        }
        reports.push(serde_json::json!({"id":name,"copies":3,"protected_tiles":protected.len(),"new_tiles":added,"capacity":size/64}));
    }
    Ok((
        target,
        serde_json::json!({"hosts":reports,"masks_sha256":crate::source::sha256(raw),"distribution_eligible":false}),
    ))
}
