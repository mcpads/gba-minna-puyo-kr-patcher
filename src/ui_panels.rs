//! 옵션·규칙 편집·랭킹 제목의 보호 타일을 유지하며 문구를 시트별로 재배정한다.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
/// 마스크 파일이 쓸 수 있는 폰트(이름, SHA-256, 라이선스). 표면은 이 중 하나를 `font`로 지정한다.
const FONTS: [(&str, &str, &str); 3] = [
    (
        "galmuri11",
        "2c709890595668f7bdb6df408420fda957dde0288e95b31a1cc17a2ab98b4b4f",
        "licenses/fonts/galmuri-v2.40.3-LICENSE.txt",
    ),
    (
        "galmuri9",
        "5cb68052ee0a15571747e91c20f145e24b51bb459c6cd58226fafee78d9c0b16",
        "licenses/fonts/galmuri-v2.40.3-LICENSE.txt",
    ),
    (
        "neodgm16",
        "d61b60eccb731f8ca9c7da582e4a05a94db66b570471809950aa9a7261b941d6",
        "licenses/fonts/neodgm-v1.600-OFL.txt",
    ),
];
#[derive(Deserialize)]
struct Input {
    policy: String,
    fonts: BTreeMap<String, FontRef>,
    surfaces: Vec<Surface>,
}
#[derive(Deserialize)]
struct FontRef {
    sha256: String,
    license: String,
}
/// 표면 하나. `rows`(0 배경·1 외곽선·2 면)와 `index_rows`(픽셀당 16진 2자리 색인, 상자 전체) 중
/// 정확히 하나를 쓴다. `dy`가 없으면 세로 가운데, `fill_y_offset`은 면 음영 `fill+(y+offset)` 기준을 옮긴다.
#[derive(Deserialize)]
struct Surface {
    id: String,
    text: String,
    render_text: String,
    font: String,
    align: String,
    tiles: usize,
    tile_bytes: usize,
    map: usize,
    width: usize,
    #[serde(rename = "box")]
    bounds: [usize; 4],
    /// 원문이 차지한 타일 경계. 상자가 이 밖으로 넓어진 셀은 원본에서 빈 타일이어야 한다.
    source_box: [usize; 4],
    #[serde(default)]
    edge: u8,
    #[serde(default)]
    fill: u8,
    #[serde(default)]
    dy: Option<usize>,
    #[serde(default)]
    fill_y_offset: i32,
    #[serde(default)]
    rows: Vec<String>,
    #[serde(default)]
    index_rows: Vec<String>,
    /// 보존 영문(`/LANGUAGE`·`G`)의 원본 픽셀 이동. 원본 시트 사각형 `source`의 0이 아닌 픽셀을
    /// 상자 안 `at`으로 복사만 한다(시트 절대 좌표).
    #[serde(default)]
    preserved: Vec<Preserved>,
}
#[derive(Deserialize, PartialEq, Debug)]
struct Preserved {
    source: [usize; 4],
    at: [usize; 2],
}
/// 3차 채택(options-castle C, language-gap A)의 보존 영문 이동. 다른 표면은 보존 이동을 쓰지 않는다.
const LANGUAGE_PRESERVED: Preserved = Preserved {
    source: [88, 112, 64, 16],
    at: [72, 112],
};
const CASTLE_PRESERVED: Preserved = Preserved {
    source: [0, 112, 16, 16],
    at: [40, 112],
};
/// 맵 항목을 따라 원본 시트의 화면 픽셀을 읽는다(반전 반영).
fn sheet_pixel(data: &[u8], tiles: usize, entries: &[u16], width: usize, x: usize, y: usize) -> u8 {
    let e = entries[y / 8 * width + x / 8];
    let px = if e & 0x400 != 0 { 7 - x % 8 } else { x % 8 };
    let py = if e & 0x800 != 0 { 7 - y % 8 } else { y % 8 };
    data[tiles + usize::from(e & 1023) * 64 + py * 8 + px]
}
pub fn options(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let raw = &crate::managed_input::read_string("assets/fonts/options-masks.json")?;
    let masks: serde_json::Value = serde_json::from_str(raw)?;
    // 손질 글리프(Galmuri11 OFL 파생)는 마스크 생성 입력이다. 마스크가 현재 대체표의 옵션 글자로 만들어졌는지 결속한다.
    crate::glyph_overrides::verify(&masks, "options")?;
    transform(
        source,
        raw,
        &crate::managed_input::read_string("assets/translations/ui-draft.json")?,
        &[
            (0xf2b08, 0x2800, 0xf5308, 30, 20, 9),
            (0xf57b8, 0x4c00, 0xfa3b8, 20, 30, 14),
        ],
    )
}
pub fn rules(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let raw = &crate::managed_input::read_string("assets/fonts/rule-masks.json")?;
    // 라벨(Galmuri11)·값(Galmuri9) 손질 글리프 대체표의 규칙 글자 결속.
    crate::glyph_overrides::verify(&serde_json::from_str(raw)?, "rules")?;
    transform(
        source,
        raw,
        &crate::managed_input::read_string("assets/translations/ui-draft.json")?,
        &[
            (0x108a08, 0x3400, 0x10be08, 30, 20, 9),
            (0x10c2b8, 0x2800, 0x10eab8, 24, 13, 7),
        ],
    )
}
pub fn ranking(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let draft_raw =
        &crate::managed_input::read_string("assets/translations/ranking-headings.json")?;
    let raw = &crate::managed_input::read_string("assets/fonts/ranking-heading-masks.json")?;
    let draft: serde_json::Value = serde_json::from_str(draft_raw)?;
    let masks: serde_json::Value = serde_json::from_str(raw)?;
    ensure!(
        draft["source_id"] == "apyj-rev0"
            && draft["source_sha256"] == crate::source::sha256(&source[0x103dc8..0x105b68])
            && masks["draft_sha256"] == crate::source::sha256(draft_raw.as_bytes()),
        "랭킹 제목 입력 결속 오류"
    );
    for (i, surface) in masks["surfaces"]
        .as_array()
        .context("랭킹 마스크 누락")?
        .iter()
        .enumerate()
    {
        ensure!(
            i < 2
                && surface["id"] == ["ranking.heading", "ranking.name-entry"][i]
                && surface["box"] == serde_json::json!([0, 160 + i * 16, 136, 16])
                && surface["source_box"] == surface["box"]
                && surface["font"] == "neodgm16"
                && surface["index_rows"]
                    .as_array()
                    .is_some_and(|r| r.len() == 16)
                && surface.get("rows").is_none()
                && draft["entries"][i]["status"] == "needs_review",
            "랭킹 제목 보호 경계 오류"
        );
    }
    transform(
        source,
        raw,
        draft_raw,
        &[(0x103dc8, 0x1800, 0x1055c8, 30, 24, 2)],
    )
}
fn transform(
    source: &[u8],
    raw: &str,
    draft_raw: &str,
    sheets: &[(usize, usize, usize, usize, usize, usize)],
) -> Result<(Vec<u8>, serde_json::Value)> {
    crate::source::Profile::supported()?.verify(source)?;
    let input: Input = serde_json::from_str(raw)?;
    let drafts: serde_json::Value = serde_json::from_str(draft_raw)?;
    ensure!(
        drafts["policy"] == "development_selected_ui",
        "UI 개발 정책 불일치"
    );
    ensure!(
        input.policy == "development"
            && input.surfaces.len() == sheets.iter().map(|s| s.5).sum::<usize>(),
        "UI 개발 입력 분모 오류"
    );
    for (name, font) in &input.fonts {
        ensure!(
            FONTS
                .iter()
                .any(|&(n, h, l)| n == name && h == font.sha256 && l == font.license),
            "UI 폰트 결속 오류: {name}"
        );
    }
    ensure!(
        input
            .surfaces
            .iter()
            .all(|s| input.fonts.contains_key(&s.font)),
        "UI 표면 폰트 미등록"
    );
    let mut target = source.to_vec();
    let mut allowed = BTreeSet::new();
    let mut records = Vec::new();
    for &(tiles, size, map, width, height, count) in sheets {
        let regions: Vec<_> = input.surfaces.iter().filter(|s| s.tiles == tiles).collect();
        ensure!(regions.len() == count, "UI 시트 문구 수 오류");
        let entries: Vec<u16> = source[map..map + width * height * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        ensure!(
            entries.iter().all(|&e| usize::from(e & 1023) < size / 64),
            "UI 타일 경계 오류"
        );
        let mut editable = BTreeSet::new();
        let mut blocks = BTreeMap::new();
        let mut moved = Vec::new();
        for s in regions {
            ensure!(
                (s.tile_bytes, s.map, s.width) == (size, map, width),
                "UI 저장 경계 변경"
            );
            let draft = drafts["entries"]
                .as_array()
                .context("초안 목록 없음")?
                .iter()
                .find(|v| v["id"] == s.id)
                .context("UI 문안 ID 없음")?;
            ensure!(
                drafts["development_scope"]
                    .as_array()
                    .context("개발 범위 없음")?
                    .iter()
                    .any(|v| v == &s.id),
                "UI 개발 범위 밖 입력"
            );
            ensure!(draft["korean_text"] == s.text, "UI 문안/보호 영문 오류");
            let [left, top, w, h] = s.bounds;
            match s.id.as_str() {
                "options.base.language" => ensure!(
                    s.bounds == [16, 112, 136, 16]
                        && s.source_box == s.bounds
                        && s.align == "left"
                        && s.preserved == [LANGUAGE_PRESERVED]
                        && s.text == format!("{}/LANGUAGE", s.render_text),
                    "LANGUAGE 보호 경계/문안 오류"
                ),
                "options.value.ranking-course.castle" => ensure!(
                    s.bounds == [0, 112, 112, 16]
                        && s.source_box == s.bounds
                        && s.align == "right"
                        && s.preserved == [CASTLE_PRESERVED]
                        && s.text == format!("G{}", s.render_text),
                    "G 보호 경계/문안 오류"
                ),
                _ => ensure!(
                    s.text == s.render_text && s.preserved.is_empty(),
                    "UI 렌더 문안 불일치"
                ),
            }
            ensure!(
                [left, top, w, h].iter().all(|v| v % 8 == 0)
                    && left + w <= width * 8
                    && top + h <= height * 8,
                "UI 편집 경계 오류"
            );
            let [sl, st, sw, sh] = s.source_box;
            ensure!(
                [sl, st, sw, sh].iter().all(|v| v % 8 == 0)
                    && sw > 0
                    && sh > 0
                    && sl + sw <= width * 8
                    && st + sh <= height * 8,
                "UI 원문 경계 오류"
            );
            let tile = |cell: usize| {
                let id = usize::from(entries[cell] & 1023);
                &source[tiles + id * 64..tiles + (id + 1) * 64]
            };
            let in_source = |cx: usize, cy: usize| {
                (sl / 8..(sl + sw) / 8).contains(&cx) && (st / 8..(st + sh) / 8).contains(&cy)
            };
            // 원문 칸의 실제 색인. 색인 직접 지정 표면은 이 집합 밖 색을 쓰지 못한다.
            let source_indices: BTreeSet<u8> = (st / 8..(st + sh) / 8)
                .flat_map(|cy| (sl / 8..(sl + sw) / 8).map(move |cx| cy * width + cx))
                .flat_map(|cell| tile(cell).iter().copied())
                .filter(|&v| v != 0)
                .collect();
            let indexed = !s.index_rows.is_empty();
            ensure!(indexed == s.rows.is_empty(), "UI 마스크 형식 중복/누락");
            let pixels: Vec<Vec<u8>> = if indexed {
                ensure!(
                    (s.edge, s.fill, s.fill_y_offset) == (0, 0, 0),
                    "색인 표면의 역할 색 지정"
                );
                s.index_rows
                    .iter()
                    .map(|r| {
                        ensure!(r.len() % 2 == 0, "UI 색인 행 길이 오류");
                        (0..r.len() / 2)
                            .map(|i| {
                                let v = u8::from_str_radix(&r[i * 2..i * 2 + 2], 16)
                                    .context("UI 색인 값 오류")?;
                                ensure!(
                                    v == 0 || source_indices.contains(&v),
                                    "UI 색인이 원문 색 밖: {} {v}",
                                    s.id
                                );
                                Ok(v)
                            })
                            .collect()
                    })
                    .collect::<Result<_>>()?
            } else {
                ensure!(s.edge != 0 && s.fill != 0, "UI 역할 색 누락");
                s.rows
                    .iter()
                    .map(|r| {
                        r.bytes()
                            .map(|c| match c {
                                b'0' | b'1' | b'2' => Ok(c - b'0'),
                                _ => anyhow::bail!("UI 마스크 값 오류"),
                            })
                            .collect()
                    })
                    .collect::<Result<_>>()?
            };
            let mw = pixels.first().context("빈 마스크")?.len();
            let mh = pixels.len();
            ensure!(
                mw > 0 && mw <= w && mh <= h && pixels.iter().all(|r| r.len() == mw),
                "UI 조판 잘림"
            );
            ensure!(!indexed || (mw, mh) == (w, h), "UI 색인 표면은 상자 전체");
            let dx = match s.align.as_str() {
                "left" => 0,
                "center" => (w - mw) / 2,
                "right" => w - mw,
                _ => anyhow::bail!("UI 정렬 값 오류"),
            };
            let dy = s.dy.unwrap_or((h - mh) / 2);
            ensure!(dy + mh <= h, "UI 세로 배치 초과");
            // 상자 전체 색인. 마스크 밖 상자 픽셀은 0이다.
            let mut grid = vec![vec![0u8; w]; h];
            for (my, row) in pixels.iter().enumerate() {
                for (mx, &v) in row.iter().enumerate() {
                    let y = my + dy;
                    grid[y][mx + dx] = if indexed {
                        v
                    } else {
                        match v {
                            0 => 0,
                            1 => s.edge,
                            _ => {
                                let shade = (y as i32 + s.fill_y_offset).min(13);
                                ensure!(shade >= 0, "음영 기준 오류");
                                s.fill.checked_add(shade as u8).context("음영 색인 초과")?
                            }
                        }
                    };
                }
            }
            // 보존 영문: 원본 시트 사각형의 0이 아닌 픽셀만 새 위치로 복사한다. 원문 칸 안에서만 가져오고,
            // 문구 픽셀이나 다른 복사 픽셀과 겹치면 실패한다.
            let text_pixels: Vec<(usize, usize, u8)> = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|&(x, y)| grid[y][x] != 0)
                .map(|(x, y)| (left + x, top + y, grid[y][x]))
                .collect();
            let mut copied = Vec::new();
            for p in &s.preserved {
                let [px0, py0, pw, ph] = p.source;
                let [ax, ay] = p.at;
                ensure!(
                    pw > 0
                        && ph > 0
                        && px0 >= sl
                        && py0 >= st
                        && px0 + pw <= sl + sw
                        && py0 + ph <= st + sh
                        && ax >= left
                        && ay >= top
                        && ax + pw <= left + w
                        && ay + ph <= top + h,
                    "보존 영문 사각형 경계 오류: {}",
                    s.id
                );
                for y in 0..ph {
                    for x in 0..pw {
                        let v = sheet_pixel(source, tiles, &entries, width, px0 + x, py0 + y);
                        if v == 0 {
                            continue;
                        }
                        let cell = &mut grid[ay + y - top][ax + x - left];
                        ensure!(*cell == 0, "보존 영문·문구 겹침: {}", s.id);
                        *cell = v;
                        copied.push((ax + x, ay + y, v));
                    }
                }
            }
            ensure!(
                s.preserved.is_empty() || !copied.is_empty(),
                "보존 영문 사각형이 비어 있음: {}",
                s.id
            );
            match s.id.as_str() {
                // 한글 잉크(면) 끝과 `/` 잉크(면 색인) 시작 사이 빈 4열(시작 언어 화면 원본 간격).
                "options.base.language" => {
                    let face = |v: u8| v > s.edge && v <= s.edge + 14;
                    let ink_end = text_pixels.iter().filter(|p| face(p.2)).map(|p| p.0).max();
                    let slash = copied.iter().filter(|p| face(p.2)).map(|p| p.0).min();
                    ensure!(
                        matches!((ink_end, slash), (Some(a), Some(b)) if b == a + 5),
                        "LANGUAGE 간격 오류: {ink_end:?}/{slash:?}"
                    );
                }
                // G와 문구 외곽선 사이 빈 1열, 문구 오른쪽 끝은 다른 코스 값과 같은 상자 끝.
                "options.value.ranking-course.castle" => {
                    let g_end = copied.iter().map(|p| p.0).max();
                    let text_start = text_pixels.iter().map(|p| p.0).min();
                    let text_end = text_pixels.iter().map(|p| p.0).max();
                    ensure!(
                        matches!((g_end, text_start), (Some(a), Some(b)) if b == a + 2)
                            && text_end == Some(left + w - 1),
                        "G 간격/정렬 오류: {g_end:?}/{text_start:?}/{text_end:?}"
                    );
                }
                _ => (),
            }
            moved.extend(copied.iter().map(|&(x, y, v)| (s.id.clone(), x, y, v)));
            for cy in top / 8..(top + h) / 8 {
                for cx in left / 8..(left + w) / 8 {
                    let cell = cy * width + cx;
                    ensure!(editable.insert(cell), "UI 편집 셀 중복");
                    ensure!(
                        in_source(cx, cy) || tile(cell).iter().all(|&v| v == 0),
                        "UI 확장 셀이 원본 빈칸이 아님: {} ({cx},{cy})",
                        s.id
                    );
                    let block: Vec<u8> = (0..64)
                        .map(|i| grid[cy * 8 + i / 8 - top][cx * 8 + i % 8 - left])
                        .collect();
                    blocks.insert(cell, block);
                }
            }
        }
        let external: BTreeSet<_> = entries
            .iter()
            .enumerate()
            .filter(|(i, _)| !editable.contains(i))
            .map(|(_, v)| usize::from(v & 1023))
            .collect();
        let mut available: BTreeSet<_> = editable
            .iter()
            .map(|&i| usize::from(entries[i] & 1023))
            .filter(|i| !external.contains(i))
            .collect();
        // 규칙 값 로더의 전용 0x2800바이트 안에서만, 전체 24×13 맵의 미참조 타일을 쓴다.
        // 숫자·공백·초기 배치 및 다른 문구의 모든 맵 참조는 external이 보호한다.
        let mut unused_added = 0;
        if tiles == 0x10c2b8 {
            let referenced: BTreeSet<_> = entries.iter().map(|v| usize::from(v & 1023)).collect();
            for id in 0..size / 64 {
                if !referenced.contains(&id) {
                    available.insert(id);
                    unused_added += 1;
                }
            }
            ensure!(unused_added == 13, "규칙 값 미참조 전용 타일 분모 변경");
        }
        let mut lookup = BTreeMap::new();
        for i in 0..size / 64 {
            if !available.contains(&i) {
                lookup.insert(source[tiles + i * 64..tiles + (i + 1) * 64].to_vec(), i);
            }
        }
        let mut free = available.iter();
        let mut used = 0;
        for (&cell, block) in &blocks {
            let id = if let Some(&i) = lookup.get(block) {
                i
            } else {
                let &i = free.next().with_context(|| {
                    format!(
                        "UI 전용 타일 용량 초과: 시트 {tiles:X}, 용량 {}, 작성 {used}",
                        available.len()
                    )
                })?;
                target[tiles + i * 64..tiles + (i + 1) * 64].copy_from_slice(block);
                lookup.insert(block.clone(), i);
                used += 1;
                i
            };
            target[map + cell * 2..map + cell * 2 + 2]
                .copy_from_slice(&((entries[cell] & 0xf000) | id as u16).to_le_bytes());
            ensure!(
                target[tiles + id * 64..tiles + (id + 1) * 64] == *block,
                "UI 픽셀 왕복 불일치"
            );
        }
        // 원본 픽셀 이동 검사: 최종 시트에서 옮긴 자리의 픽셀이 원본 사각형 픽셀과 같다.
        let new_entries: Vec<u16> = target[map..map + width * height * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        for (id, x, y, v) in &moved {
            ensure!(
                sheet_pixel(&target, tiles, &new_entries, width, *x, *y) == *v,
                "보존 영문 원본 픽셀 이동 불일치: {id} ({x},{y})"
            );
        }
        for &i in &available {
            allowed.extend(tiles + i * 64..tiles + (i + 1) * 64);
        }
        for &i in &editable {
            allowed.extend(map + i * 2..map + i * 2 + 2);
        }
        records.push(serde_json::json!({"tiles":tiles,"editable_cells":editable.len(),"dedicated_tiles":available.len(),"unused_tiles_added":unused_added,"used_tiles":used,"preserved_pixels_moved":moved.len(),"pixel_roundtrip":"pass"}));
    }
    let mut changed = 0;
    for (at, (&a, &b)) in source.iter().zip(&target).enumerate() {
        if a != b {
            ensure!(allowed.contains(&at), "UI 보호 범위 침범");
            changed += 1;
        }
    }
    Ok((
        target,
        serde_json::json!({"sheets":records,"phrases":input.surfaces.len(),"changed_bytes":changed,"policy":"development","final_write_audit":"pass","distribution_eligible":false}),
    ))
}
