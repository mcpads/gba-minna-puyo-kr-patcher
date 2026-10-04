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
    index: usize,
    tiles: usize,
    tile_bytes: usize,
    map: usize,
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
    rows: Vec<Vec<u8>>,
}

pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    crate::source::Profile::supported()?.verify(source)?;
    let raw = &crate::managed_input::read("assets/fonts/tutorial-annotation-masks.json")?;
    let input: Input = serde_json::from_slice(raw)?;
    let draft_raw = &crate::managed_input::read("assets/translations/tutorial-annotations.json")?;
    let draft: serde_json::Value = serde_json::from_slice(draft_raw)?;
    ensure!(
        input.policy == "development_selected_annotations"
            && draft["policy"] == input.policy
            && input.draft_sha256 == crate::source::sha256(draft_raw),
        "그림 문안 결속 오류"
    );
    let mut target = source.to_vec();
    let mut reports = Vec::new();
    ensure!(input.hosts.len() == 3, "그림 시트 분모 오류");
    let mut indices = BTreeSet::new();
    for host in input.hosts {
        ensure!(indices.insert(host.index), "그림 시트 중복");
        let boxes: &[[usize; 4]] = match host.index {
            4 => &[[64, 40, 16, 8]],
            8 => &[[88, 40, 16, 8], [24, 56, 16, 8]],
            11 => &[[32, 72, 56, 24]],
            _ => anyhow::bail!("그림 범위 밖"),
        };
        let expected = match host.index {
            4 => (0xb3088, 0xb3888),
            8 => (0xb5c88, 0xb6488),
            11 => (0xb8388, 0xb8b88),
            _ => unreachable!(),
        };
        ensure!(
            (host.tiles, host.map) == expected
                && host.tile_bytes == 2048
                && host.regions.len() == boxes.len(),
            "그림 저장 경계 변경"
        );
        ensure!(
            crate::source::sha256(&source[host.tiles..host.tiles + 2048])
                == host.source_tiles_sha256
                && crate::source::sha256(&source[host.map..host.map + 1280])
                    == host.source_map_sha256,
            "그림 원본 해시 변경"
        );
        let entries: Vec<_> = source[host.map..host.map + 1280]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        ensure!(
            entries.iter().all(|e| usize::from(e & 1023) < 32),
            "그림 원본 타일 범위"
        );
        let mut blocks = BTreeMap::new();
        for (reg, bounds) in host.regions.iter().zip(boxes) {
            ensure!(&reg.bounds == bounds, "그림 보호 사각형 변경");
            let entry = draft["entries"]
                .as_array()
                .context("그림 초안 없음")?
                .iter()
                .find(|e| e["id"] == reg.id)
                .context("그림 문안 없음")?;
            ensure!(entry["korean_text"] == reg.text, "그림 문안 불일치");
            let [x, y, w, h] = reg.bounds;
            ensure!(
                reg.rows.len() == h
                    && reg.rows.iter().all(|r| r.len() == w
                        && r.iter().all(|v| if host.index == 11 {
                            [0, 33, 41, 42].contains(v)
                        } else {
                            [0, 17].contains(v)
                        })),
                "그림 마스크 오류"
            );
            for cy in 0..h / 8 {
                for cx in 0..w / 8 {
                    let tile: Vec<_> = (0..8)
                        .flat_map(|py| (0..8).map(move |px| reg.rows[cy * 8 + py][cx * 8 + px]))
                        .collect();
                    ensure!(
                        blocks
                            .insert((y / 8 + cy) * 32 + x / 8 + cx, tile)
                            .is_none(),
                        "그림 편집 중복"
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
            lookup.insert(
                source[host.tiles + id * 64..host.tiles + (id + 1) * 64].to_vec(),
                id,
            );
        }
        let mut free = (0..32).filter(|id| !protected.contains(id));
        let mut used = 0;
        for (cell, tile) in blocks {
            let id = if let Some(&id) = lookup.get(&tile) {
                id
            } else {
                let id = free.next().context("그림 전용 타일 용량 초과")?;
                target[host.tiles + id * 64..host.tiles + (id + 1) * 64].copy_from_slice(&tile);
                lookup.insert(tile, id);
                used += 1;
                id
            };
            target[host.map + cell * 2..host.map + cell * 2 + 2]
                .copy_from_slice(&((entries[cell] & 0xf000) | id as u16).to_le_bytes());
        }
        reports.push(serde_json::json!({"index":host.index,"protected_tiles":protected.len(),"new_tiles":used}));
    }
    Ok((
        target,
        serde_json::json!({"sheets":reports,"masks_sha256":crate::source::sha256(raw),"distribution_eligible":false}),
    ))
}
