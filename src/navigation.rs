//! 일본어 메뉴·선택 제목의 네이티브 글자 마스크와 원본 색인 역할을 합성한다.
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
    tiles: usize,
    tile_bytes: usize,
    map: usize,
    width: usize,
    height: usize,
    map_entry_bytes: usize,
    regions: Vec<Region>,
}
#[derive(Deserialize)]
struct Region {
    id: String,
    text: String,
    #[serde(rename = "box")]
    bounds: [usize; 4],
    review: [usize; 4],
    roles: Roles,
    rows: Vec<String>,
}
/// 마스크 기호 1 외곽선, 2 몸통(행별 색인), 3 획 윗면, 4 획 아랫면의 원본 색인.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Roles {
    edge: u8,
    highlight: Option<u8>,
    shade: Option<u8>,
    body_by_row: Vec<u8>,
}
fn entries(source: &[u8], host: &Host) -> Vec<usize> {
    (0..host.width * host.height)
        .map(|i| {
            let at = host.map + i * host.map_entry_bytes;
            if host.map_entry_bytes == 1 {
                source[at] as usize
            } else {
                u16::from_le_bytes([source[at], source[at + 1]]) as usize
            }
        })
        .collect()
}
fn pixels(source: &[u8], host: &Host) -> Result<Vec<u8>> {
    let width = host.width * 8;
    let mut result = vec![0; width * host.height * 8];
    for (cell, entry) in entries(source, host).into_iter().enumerate() {
        let id = if host.map_entry_bytes == 1 {
            entry
        } else {
            entry & 1023
        };
        ensure!(id * 64 < host.tile_bytes, "메뉴 타일 경계 밖 참조");
        for y in 0..8 {
            for x in 0..8 {
                let sx = if host.map_entry_bytes == 2 && entry & 1024 != 0 {
                    7 - x
                } else {
                    x
                };
                let sy = if host.map_entry_bytes == 2 && entry & 2048 != 0 {
                    7 - y
                } else {
                    y
                };
                result[(cell / host.width * 8 + y) * width + cell % host.width * 8 + x] =
                    source[host.tiles + id * 64 + sy * 8 + sx];
            }
        }
    }
    Ok(result)
}

/// 마스크 크기·역할·외곽선 형식을 검사하고 (너비, 높이)를 돌려준다.
/// 글자(2·3·4)의 8방향 이웃 중 글자가 아닌 칸이 정확히 외곽선(1)이어야 하며, 원문 상자를 넘을 수 없다.
fn check_region_shape(region: &Region) -> Result<(usize, usize)> {
    let [_, _, w, h] = region.bounds;
    let mh = region.rows.len();
    let mw = region.rows.first().context("빈 메뉴 마스크")?.len();
    ensure!(
        mw > 0 && mw <= w && mh <= h && region.rows.iter().all(|r| r.len() == mw),
        "메뉴 글자 잘림: {}",
        region.id
    );
    let roles = &region.roles;
    ensure!(roles.body_by_row.len() == mh, "메뉴 몸통 행 색인 수 오류");
    ensure!(
        roles.highlight.is_some() == roles.shade.is_some()
            && (roles.highlight.is_none() || mh == h),
        "입체 제목 역할/높이 오류"
    );
    let code = |x: isize, y: isize| -> u8 {
        if x < 0 || y < 0 || x as usize >= mw || y as usize >= mh {
            b'0'
        } else {
            region.rows[y as usize].as_bytes()[x as usize]
        }
    };
    for y in 0..mh as isize {
        for x in 0..mw as isize {
            let c = code(x, y);
            ensure!(b"01234".contains(&c), "메뉴 마스크 값 오류");
            ensure!(
                roles.highlight.is_some() || !b"34".contains(&c),
                "평면 문구에 입체 역할"
            );
            let near_ink = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                .any(|(dx, dy)| b"234".contains(&code(x + dx, y + dy)));
            if !b"234".contains(&c) {
                ensure!(
                    (c == b'1') == near_ink,
                    "메뉴 외곽선 형식 오류: {}",
                    region.id
                );
            }
        }
    }
    Ok((mw, mh))
}

pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    crate::source::Profile::supported()?.verify(source)?;
    let input: Input = serde_json::from_str(&crate::managed_input::read_string(
        "assets/fonts/navigation-masks.json",
    )?)?;
    let draft_raw = &crate::managed_input::read("assets/translations/ui-draft.json")?;
    let draft: serde_json::Value = serde_json::from_slice(draft_raw)?;
    ensure!(
        draft["policy"] == "development_selected_ui",
        "메뉴 개발 입력 정책 불일치"
    );
    ensure!(
        input.policy == "development" && input.draft_sha256 == crate::source::sha256(draft_raw),
        "메뉴 문안 결속 오류"
    );
    let bounds = [
        (0x3b488, 0x2800, 0x3dc88, 32, 32, 1),
        (0x195368, 0xc00, 0x195f68, 30, 20, 2),
        (0x1970c8, 0x1000, 0x1980c8, 30, 20, 2),
        (0x199228, 0xc00, 0x199e28, 30, 20, 2),
        (0x19af88, 0x800, 0x19b788, 30, 20, 2),
        (0x19c8e8, 0x800, 0x19d0e8, 30, 20, 2),
        (0x19e248, 0x800, 0x19ea48, 30, 20, 2),
    ];
    ensure!(input.hosts.len() == bounds.len(), "메뉴 호스트 수 변경");
    let mut target = source.to_vec();
    let mut allowed = BTreeSet::new();
    let mut reports = Vec::new();
    for (host, spec) in input.hosts.iter().zip(bounds) {
        ensure!(
            (
                host.tiles,
                host.tile_bytes,
                host.map,
                host.width,
                host.height,
                host.map_entry_bytes
            ) == spec,
            "메뉴 쓰기 경계 변경"
        );
        let original = pixels(source, host)?;
        let mut image = original.clone();
        let stride = host.width * 8;
        let mut editable = BTreeSet::new();
        for region in &host.regions {
            let entry = draft["entries"]
                .as_array()
                .context("문안 목록 없음")?
                .iter()
                .find(|e| e["id"] == region.id)
                .context("메뉴 문안 ID 없음")?;
            ensure!(
                entry["korean_text"] == region.text,
                "메뉴 마스크와 문안 불일치"
            );
            let [x, y, w, h] = region.bounds;
            let [rx, ry, rw, rh] = region.review;
            ensure!(
                rx + rw <= stride
                    && ry + rh <= host.height * 8
                    && [rx, ry, rw, rh].iter().all(|v| v % 8 == 0),
                "메뉴 편집 셀 경계 오류"
            );
            ensure!(
                x >= rx && y >= ry && x + w <= rx + rw && y + h <= ry + rh,
                "메뉴 원문 외곽 오류"
            );
            for cy in ry / 8..(ry + rh) / 8 {
                for cx in rx / 8..(rx + rw) / 8 {
                    ensure!(editable.insert(cy * host.width + cx), "메뉴 문구 영역 중복");
                }
            }
            for py in ry..ry + rh {
                for px in rx..rx + rw {
                    ensure!(
                        (px >= x && px < x + w && py >= y && py < y + h)
                            || image[py * stride + px] == 0,
                        "원문 외곽 밖 비문자 픽셀"
                    );
                    image[py * stride + px] = 0;
                }
            }
            let (mw, mh) = check_region_shape(region)?;
            let roles = &region.roles;
            // 쓰는 색인은 원문 외곽 안 원본 글자 픽셀에 실제로 있는 색인이어야 한다.
            let present: BTreeSet<u8> = (y..y + h)
                .flat_map(|py| (x..x + w).map(move |px| (px, py)))
                .map(|(px, py)| original[py * stride + px])
                .filter(|&v| v != 0)
                .collect();
            let dx = x + (w - mw) / 2;
            let dy = y + (h - mh) / 2;
            for (py, row) in region.rows.iter().enumerate() {
                for (px, code) in row.bytes().enumerate() {
                    let value = match code {
                        b'0' => 0,
                        b'1' => roles.edge,
                        b'2' => roles.body_by_row[py],
                        b'3' => roles.highlight.context("메뉴 윗면 색 역할 없음")?,
                        b'4' => roles.shade.context("메뉴 아랫면 색 역할 없음")?,
                        _ => anyhow::bail!("메뉴 마스크 값 오류"),
                    };
                    ensure!(
                        value == 0 || present.contains(&value),
                        "원문에 없는 메뉴 색인: {} {value}",
                        region.id
                    );
                    image[(dy + py) * stride + dx + px] = value;
                }
            }
        }
        let source_map = entries(source, host);
        let mut protected = BTreeSet::from([0]);
        for (cell, &entry) in source_map.iter().enumerate() {
            if !editable.contains(&cell) {
                protected.insert(if host.map_entry_bytes == 1 {
                    entry
                } else {
                    entry & 1023
                });
            }
        }
        let mut lookup = BTreeMap::new();
        for &id in &protected {
            lookup.insert(
                source[host.tiles + id * 64..host.tiles + (id + 1) * 64].to_vec(),
                id,
            );
        }
        let mut free = (0..host.tile_bytes / 64).filter(|id| !protected.contains(id));
        for &cell in &editable {
            let mut tile = Vec::with_capacity(64);
            for y in 0..8 {
                let start = (cell / host.width * 8 + y) * stride + cell % host.width * 8;
                tile.extend_from_slice(&image[start..start + 8]);
            }
            let id = if let Some(&id) = lookup.get(&tile) {
                id
            } else {
                let id = free.next().context("메뉴 고유 타일 용량 초과")?;
                target[host.tiles + id * 64..host.tiles + (id + 1) * 64].copy_from_slice(&tile);
                lookup.insert(tile, id);
                id
            };
            let at = host.map + cell * host.map_entry_bytes;
            if host.map_entry_bytes == 1 {
                target[at] = u8::try_from(id)?;
            } else {
                let value = (source_map[cell] & 0xf000) | id;
                target[at..at + 2].copy_from_slice(&(value as u16).to_le_bytes());
            }
        }
        for (start, len) in [
            (host.tiles, host.tile_bytes),
            (host.map, host.width * host.height * host.map_entry_bytes),
        ] {
            for at in start..start + len {
                ensure!(allowed.insert(at), "메뉴 작성자 중복");
            }
        }
        ensure!(
            pixels(&target, host)? == image,
            "메뉴 최종 픽셀 왕복 불일치"
        );
        reports.push(serde_json::json!({"id":host.id,"phrases":host.regions.len(),"used_tiles":lookup.len(),"capacity_tiles":host.tile_bytes/64,"pixel_roundtrip":"pass"}));
    }
    let mut changed = 0;
    for (at, (&a, &b)) in source.iter().zip(&target).enumerate() {
        if a != b {
            ensure!(allowed.contains(&at), "메뉴 보호 범위 침범");
            changed += 1;
        }
    }
    Ok((
        target,
        serde_json::json!({"hosts":reports,"changed_bytes":changed,"final_write_audit":"pass","policy":"development","distribution_eligible":false}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn regions() -> Vec<Region> {
        let input: Input = serde_json::from_str(
            &crate::managed_input::read_string("assets/fonts/navigation-masks.json").unwrap(),
        )
        .unwrap();
        input.hosts.into_iter().flat_map(|h| h.regions).collect()
    }
    #[test]
    #[ignore = "requires assets/fonts/navigation-masks.json"]
    fn managed_masks_fit_boxes_with_closed_outline() {
        let all = regions();
        assert_eq!(all.len(), 12);
        for region in &all {
            check_region_shape(region).unwrap();
        }
        // 제목 7곳은 입체 역할·높이 16.
        assert_eq!(
            all.iter().filter(|r| r.roles.highlight.is_some()).count(),
            7
        );
    }
    #[test]
    #[ignore = "requires assets/fonts/navigation-masks.json"]
    fn rejects_overwide_title_and_broken_outline() {
        let mut wide = regions().into_iter().last().unwrap();
        assert_eq!(wide.rows[0].len(), wide.bounds[2]);
        for row in &mut wide.rows {
            row.insert(0, '0');
        }
        assert!(check_region_shape(&wide).is_err());
        let mut gap = regions().into_iter().last().unwrap();
        let at = gap.rows[1].find('1').unwrap();
        gap.rows[1].replace_range(at..at + 1, "0");
        assert!(check_region_shape(&gap).is_err());
    }
}
