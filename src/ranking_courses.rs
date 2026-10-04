//! 랭킹 코스 OBJ 24조각(96×16×8)을 다시 그린다. 한글 부제는 마스크로, 보존 장식 ~와 G는
//! 원본 픽셀 사각형을 새 위치로 복사만 한다.
use crate::source::sha256;
use anyhow::{Result, ensure};
use serde::Deserialize;
#[derive(Deserialize)]
struct Draft {
    source_id: String,
    policy: String,
    source_wording_sha256: String,
    courses: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    course: usize,
    source_text: String,
    korean_text: String,
    status: String,
}
#[derive(Deserialize)]
struct Masks {
    policy: String,
    draft_sha256: String,
    layout_sha256: String,
    font_sha256: String,
    font_size: usize,
    word_spacing: usize,
    license: String,
    edge: u8,
    fill_by_y: Vec<u8>,
    courses: Vec<Mask>,
}
#[derive(Deserialize)]
struct Mask {
    course: usize,
    rendered_text: String,
    start: usize,
    rows: Vec<String>,
    preserved: Vec<Preserved>,
    source_sha256: String,
}
/// 원본 보존 픽셀(장식 ~·G) 사각형 `source`를 `at`으로 옮긴다. 0이 아닌 픽셀만 복사한다.
#[derive(Deserialize)]
struct Preserved {
    source: [usize; 4],
    at: [usize; 2],
}
fn pixel_at(begin: usize, x: usize, y: usize) -> usize {
    begin + x / 32 * 512 + (y / 8 * 4 + x % 32 / 8) * 64 + y % 8 * 8 + x % 8
}
pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let dr = &crate::managed_input::read("assets/translations/ranking-courses.json")?;
    let mr = &crate::managed_input::read("assets/fonts/ranking-course-masks.json")?;
    let wr = &crate::managed_input::read("config/ranking-obj-source-text.json")?;
    let lr = &crate::managed_input::read("config/ranking-obj-layout.json")?;
    let d: Draft = serde_json::from_slice(dr)?;
    let m: Masks = serde_json::from_slice(mr)?;
    // 마스크 생성에 쓴 손질 글리프 대체표(Galmuri11 OFL 파생)의 랭킹 코스 글자 결속.
    crate::glyph_overrides::verify(&serde_json::from_slice(mr)?, "ranking-courses")?;
    let words: serde_json::Value = serde_json::from_slice(wr)?;
    let options: serde_json::Value = serde_json::from_slice(&crate::managed_input::read(
        "assets/fonts/options-masks.json",
    )?)?;
    let option_words: Vec<_> = options["surfaces"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["id"].as_str().unwrap().contains("ranking-course"))
        .map(|v| v["text"].as_str().unwrap())
        .collect();
    ensure!(
        d.source_id == "apyj-rev0"
            && d.policy == "development_ranking_courses"
            && m.policy == d.policy
            && m.draft_sha256 == sha256(dr)
            && d.source_wording_sha256 == sha256(wr)
            && m.layout_sha256 == sha256(lr)
            && d.courses.len() == 8
            && m.courses.len() == 8
            && option_words.len() == 8,
        "랭킹 코스 입력 결속 오류"
    );
    ensure!(
        m.font_sha256 == "2c709890595668f7bdb6df408420fda957dde0288e95b31a1cc17a2ab98b4b4f"
            && m.font_size == 12
            && m.word_spacing == 4
            && m.license == "licenses/fonts/galmuri-v2.40.3-LICENSE.txt"
            && m.edge == 16
            && m.fill_by_y.len() == 16
            && m.fill_by_y.iter().all(|v| (18..=22).contains(v))
            && m.fill_by_y.windows(2).all(|w| w[0] >= w[1]),
        "랭킹 폰트/색인 오류"
    );
    let mut target = source.to_vec();
    let starts = [16, 8, 8, 8, 17, 28, 8, 8];
    let mut owned = 0;
    for (i, (e, mask)) in d.courses.iter().zip(&m.courses).enumerate() {
        let begin = 0x1b5168 + i * 1536;
        ensure!(
            e.course == i
                && mask.course == i
                && e.source_text == words["units"][i]["ja"]
                && e.status == "needs_review"
                && e.korean_text == option_words[i]
                && e.korean_text
                    == format!("{}{}", if i == 4 { "G" } else { "" }, mask.rendered_text)
                && !mask.rendered_text.is_empty()
                && mask.start == starts[i]
                && mask.rows.len() == 16
                && mask
                    .rows
                    .iter()
                    .all(|r| r.len() == 96 && r.bytes().all(|c| b"012".contains(&c)))
                && sha256(&source[begin..begin + 1536]) == mask.source_sha256,
            "랭킹 코스 문구/원본 오류: {i}"
        );
        for n in 0..3 {
            let p = 0x54beb8 + (i * 3 + n) * 4;
            ensure!(
                u32::from_le_bytes(source[p..p + 4].try_into()?) as usize
                    == 0x8000000 + begin + n * 512,
                "랭킹 코스 포인터 오류"
            );
        }
        // 원래 글자 창(start..88) 밖의 원본 비영 픽셀은 모두 보존 사각형 안에 있어야 하고,
        // 각 사각형은 창 밖에서 서로 겹치지 않으며 정확히 한 번 옮겨진다.
        let original = |x: usize, y: usize| source[pixel_at(begin, x, y)];
        let rects: Vec<_> = mask.preserved.iter().map(|p| p.source).collect();
        // 여는 ~·닫는 ~, 코스4는 G까지
        ensure!(
            rects.len() == if i == 4 { 3 } else { 2 },
            "랭킹 보존 항목 수 오류: {i}"
        );
        for (a, &[x, y, w, h]) in rects.iter().enumerate() {
            ensure!(
                w > 0 && h > 0 && x + w <= 96 && y + h <= 16 && (x + w <= mask.start || x >= 88),
                "랭킹 보존 사각형 경계 오류: {i}"
            );
            ensure!(
                (y..y + h).any(|yy| (x..x + w).any(|xx| original(xx, yy) != 0)),
                "랭킹 보존 사각형이 비어 있음: {i}"
            );
            for &[x2, y2, w2, h2] in &rects[a + 1..] {
                ensure!(
                    x + w <= x2 || x2 + w2 <= x || y + h <= y2 || y2 + h2 <= y,
                    "랭킹 보존 사각형 중복: {i}"
                );
            }
        }
        for y in 0..16 {
            for x in (0..96).filter(|x| !(mask.start..88).contains(x)) {
                ensure!(
                    original(x, y) == 0
                        || rects
                            .iter()
                            .any(|&[rx, ry, rw, rh]| (rx..rx + rw).contains(&x)
                                && (ry..ry + rh).contains(&y)),
                    "랭킹 보존 픽셀 누락: {i} ({x},{y})"
                );
            }
        }
        let mut image = [[0u8; 96]; 16];
        let mut copied = [[false; 96]; 16];
        for p in &mask.preserved {
            let [sx, sy, w, h] = p.source;
            let [ax, ay] = p.at;
            ensure!(
                ax + w <= 96 && ay + h <= 16,
                "랭킹 보존 이동 범위 초과: {i}"
            );
            for y in 0..h {
                for x in 0..w {
                    let v = original(sx + x, sy + y);
                    if v != 0 {
                        ensure!(!copied[ay + y][ax + x], "랭킹 보존 이동 겹침: {i}");
                        image[ay + y][ax + x] = v;
                        copied[ay + y][ax + x] = true;
                    }
                }
            }
        }
        for (y, row) in mask.rows.iter().enumerate() {
            for (x, c) in row.bytes().enumerate() {
                if c == b'0' {
                    continue;
                }
                ensure!(!copied[y][x], "랭킹 문구·보존 픽셀 겹침: {i}");
                image[y][x] = if c == b'1' { m.edge } else { m.fill_by_y[y] };
            }
        }
        for (y, row) in image.iter().enumerate() {
            for (x, &v) in row.iter().enumerate() {
                target[pixel_at(begin, x, y)] = v;
                owned += 1;
            }
        }
    }
    let changed = source.iter().zip(&target).filter(|(a, b)| a != b).count();
    Ok((
        target,
        serde_json::json!({"courses":8,"pieces":24,"owned_pixels":owned,"changed_bytes":changed,"masks_sha256":sha256(mr),"distribution_eligible":false,"runtime_verification":"deferred_until_cumulative_insertion"}),
    ))
}
