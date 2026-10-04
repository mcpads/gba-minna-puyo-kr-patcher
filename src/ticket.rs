//! 전체 한글 생성 티켓 후보를 별도 BG15 소스로 삽입한다.
use crate::{
    indexed_graphics::{read, text},
    source::sha256,
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::Path,
};
const REGION: usize = 0x930000;
const TILE_BYTES: usize = 0x4000;
const MAP: usize = REGION + TILE_BYTES;
const END: usize = MAP + 1280;
const SOURCE_TILES: usize = 0x84788;
const SOURCE_MAP: usize = 0x88788;

pub fn apply(source: &[u8], target: &mut [u8]) -> Result<serde_json::Value> {
    crate::source::Profile::supported()?.verify(source)?;
    let root = Path::new("assets/graphics/ticket-localized");
    let raw = fs::read(root.join("manifest.json"))?;
    let m: serde_json::Value = serde_json::from_slice(&raw)?;
    ensure!(
        m["policy"] == "asset_preparation_only"
            && m["status"] == "needs_review"
            && m["distribution_eligible"] == false
            && m["source"]["sha256"] == sha256(source)
            && m["palette_sha256"] == sha256(&source[0x2be78c..0x2be98c]),
        "티켓 후보 정책/원본 결속 오류"
    );
    // out/의 원본 PNG 경로는 생성 계보일 뿐 빌드 의존성이 아니다.
    let mut managed_inputs = BTreeSet::new();
    for input in m["inputs"].as_array().context("티켓 입력 목록 누락")? {
        let path = text(input, "path")?;
        if path.starts_with("assets/") {
            ensure!(
                sha256(&fs::read(path)?) == text(input, "sha256")?,
                "티켓 입력 변경: {path}"
            );
            managed_inputs.insert(path);
        }
    }
    ensure!(
        managed_inputs.contains("assets/translations/ticket-draft.json")
            && managed_inputs.contains("assets/graphics/ticket-localized/complete.png"),
        "티켓 필수 입력 결속 누락"
    );
    let words: serde_json::Value =
        serde_json::from_slice(&fs::read("assets/translations/ticket-draft.json")?)?;
    ensure!(
        words["wording"] == m["wording"]
            && words["story_draft_sha256"]
                == sha256(&fs::read("assets/translations/story-draft.json")?),
        "티켓 문안/대사 결속 오류"
    );
    ensure!(
        END <= target.len() && target[REGION..END].iter().all(|&v| v == 255),
        "티켓 확장 범위 충돌"
    );
    let pixels = read(
        &root.join(text(&m, "image")?),
        text(&m, "image_sha256")?,
        text(&m, "pixel_sha256")?,
        [256, 160],
        &source[0x2be78c..0x2be98c],
    )?;
    let mut original = vec![0; 40960];
    for y in 0..160 {
        for x in 0..256 {
            let at = SOURCE_MAP + ((y / 8) * 32 + x / 8) * 2;
            let entry = u16::from_le_bytes([source[at], source[at + 1]]);
            ensure!(
                entry & 1023 < 256 && entry & 0xf000 == 0,
                "티켓 원본 맵 범위 변경"
            );
            let px = if entry & 0x400 != 0 { 7 - x % 8 } else { x % 8 };
            let py = if entry & 0x800 != 0 { 7 - y % 8 } else { y % 8 };
            original[y * 256 + x] =
                source[SOURCE_TILES + usize::from(entry & 1023) * 64 + py * 8 + px];
        }
    }
    let mut exterior = BTreeSet::new();
    let mut queue: VecDeque<(usize, usize)> = (0..240)
        .map(|x| (x, 0))
        .chain((0..96).flat_map(|y| [(0, y), (239, y)]))
        .collect();
    while let Some((x, y)) = queue.pop_front() {
        if x >= 240 || y >= 96 || exterior.contains(&(x, y)) || original[y * 256 + x] != 249 {
            continue;
        }
        exterior.insert((x, y));
        if x > 0 {
            queue.push_back((x - 1, y));
        }
        if y > 0 {
            queue.push_back((x, y - 1));
        }
        queue.extend([(x + 1, y), (x, y + 1)]);
    }
    let mut protected = 0;
    for y in 0..160 {
        for x in 0..256 {
            if x >= 240 || y >= 96 || exterior.contains(&(x, y)) {
                ensure!(
                    pixels[y * 256 + x] == original[y * 256 + x],
                    "티켓 바깥 원화 변경"
                );
                protected += 1;
            }
        }
    }
    ensure!(m["protected_pixels"] == protected, "티켓 보호 픽셀 수 변경");
    let mut tiles = Vec::new();
    let mut entries = Vec::new();
    let mut lookup = BTreeMap::new();
    for cy in 0..20 {
        for cx in 0..32 {
            let block = (0..8)
                .flat_map(|y| {
                    pixels[(cy * 8 + y) * 256 + cx * 8..(cy * 8 + y) * 256 + cx * 8 + 8]
                        .iter()
                        .copied()
                })
                .collect::<Vec<_>>();
            let id = if let Some(&id) = lookup.get(&block) {
                id
            } else {
                let id = (tiles.len() / 64) as u16;
                ensure!(id < 256, "티켓 타일 용량 초과");
                tiles.extend_from_slice(&block);
                lookup.insert(block, id);
                id
            };
            entries.push(id);
        }
    }
    let count = tiles.len() / 64;
    tiles.resize(TILE_BYTES, 0);
    for y in 0..160 {
        for x in 0..256 {
            ensure!(
                tiles[usize::from(entries[(y / 8) * 32 + x / 8]) * 64 + (y % 8) * 8 + x % 8]
                    == pixels[y * 256 + x],
                "티켓 픽셀 재구성 불일치"
            );
        }
    }
    for (at, expected, value) in [
        (0x5652f0, 0x08084788u32, 0x08000000 + REGION as u32),
        (0x565340, 0x08088788u32, 0x08000000 + MAP as u32),
    ] {
        ensure!(
            source[at..at + 4] == expected.to_le_bytes()
                && target[at..at + 4] == expected.to_le_bytes(),
            "티켓 포인터 원본/작성 충돌"
        );
        target[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    target[REGION..MAP].copy_from_slice(&tiles);
    for (i, e) in entries.iter().enumerate() {
        target[MAP + i * 2..MAP + i * 2 + 2].copy_from_slice(&e.to_le_bytes());
    }
    for (start, len) in [
        (SOURCE_TILES, TILE_BYTES + 1280),
        (0x2be78c, 512),
        (0x565300, 4),
        (0x565350, 4),
        (0x98788, 0x4500),
        (0x23ce4, 80),
    ] {
        ensure!(
            source[start..start + len] == target[start..start + len],
            "티켓 영어/공통 소스 보호 위반"
        );
    }
    Ok(
        serde_json::json!({"manifest_sha256":crate::source::sha256(&raw),"region_start":REGION,"region_end":END,"tiles":REGION,"map":MAP,"unique_tiles":count,"capacity_tiles":256,"protected_pixels":protected,"composition":"whole_imagegen_localization","runtime":"not_run","distribution_eligible":false}),
    )
}
