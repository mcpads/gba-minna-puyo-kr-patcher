//! 시작 언어 화면의 제목·한국어 선택지·일본어 설명과 양언어 옵션의 언어명을 쓰는 개발 빌드.
//! 저장된 글자 마스크에서 직접 타일을 만들고, 제목의 보존 영문은 원본 픽셀을 옮겨 붙인다.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mask {
    id: String,
    unit_id: String,
    style: String,
    status: String,
    font_sha256: String,
    size_px: usize,
    baseline: usize,
    license: String,
    lines: Vec<MaskLine>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaskLine {
    text: String,
    rows: Vec<String>,
}

impl Mask {
    fn text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

enum Fill {
    Flat(u8),
    /// 영역 안 y에 따라 98..=111 단계색을 쓰는 옵션 값 표현.
    Ramp(u8),
}

enum Layout {
    Center,
    Right,
    /// 각 줄 잉크 상자의 화면 좌상단 좌표.
    Lines(&'static [[usize; 2]]),
    /// 한국어 마스크 + 원본 `preserved_from` 열부터의 픽셀(공백 열 포함)을 하나로 보고
    /// 원본 제목 전체 잉크 상자의 가로 중앙에 다시 놓는다.
    CenteredHeading {
        preserved_from: usize,
        gap_columns: usize,
    },
}

struct Region {
    unit: &'static str,
    source_text: &'static str,
    preserved_suffix: &'static str,
    mask: &'static str,
    box_xywh: [usize; 4],
    layout: Layout,
    expected_dedicated: usize,
}

struct Surface {
    id: &'static str,
    tiles: usize,
    tile_bytes: usize,
    map: usize,
    map_width: usize,
    background: u8,
    edge: u8,
    fill: Fill,
    regions: Vec<Region>,
}

const MASKS_INPUT: &str = "assets/fonts/language-label-masks.json";
const DECISIONS_INPUT: &str = "config/korean-wording-decisions.json";
const MENU_SOURCE_INPUT: &str = "config/language-menu-source-text.json";
const MAP_ENTRIES: usize = 600;
const HEADING_LINES: &[[usize; 2]] = &[[17, 98], [17, 112]];

fn surfaces() -> Vec<Surface> {
    let option = |unit, mask| Region {
        unit,
        source_text: "日本語",
        preserved_suffix: "",
        mask,
        box_xywh: [0, 208, 64, 16],
        layout: Layout::Right,
        expected_dedicated: 10,
    };
    vec![
        Surface {
            id: "startup",
            tiles: 0x25e28,
            tile_bytes: 0x4400,
            map: 0x2a228,
            map_width: 30,
            background: 1,
            edge: 29,
            fill: Fill::Flat(18),
            // 순서가 전용 타일 배정 순서다. 기존 한국어 선택지를 먼저 둬 그 바이트를 유지한다.
            regions: vec![
                Region {
                    unit: "language-menu.japanese-option",
                    source_text: "日本語",
                    preserved_suffix: "",
                    mask: "startup-japanese-option",
                    box_xywh: [88, 40, 64, 24],
                    layout: Layout::Center,
                    expected_dedicated: 24,
                },
                Region {
                    unit: "language-menu.heading",
                    source_text: "ことばの設定 /LANGUAGE",
                    preserved_suffix: " /LANGUAGE",
                    mask: "startup-heading",
                    box_xywh: [32, 16, 176, 16],
                    layout: Layout::CenteredHeading {
                        preserved_from: 130,
                        gap_columns: 4,
                    },
                    expected_dedicated: 44,
                },
                Region {
                    unit: "language-menu.japanese-help",
                    source_text: "ことばの設定は、おぷしょんで\nいつでも変更できるよ",
                    preserved_suffix: "",
                    mask: "startup-japanese-help",
                    box_xywh: [16, 96, 184, 32],
                    layout: Layout::Lines(HEADING_LINES),
                    expected_dedicated: 71,
                },
            ],
        },
        Surface {
            id: "options-ja",
            tiles: 0xf57b8,
            tile_bytes: 0x4c00,
            map: 0xfa3b8,
            map_width: 20,
            background: 0,
            edge: 97,
            fill: Fill::Ramp(98),
            regions: vec![option(
                "options.value.language.japanese",
                "options-japanese",
            )],
        },
        Surface {
            id: "options-en",
            tiles: 0xfe918,
            tile_bytes: 0x5000,
            map: 0x103918,
            map_width: 20,
            background: 0,
            edge: 97,
            fill: Fill::Ramp(98),
            regions: vec![option(
                "options.value.language.japanese",
                "options-japanese",
            )],
        },
    ]
}

/// 맵 반전을 반영한 화면 색인 픽셀. 행 우선, 폭 `map_width*8`.
fn render(rom: &[u8], s: &Surface) -> Vec<u8> {
    let width = s.map_width * 8;
    let mut pixels = vec![0; width * MAP_ENTRIES / s.map_width * 8];
    for cell in 0..MAP_ENTRIES {
        let v = u16::from_le_bytes([rom[s.map + cell * 2], rom[s.map + cell * 2 + 1]]);
        let tile = s.tiles + usize::from(v & 1023) * 64;
        let (hf, vf) = (v & 0x400 != 0, v & 0x800 != 0);
        for y in 0..8 {
            for x in 0..8 {
                let (sx, sy) = (if hf { 7 - x } else { x }, if vf { 7 - y } else { y });
                pixels[(cell / s.map_width * 8 + y) * width + cell % s.map_width * 8 + x] =
                    rom[tile + sy * 8 + sx];
            }
        }
    }
    pixels
}

fn check_decisions(regions: &[&Region], masks: &[Mask]) -> Result<()> {
    let decisions: serde_json::Value =
        serde_json::from_str(&crate::managed_input::read_string(DECISIONS_INPUT)?)?;
    let decisions = decisions.as_array().context("문안 결정 배열 필요")?;
    let menu: serde_json::Value =
        serde_json::from_str(&crate::managed_input::read_string(MENU_SOURCE_INPUT)?)?;
    for region in regions {
        let mask = masks
            .iter()
            .find(|m| m.id == region.mask)
            .context("마스크 누락")?;
        ensure!(mask.unit_id == region.unit, "마스크·문안 결정 ID 불일치");
        let rows: Vec<_> = decisions
            .iter()
            .filter(|row| row["unit_id"] == region.unit)
            .collect();
        ensure!(
            rows.len() == 1
                && rows[0]["source_text"] == region.source_text
                && rows[0]["korean_text"] == mask.text() + region.preserved_suffix,
            "언어 문구 문안 결정 불일치: {}",
            region.unit
        );
        if let Some(id) = region.unit.strip_prefix("language-menu.") {
            let source = menu["regions"]
                .as_array()
                .context("언어 화면 원문 목록 필요")?
                .iter()
                .find(|r| r["id"] == id)
                .context("언어 화면 원문 영역 누락")?;
            ensure!(
                source["text"] == region.source_text
                    && source["box"] == serde_json::json!(region.box_xywh),
                "언어 화면 원문·검토 상자 변경: {id}"
            );
        }
    }
    Ok(())
}

pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    crate::source::Profile::supported()?.verify(source)?;
    let masks_text = crate::managed_input::read_string(MASKS_INPUT)?;
    let masks: Vec<Mask> = serde_json::from_str(&masks_text)?;
    ensure!(masks.len() == 4, "언어 문구 마스크 분모 변경");
    for mask in &masks {
        ensure!(
            mask.status == "development" && !mask.lines.is_empty(),
            "개발 마스크 상태 또는 줄 오류: {}",
            mask.id
        );
        let digits: &[u8] = match mask.style.as_str() {
            "outline" => b"012",
            "fill" => b"02",
            _ => anyhow::bail!("알 수 없는 마스크 조판: {}", mask.id),
        };
        for line in &mask.lines {
            ensure!(
                !line.text.is_empty() && !line.rows.is_empty() && !line.rows[0].is_empty(),
                "빈 글자 마스크"
            );
            let width = line.rows[0].len();
            ensure!(
                line.rows
                    .iter()
                    .all(|r| r.len() == width && r.bytes().all(|b| digits.contains(&b))),
                "마스크 크기 또는 픽셀 값 오류: {}",
                mask.id
            );
        }
    }
    let surfaces = surfaces();
    let all_regions: Vec<&Region> = surfaces.iter().flat_map(|s| &s.regions).collect();
    check_decisions(&all_regions, &masks)?;
    let mut target = source.to_vec();
    let mut allowed = vec![false; source.len()];
    let mut reports = Vec::new();
    for surface in &surfaces {
        let width = surface.map_width * 8;
        let height = MAP_ENTRIES / surface.map_width * 8;
        let entries: Vec<u16> = source[surface.map..surface.map + MAP_ENTRIES * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        let tile_count = surface.tile_bytes / 64;
        ensure!(
            entries.iter().all(|v| usize::from(v & 1023) < tile_count),
            "타일 번호 범위 오류"
        );
        let region_cells: Vec<BTreeSet<usize>> = surface
            .regions
            .iter()
            .map(|r| {
                let [left, top, w, h] = r.box_xywh;
                ensure!(
                    left % 8 == 0 && top % 8 == 0 && w % 8 == 0 && h % 8 == 0,
                    "편집 상자가 셀 경계가 아님"
                );
                ensure!(left + w <= width && top + h <= height, "편집 상자 화면 밖");
                Ok((top / 8..(top + h) / 8)
                    .flat_map(|y| {
                        (left / 8..(left + w) / 8).map(move |x| y * surface.map_width + x)
                    })
                    .collect())
            })
            .collect::<Result<_>>()?;
        let editable: BTreeSet<usize> = region_cells.iter().flatten().copied().collect();
        ensure!(
            editable.len() == region_cells.iter().map(BTreeSet::len).sum::<usize>(),
            "편집 상자 중복"
        );
        let external: BTreeSet<usize> = entries
            .iter()
            .enumerate()
            .filter(|(i, _)| !editable.contains(i))
            .map(|(_, v)| usize::from(v & 1023))
            .collect();
        // 영역 순서대로 각 영역 전용 타일(오름차순)을 이어 붙인 공용 예산.
        let mut free_order = Vec::new();
        let mut available: BTreeSet<usize> = BTreeSet::new();
        let mut region_reports = Vec::new();
        for (region, cells) in surface.regions.iter().zip(&region_cells) {
            let own: BTreeSet<usize> = cells
                .iter()
                .map(|&i| usize::from(entries[i] & 1023))
                .filter(|i| !external.contains(i) && !available.contains(i))
                .collect();
            ensure!(
                own.len() == region.expected_dedicated,
                "전용 타일 분모 변경: {}",
                region.unit
            );
            available.extend(&own);
            free_order.extend(own);
        }
        for &tile in &available {
            for written in &mut allowed[surface.tiles + tile * 64..surface.tiles + (tile + 1) * 64]
            {
                ensure!(!*written, "타일 작성 범위 충돌");
                *written = true;
            }
        }
        for &cell in &editable {
            for written in &mut allowed[surface.map + cell * 2..surface.map + cell * 2 + 2] {
                ensure!(!*written, "맵 작성 범위 충돌");
                *written = true;
            }
        }
        // 편집 셀을 배경으로 비운 화면에 각 영역을 그린다.
        let original = render(source, surface);
        let mut canvas = original.clone();
        for &cell in &editable {
            for y in 0..8 {
                let row = (cell / surface.map_width * 8 + y) * width + cell % surface.map_width * 8;
                canvas[row..row + 8].fill(surface.background);
            }
        }
        let mut moved = Vec::new();
        for region in &surface.regions {
            let mask = masks
                .iter()
                .find(|m| m.id == region.mask)
                .context("마스크 누락")?;
            let [left, top, w, h] = region.box_xywh;
            let mut origins = Vec::new();
            match &region.layout {
                Layout::Center | Layout::Right | Layout::CenteredHeading { .. } => {
                    ensure!(mask.lines.len() == 1, "한 줄 마스크 필요: {}", mask.id)
                }
                Layout::Lines(lines) => {
                    ensure!(lines.len() == mask.lines.len(), "줄 위치 수 불일치")
                }
            }
            let (mw, mh) = (mask.lines[0].rows[0].len(), mask.lines[0].rows.len());
            match &region.layout {
                Layout::Center => origins.push([left + (w - mw) / 2, top + (h - mh) / 2]),
                Layout::Right => origins.push([left + w - mw, top + (h - mh) / 2]),
                Layout::Lines(lines) => origins.extend(lines.iter().copied()),
                &Layout::CenteredHeading {
                    preserved_from,
                    gap_columns,
                } => {
                    let ink = |x: usize| {
                        (top..top + h).any(|y| original[y * width + x] != surface.background)
                    };
                    let inked: Vec<usize> = (0..width).filter(|&x| ink(x)).collect();
                    let (first, last) = (
                        *inked.first().context("원본 제목 없음")?,
                        *inked.last().unwrap(),
                    );
                    // 칸 안 픽셀열 경계: 일본어 마지막 열 바로 뒤에서 자르고 원본 공백 열을 그대로 옮긴다.
                    ensure!(
                        ink(preserved_from - 1)
                            && (preserved_from..preserved_from + gap_columns).all(|x| !ink(x))
                            && ink(preserved_from + gap_columns),
                        "제목 일본어/보존 영문 경계 변경"
                    );
                    ensure!(
                        first >= left
                            && last < left + w
                            && (top > 0 && top + h < height)
                            && (0..width).all(|x| {
                                original[(top - 1) * width + x] == surface.background
                                    && original[(top + h) * width + x] == surface.background
                            }),
                        "원본 제목 잉크가 편집 상자 밖"
                    );
                    let total = mw + last + 1 - preserved_from;
                    let new_left = (first + last + 1 - total) / 2;
                    ensure!(
                        (2 * new_left + total - 1).abs_diff(first + last) <= 2,
                        "제목 가운데 정렬 오차 초과"
                    );
                    ensure!(
                        new_left >= left && new_left + total <= left + w,
                        "제목 편집 상자 초과"
                    );
                    origins.push([new_left, top + (h - mh).div_ceil(2)]);
                    let destination = new_left + mw;
                    for y in top..top + h {
                        for x in preserved_from..=last {
                            canvas[y * width + x - preserved_from + destination] =
                                original[y * width + x];
                        }
                    }
                    moved.push((preserved_from, last, destination, top, h));
                    region_reports.push(serde_json::json!({"unit":region.unit,
                        "source_ink_columns":[first,last],"target_ink_columns":[new_left,new_left+total-1],
                        "preserved_source_columns":[preserved_from,last],"preserved_target_left":destination,
                        "preserved_gap_columns":gap_columns}));
                }
            }
            for (line, &[ox, oy]) in mask.lines.iter().zip(&origins) {
                let (lw, lh) = (line.rows[0].len(), line.rows.len());
                ensure!(
                    ox >= left && oy >= top && ox + lw <= left + w && oy + lh <= top + h,
                    "언어 문구 영역 초과: {}",
                    mask.id
                );
                for (yy, row) in line.rows.iter().enumerate() {
                    for (xx, b) in row.bytes().enumerate() {
                        let box_y = oy + yy - top;
                        canvas[(oy + yy) * width + ox + xx] = match b {
                            b'0' => surface.background,
                            b'1' => surface.edge,
                            _ => match surface.fill {
                                Fill::Flat(v) => v,
                                Fill::Ramp(base) => base + box_y.min(13) as u8,
                            },
                        };
                    }
                }
            }
        }
        // 보호 타일 중 같은 픽셀이 있으면 원래 타일을 재사용한다.
        let mut lookup: HashMap<Vec<u8>, usize> = HashMap::new();
        for i in 0..tile_count {
            if !available.contains(&i) {
                lookup.insert(
                    source[surface.tiles + i * 64..surface.tiles + (i + 1) * 64].to_vec(),
                    i,
                );
            }
        }
        let mut free = free_order.iter();
        let mut used = Vec::new();
        for cells in &region_cells {
            let mut region_used = 0;
            for &cell in cells {
                let (x, y) = (cell % surface.map_width * 8, cell / surface.map_width * 8);
                let block: Vec<u8> = (0..8)
                    .flat_map(|py| canvas[(y + py) * width + x..(y + py) * width + x + 8].to_vec())
                    .collect();
                let index = match lookup.get(&block) {
                    Some(&i) => i,
                    None => {
                        let &i = free.next().context("전용 타일 예산 초과")?;
                        target[surface.tiles + i * 64..surface.tiles + (i + 1) * 64]
                            .copy_from_slice(&block);
                        lookup.insert(block.clone(), i);
                        region_used += 1;
                        i
                    }
                };
                let word = (entries[cell] & 0xf000) | index as u16;
                target[surface.map + cell * 2..surface.map + cell * 2 + 2]
                    .copy_from_slice(&word.to_le_bytes());
            }
            used.push(region_used);
        }
        // 결과 화면 전체 대조: canvas는 편집 셀 밖이 원본 그대로이므로 밖의 불변도 함께 검사된다.
        let written = render(&target, surface);
        ensure!(
            written == canvas,
            "타일 픽셀 왕복 또는 편집 셀 밖 화면 불일치"
        );
        // 보존 영문은 원본 픽셀 복사만 허용한다.
        let mut preserved_pixels = 0;
        for &(from, last, destination, top, h) in &moved {
            for y in top..top + h {
                for x in from..=last {
                    ensure!(
                        written[y * width + x - from + destination] == original[y * width + x],
                        "보존 영문 픽셀 불일치"
                    );
                    preserved_pixels += 1;
                }
            }
        }
        let regions: Vec<_> = surface
            .regions
            .iter()
            .zip(&region_cells)
            .zip(&used)
            .map(|((region, cells), used)| {
                let mask = masks.iter().find(|m| m.id == region.mask).unwrap();
                serde_json::json!({"unit":region.unit,"text":mask.text(),"mask":mask.id,
                    "style":mask.style,"font_sha256":mask.font_sha256,"font_size_px":mask.size_px,
                    "font_baseline":mask.baseline,"license":mask.license,
                    "editable_map_cells":cells.len(),"dedicated_tiles":region.expected_dedicated,
                    "newly_written_tiles":used})
            })
            .collect();
        reports.push(serde_json::json!({"surface":surface.id,"regions":regions,
            "heading_layout":region_reports,"dedicated_tiles":available.len(),
            "used_tiles":used.iter().sum::<usize>(),"preserved_pixels_copied":preserved_pixels}));
    }
    let mut changes = 0;
    for (i, (&before, &after)) in source.iter().zip(&target).enumerate() {
        if before != after {
            ensure!(allowed[i], "최종 보호 범위 침범: {i:X}");
            changes += 1;
        }
    }
    Ok((
        target,
        serde_json::json!({"surfaces":reports,"changed_bytes":changes,
        "final_write_audit":"pass","mask_sha256":crate::source::sha256(masks_text.as_bytes()),
        "wording_decisions_sha256":crate::source::sha256(&crate::managed_input::read(DECISIONS_INPUT)?)}),
    ))
}
