use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
struct Input {
    policy: String,
    draft_sha256: String,
    ui_draft_sha256: String,
    fonts_sha256: BTreeMap<String, String>,
    hosts: Vec<Host>,
}
#[derive(Deserialize)]
struct Host {
    id: String,
    tiles: usize,
    tile_bytes: usize,
    map: usize,
    palette: usize,
    source_tiles_sha256: String,
    source_map_sha256: String,
    regions: Vec<Region>,
}
#[derive(Deserialize)]
struct Region {
    id: String,
    phrase: String,
    text: String,
    #[serde(rename = "box")]
    bounds: [usize; 4],
    edge: u8,
    fill: u8,
    rows: Vec<String>,
}
fn pixel(source: &[u8], host: &Host, x: usize, y: usize) -> u8 {
    let at = host.map + ((y / 8) * 30 + x / 8) * 2;
    let entry = u16::from_le_bytes([source[at], source[at + 1]]);
    let px = if entry & 0x400 != 0 { 7 - x % 8 } else { x % 8 };
    let py = if entry & 0x800 != 0 { 7 - y % 8 } else { y % 8 };
    source[host.tiles + usize::from(entry & 1023) * 64 + py * 8 + px]
}
fn gba_pixel(source: &[u8], host: &Host, x: usize, y: usize) -> bool {
    (x.saturating_sub(1)..=x + 1).any(|xx| {
        (y.saturating_sub(1)..=y + 1).any(|yy| {
            (8..26).contains(&xx) && (120..136).contains(&yy) && pixel(source, host, xx, yy) == 48
        })
    })
}
pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    crate::source::Profile::supported()?.verify(source)?;
    let raw = &crate::managed_input::read("assets/fonts/link-connection-masks.json")?;
    let input: Input = serde_json::from_slice(raw)?;
    // Galmuri11 줄의 손질 글리프 대체표(OFL 파생) 통신 연결 글자 결속.
    crate::glyph_overrides::verify(&serde_json::from_slice(raw)?, "link-connection")?;
    let draft_raw = &crate::managed_input::read("assets/translations/link-connection.json")?;
    let draft: serde_json::Value = serde_json::from_slice(draft_raw)?;
    let ui_raw = &crate::managed_input::read("assets/translations/ui-draft.json")?;
    let ui: serde_json::Value = serde_json::from_slice(ui_raw)?;
    ensure!(
        input.policy == "development_link_connection"
            && draft["policy"] == input.policy
            && input.draft_sha256 == crate::source::sha256(draft_raw)
            && input.ui_draft_sha256 == crate::source::sha256(ui_raw)
            // 제목 Neo둥근모16, 카트리지 방식·아래 안내 Galmuri11 12px만 쓴다.
            && input.fonts_sha256
                == BTreeMap::from([
                    (
                        "Galmuri11.ttf".to_owned(),
                        "2c709890595668f7bdb6df408420fda957dde0288e95b31a1cc17a2ab98b4b4f"
                            .to_owned()
                    ),
                    (
                        "neodgm.ttf".to_owned(),
                        "d61b60eccb731f8ca9c7da582e4a05a94db66b570471809950aa9a7261b941d6"
                            .to_owned()
                    ),
                ])
            && draft["source_sha256"]
                == crate::source::sha256(&crate::managed_input::read("config/link-connection-source-text.json")?),
        "연결 안내 입력 결속 오류"
    );
    let expected = [
        ("link-ja-two-single", 0xcfbf0, 0x2000, 0xd1bf0, 0xcfb10),
        ("link-ja-two-multi", 0xd2180, 0x3000, 0xd5180, 0xd20a0),
        ("link-ja-four-single", 0xd5710, 0x2000, 0xd7710, 0xd5630),
        ("link-ja-four-multi", 0xd7ca0, 0x3000, 0xdaca0, 0xd7bc0),
    ];
    ensure!(input.hosts.len() == 4, "연결 안내 분모 변경");
    let mut target = source.to_vec();
    let mut reports = Vec::new();
    for (host, (name, t, size, map, palette)) in input.hosts.iter().zip(expected) {
        ensure!(
            host.id == name
                && (host.tiles, host.tile_bytes, host.map, host.palette) == (t, size, map, palette),
            "연결 안내 공급 변경"
        );
        ensure!(
            crate::source::sha256(&source[t..t + size]) == host.source_tiles_sha256
                && crate::source::sha256(&source[map..map + 1200]) == host.source_map_sha256,
            "연결 원본 해시 오류"
        );
        let entries: Vec<_> = source[map..map + 1200]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect();
        ensure!(
            entries.iter().all(|v| usize::from(v & 1023) < size / 64),
            "연결 타일 범위 오류"
        );
        let multi = name.ends_with("multi");
        ensure!(
            host.regions.len() == if multi { 4 } else { 3 },
            "연결 문구 수 오류"
        );
        let mut seen = BTreeSet::new();
        let mut blocks = BTreeMap::new();
        for region in &host.regions {
            ensure!(seen.insert(&region.id), "연결 문구 중복");
            let (bounds, phrase, edge, fill) = match region.id.as_str() {
                "title" => (
                    [0, 0, 240, 24],
                    if name.contains("-two-") {
                        "navigation.two-player"
                    } else {
                        "navigation.four-player"
                    },
                    111,
                    102,
                ),
                "cartridge" => (
                    [0, 24, 240, 16],
                    if multi { "multi" } else { "single" },
                    39,
                    48,
                ),
                "connect" => ([0, 120, 240, 16], "connect", 39, 48),
                "matching" if multi => ([0, 136, 240, 16], "matching", 39, 48),
                _ => anyhow::bail!("미허용 연결 문구"),
            };
            ensure!(
                region.bounds == bounds
                    && region.phrase == phrase
                    && (region.edge, region.fill) == (edge, fill),
                "연결 문구 배치 변경"
            );
            let words = if region.id == "title" { &ui } else { &draft };
            let entry = words["entries"]
                .as_array()
                .context("문안 없음")?
                .iter()
                .find(|e| e["id"] == phrase)
                .context("문안 ID 없음")?;
            ensure!(entry["korean_text"] == region.text, "연결 문안 불일치");
            let [x, y, w, h] = bounds;
            ensure!(
                region.rows.len() == h && region.rows.iter().all(|r| r.len() == w),
                "연결 마스크 크기 오류"
            );
            for cy in 0..h / 8 {
                for cx in 0..w / 8 {
                    let mut tile = Vec::with_capacity(64);
                    for py in 0..8 {
                        for px in 0..8 {
                            let (xx, yy) = (x + cx * 8 + px, y + cy * 8 + py);
                            let code = region.rows[cy * 8 + py].as_bytes()[cx * 8 + px];
                            let preserve =
                                region.id == "connect" && gba_pixel(source, host, xx, yy);
                            ensure!((code == b'p') == preserve, "GBA 보존 마스크 변경");
                            tile.push(match code {
                                b'0' => 0,
                                b'1' => edge,
                                b'2' => fill,
                                b'p' => pixel(source, host, xx, yy),
                                _ => anyhow::bail!("연결 마스크 코드 오류"),
                            });
                        }
                    }
                    ensure!(
                        blocks
                            .insert((y / 8 + cy) * 30 + x / 8 + cx, tile)
                            .is_none(),
                        "연결 편집 겹침"
                    );
                }
            }
        }
        let mut protected: BTreeSet<_> = entries
            .iter()
            .enumerate()
            .filter(|(cell, _)| !blocks.contains_key(cell))
            .map(|(_, e)| usize::from(e & 1023))
            .collect();
        protected.extend(0..5);
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
                let id = free.next().context("연결 안내 타일 용량 초과")?;
                target[t + id * 64..t + (id + 1) * 64].copy_from_slice(&tile);
                lookup.insert(tile, id);
                added += 1;
                id
            };
            target[map + cell * 2..map + cell * 2 + 2]
                .copy_from_slice(&((entries[cell] & 0xf000) | id as u16).to_le_bytes());
        }
        ensure!(
            target[palette..palette + 512] == source[palette..palette + 512],
            "공유 팔레트 변경"
        );
        reports.push(serde_json::json!({"id":name,"protected_tiles":protected.len(),"new_tiles":added,"capacity":size/64}));
    }
    Ok((
        target,
        serde_json::json!({"hosts":reports,"masks_sha256":crate::source::sha256(raw),"distribution_eligible":false}),
    ))
}
