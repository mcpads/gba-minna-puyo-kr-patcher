use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
struct Input {
    policy: String,
    draft_sha256: String,
    font_sha256: String,
    license: String,
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
pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    crate::source::Profile::supported()?.verify(source)?;
    let raw = &crate::managed_input::read("assets/fonts/link-status-masks.json")?;
    let input: Input = serde_json::from_slice(raw)?;
    let draft_raw = &crate::managed_input::read("assets/translations/link-status.json")?;
    let draft: serde_json::Value = serde_json::from_slice(draft_raw)?;
    ensure!(
        input.policy == "development_selected_link_status"
            && draft["policy"] == input.policy
            && input.draft_sha256 == crate::source::sha256(draft_raw)
            // 원본 転送中…의 2px 세로획·높이 15px에 맞춘 Neo둥근모16(OFL) 조판만 허용한다.
            && input.font_sha256
                == "d61b60eccb731f8ca9c7da582e4a05a94db66b570471809950aa9a7261b941d6"
            && input.license == "licenses/fonts/neodgm-v1.600-OFL.txt"
            && draft["source_sha256"]
                == crate::source::sha256(&crate::managed_input::read("config/link-status-source-text.json")?)
            && draft["entries"]
                .as_array()
                .is_some_and(|entries| entries.len() == 6),
        "통신 상태 입력 결속 오류"
    );
    let (tiles, map) = (0xcac88, 0xcd088);
    ensure!(
        input.regions.len() == 6
            && crate::source::sha256(&source[tiles..map]) == input.source_tiles_sha256
            && crate::source::sha256(&source[map..map + 578]) == input.source_map_sha256,
        "통신 상태 원본/분모 오류"
    );
    let entries: Vec<_> = source[map..map + 578]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect();
    ensure!(
        entries.iter().all(|e| e & 1023 < 144),
        "통신 상태 타일 범위 오류"
    );
    let mut blocks = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for reg in input.regions {
        ensure!(seen.insert(reg.id.clone()), "통신 상태 중복");
        let bounds = match reg.id.as_str() {
            "checking" => [32, 24, 104, 16],
            "none" => [32, 40, 104, 16],
            "error" => [32, 56, 104, 16],
            "sending" => [0, 88, 120, 16],
            "starting" => [0, 104, 120, 16],
            "mode-error" => [0, 120, 120, 16],
            _ => anyhow::bail!("보존 문구 변경 시도"),
        };
        ensure!(reg.bounds == bounds, "통신 상태 편집 경계 변경");
        let entry = draft["entries"]
            .as_array()
            .context("통신 문안 없음")?
            .iter()
            .find(|e| e["id"] == reg.id)
            .context("문안 ID 없음")?;
        ensure!(entry["korean_text"] == reg.text, "통신 문안 불일치");
        let [x, y, w, h] = bounds;
        ensure!(
            reg.rows.len() == h
                && reg
                    .rows
                    .iter()
                    .all(|r| r.len() == w && r.bytes().all(|c| matches!(c, b'0'..=b'2'))),
            "통신 마스크 오류"
        );
        for cy in 0..h / 8 {
            for cx in 0..w / 8 {
                let mut tile = Vec::with_capacity(64);
                for py in 0..8 {
                    for px in 0..8 {
                        tile.push(match reg.rows[cy * 8 + py].as_bytes()[cx * 8 + px] {
                            b'0' => 0,
                            b'1' => 39,
                            _ => 48,
                        });
                    }
                }
                ensure!(
                    blocks
                        .insert((y / 8 + cy) * 17 + x / 8 + cx, tile)
                        .is_none(),
                    "통신 편집 중복"
                );
            }
        }
    }
    let protected: BTreeSet<_> = entries
        .iter()
        .enumerate()
        .filter(|(cell, _)| !blocks.contains_key(cell))
        .map(|(_, e)| usize::from(e & 1023))
        .collect();
    ensure!(
        protected.len() == 53 && [0, 5, 9].iter().all(|i| protected.contains(i)),
        "영문 공유 타일 보호 오류"
    );
    let mut target = source.to_vec();
    let mut lookup = BTreeMap::new();
    for &id in &protected {
        lookup.insert(source[tiles + id * 64..tiles + (id + 1) * 64].to_vec(), id);
    }
    let mut free = (0..144).filter(|id| !protected.contains(id));
    let mut used = 0;
    for (cell, tile) in blocks {
        let id = if let Some(&id) = lookup.get(&tile) {
            id
        } else {
            let id = free.next().context("통신 상태 타일 용량 초과")?;
            target[tiles + id * 64..tiles + (id + 1) * 64].copy_from_slice(&tile);
            lookup.insert(tile, id);
            used += 1;
            id
        };
        target[map + cell * 2..map + cell * 2 + 2]
            .copy_from_slice(&((entries[cell] & 0xf000) | id as u16).to_le_bytes());
    }
    Ok((
        target,
        serde_json::json!({"phrases":6,"protected_tiles":protected.len(),"new_tiles":used,"masks_sha256":crate::source::sha256(raw),"distribution_eligible":false}),
    ))
}
