//! 난이도 목록의 격자·테두리를 보존하고 기본/강조에 같은 글자를 조판한다.
//! 문구는 원본 한자와 같은 무게(면 57, 8방향 1px 외곽선 51, 면 y4..14)의 한 줄이다.
use crate::source::sha256;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeSet;
#[derive(Deserialize)]
struct Draft {
    source_id: String,
    policy: String,
    entries: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    index: usize,
    source_text: String,
    korean_lines: Vec<String>,
    status: String,
}
#[derive(Deserialize)]
struct Masks {
    policy: String,
    draft_sha256: String,
    font_sha256: String,
    font_size: usize,
    license: String,
    entries: Vec<Mask>,
}
#[derive(Deserialize)]
struct Mask {
    index: usize,
    rows: Vec<String>,
}
fn offset(x: usize, y: usize, w: usize) -> usize {
    (y / 8 * (w / 8) + x / 8) * 64 + y % 8 * 8 + x % 8
}
/// 사용자 결정 2026-10-04(시안 B, "화끈"→"불맛")의 문안. 문구 변경은 결정 기록과 함께 바꾼다.
const WORDS: [(&str, &str); 5] = [
    ("激甘", "달콤"),
    ("甘口", "순한"),
    ("中辛", "보통"),
    ("辛口", "매운"),
    ("激辛", "불맛"),
];
const DECISIONS_INPUT: &str = "config/korean-wording-decisions.json";
/// 마스크가 면(2) 주위 8방향 1px 외곽선(1)과 정확히 같고, 면이 원본 기준선 안에 가운데 정렬됐는지 검사한다.
fn check_mask(rows: &[String]) -> Result<()> {
    let at = |x: isize, y: isize| {
        (0..32).contains(&x)
            && (0..16).contains(&y)
            && rows[y as usize].as_bytes()[x as usize] == b'2'
    };
    let (mut left, mut right, mut top, mut bottom) = (32, 0, 16, 0);
    for (y, row) in rows.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            let near = (-1..=1).any(|dy| {
                (-1..=1).any(|dx| (dx, dy) != (0, 0) && at(x as isize + dx, y as isize + dy))
            });
            let expected = if c == b'2' {
                b'2'
            } else if near {
                b'1'
            } else {
                b'0'
            };
            ensure!(c == expected, "난이도 외곽선 규칙 불일치");
            if c == b'2' {
                (left, right) = (left.min(x), right.max(x));
                (top, bottom) = (top.min(y), bottom.max(y));
            }
        }
    }
    let width = right + 1 - left;
    ensure!(
        right >= left
            && top >= 4
            && bottom <= 14
            && width + 2 <= 28
            && left == 4 + (28 - width) / 2,
        "난이도 면 기준선·가운데 정렬 오류"
    );
    Ok(())
}
fn base_pixel(source: &[u8], start: usize, x: usize, y: usize) -> u8 {
    source[start + y / 64 * 4096 + offset(x, y % 64, 64)]
}
pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let dr = &crate::managed_input::read("assets/translations/difficulty-labels.json")?;
    let mr = &crate::managed_input::read("assets/fonts/difficulty-label-masks.json")?;
    let d: Draft = serde_json::from_slice(dr)?;
    let m: Masks = serde_json::from_slice(mr)?;
    ensure!(
        d.source_id == "apyj-rev0"
            && d.policy == "development"
            && m.policy == "development"
            && m.draft_sha256 == sha256(dr)
            && d.entries.len() == 5
            && m.entries.len() == 5,
        "난이도 문안 결속 오류"
    );
    ensure!(
        m.font_sha256 == "2c709890595668f7bdb6df408420fda957dde0288e95b31a1cc17a2ab98b4b4f"
            && m.font_size == 12
            && m.license == "licenses/fonts/galmuri-v2.40.3-LICENSE.txt",
        "난이도 폰트 오류"
    );
    let decisions: serde_json::Value =
        serde_json::from_str(&crate::managed_input::read_string(DECISIONS_INPUT)?)?;
    let decisions = decisions.as_array().context("문안 결정 배열 필요")?;
    for (i, (e, mask)) in d.entries.iter().zip(&m.entries).enumerate() {
        let unit = format!("difficulty-{i}");
        let decided: Vec<_> = decisions
            .iter()
            .filter(|row| row["source"] == "config/hud-source-text.json" && row["unit_id"] == unit)
            .collect();
        ensure!(
            decided.len() == 1
                && decided[0]["source_text"] == WORDS[i].0
                && decided[0]["korean_text"] == WORDS[i].1
                && e.korean_lines == [WORDS[i].1],
            "난이도 문안 결정 불일치"
        );
        ensure!(
            e.index == i
                && mask.index == i
                && e.source_text == WORDS[i].0
                && e.status == "needs_review"
                && mask.rows.len() == 16
                && mask
                    .rows
                    .iter()
                    .all(|r| r.len() == 32 && r.bytes().all(|v| b"012".contains(&v)))
                && mask.rows.iter().all(|r| r.as_bytes()[..4] == *b"0000"),
            "난이도 문안/마스크 경계 오류"
        );
        check_mask(&mask.rows)?;
    }
    let mut target = source.to_vec();
    let mut owned = BTreeSet::new();
    let mut reports = Vec::new();
    for (start, main, small) in [
        (0x1cd6e8, 0x1cd6e8, true),
        (0x581510, 0x1cd6e8, true),
        (0x1c6ee8, 0x1c6ee8, false),
        (0x5b894c, 0x1c6ee8, false),
    ] {
        ensure!(
            source[start..start + 10752] == source[main..main + 10752],
            "난이도 본체/수신 복제 차이"
        );
        let (dx, pitch, left, right) = if small {
            (0, 16, 4, 32)
        } else {
            (12, 24, 6, 50)
        };
        let mut restored = 0;
        for (option, mask) in m.entries.iter().enumerate() {
            let top = 8 + option * pitch;
            for (yy, row) in mask.rows.iter().enumerate() {
                let y = top + yy;
                for (x, c) in row.bytes().enumerate() {
                    if small && x < 4 {
                        continue;
                    }
                    let xx = dx + x;
                    let p = start + y / 64 * 4096 + offset(xx, y % 64, 64);
                    let old = source[p];
                    if [51, 57].contains(&old) {
                        let values: BTreeSet<_> = (left..right)
                            .filter(|other| other % 2 == xx % 2)
                            .map(|other| base_pixel(source, start, other, y))
                            .filter(|v| ![51, 57, 60].contains(v))
                            .collect();
                        ensure!(
                            values.len() == 1
                                && values.iter().all(|v| *v == 0 || (96..112).contains(v)),
                            "난이도 격자 복원 근거 모순"
                        );
                        target[p] = *values.first().unwrap();
                        owned.insert(p);
                        restored += 1;
                    }
                    if c != b'0' {
                        target[p] = if c == b'2' { 57 } else { 51 };
                        owned.insert(p);
                    }
                }
                for (x, c) in row.bytes().enumerate() {
                    let p = start + 8192 + option * 512 + offset(x, yy, 32);
                    ensure!(
                        source[p] == 0 || source[p] == 51 || (1..12).contains(&source[p]),
                        "난이도 강조 원본 색 차이"
                    );
                    target[p] = match c {
                        b'0' => 0,
                        b'1' => 51,
                        // 원본 강조와 같이 면 행 y4..14에 순환색 1..11을 배정한다.
                        b'2' => yy.saturating_sub(3).clamp(1, 11) as u8,
                        _ => unreachable!(),
                    };
                    ensure!(owned.insert(p), "난이도 강조 작성 중복");
                }
            }
        }
        reports.push(
            serde_json::json!({"source":start,"small":small,"restored_text_pixels":restored}),
        );
    }
    for (i, (&a, &b)) in source.iter().zip(&target).enumerate() {
        ensure!(a == b || owned.contains(&i), "난이도 보호 바이트 변경");
    }
    Ok((
        target,
        serde_json::json!({"groups":reports,"owned_bytes":owned.len(),"masks_sha256":sha256(mr),"distribution_eligible":false,"runtime_verification":"deferred_until_cumulative_insertion","final_write_audit":"pass"}),
    ))
}
