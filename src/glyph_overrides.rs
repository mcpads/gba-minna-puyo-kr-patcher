//! Galmuri 손질 글리프 대체표(OFL 파생 수정 글리프, 마스크 생성 입력)와 마스크의 표면별 결속을 대조한다.
//!
//! 대체표는 표면별 적용 글자(`surfaces`)를 가진다. 준비 스크립트는 자기 표면에 적힌 글자만 손질 글리프와
//! 글자 뒤 자간으로 바꾸고, 마스크의 `glyph_overrides`에 폰트별 {대체표 SHA-256, 표면, 적용 글자}를 기록한다.
//! 컴포넌트는 그 기록이 현재 대체표와 같은지 확인한다. 대체표 하나를 모든 표면이 공유하지 않는다
//! (4차 채택 glyph-extend A: 실제로 묻히는 글자만 표면별로 손질).
use crate::source::sha256;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// (폰트 이름, 대체표 입력 경로, 원 폰트 SHA-256, 글리프 높이).
const TABLES: [(&str, &str, &str, usize); 2] = [
    (
        "galmuri11",
        "assets/fonts/galmuri11-glyph-overrides.json",
        "2c709890595668f7bdb6df408420fda957dde0288e95b31a1cc17a2ab98b4b4f",
        11,
    ),
    (
        "galmuri9",
        "assets/fonts/galmuri9-glyph-overrides.json",
        "5cb68052ee0a15571747e91c20f145e24b51bb459c6cd58226fafee78d9c0b16",
        9,
    ),
];
/// 대체표를 쓰는 표면(마스크 준비 단위)과 그 표면이 결속해야 하는 폰트.
const SURFACES: [(&str, &[&str]); 7] = [
    ("options", &["galmuri11"]),
    ("ranking-courses", &["galmuri11"]),
    ("course-cards", &["galmuri11"]),
    ("character-names", &["galmuri11"]),
    ("link-connection", &["galmuri11"]),
    ("rules", &["galmuri11", "galmuri9"]),
    ("rule-cards", &["galmuri11"]),
];
const LICENSE: &str = "licenses/fonts/galmuri-v2.40.3-LICENSE.txt";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    role: String,
    derived_from: Derived,
    license_note: String,
    decision_source: String,
    origin: String,
    rule: String,
    surfaces: BTreeMap<String, String>,
    glyphs: BTreeMap<String, Vec<String>>,
    trailing_tracking: BTreeMap<String, u8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Derived {
    font: String,
    font_sha256: String,
    size: u8,
    license: String,
}

/// 대체표를 읽고 형식을 검사한다. 반환: (표면별 적용 글자, 대체표 SHA-256).
fn table(font: &str) -> Result<(BTreeMap<String, String>, String)> {
    let &(_, input, font_sha, height) = TABLES
        .iter()
        .find(|t| t.0 == font)
        .context("미등록 대체표 폰트")?;
    let raw = &crate::managed_input::read(input)?;
    let t: Table = serde_json::from_slice(raw)?;
    ensure!(
        t.derived_from.font_sha256 == font_sha
            && t.derived_from.license == LICENSE
            && t.license_note.contains("Open Font License")
            && !t.role.is_empty()
            && !t.decision_source.is_empty()
            && !t.origin.is_empty()
            && !t.rule.is_empty()
            && !t.derived_from.font.is_empty()
            && t.derived_from.size > 0,
        "손질 글리프 원 폰트/라이선스 결속 오류: {font}"
    );
    let expected: BTreeSet<_> = SURFACES
        .iter()
        .filter(|s| s.1.contains(&font))
        .map(|s| s.0.to_string())
        .collect();
    ensure!(
        t.surfaces.keys().cloned().collect::<BTreeSet<_>>() == expected,
        "손질 글리프 표면 목록 오류: {font}"
    );
    let mut used = BTreeSet::new();
    for chars in t.surfaces.values() {
        let set: BTreeSet<_> = chars.chars().map(String::from).collect();
        ensure!(
            !set.is_empty() && set.len() == chars.chars().count(),
            "표면 적용 글자 중복/공백: {font}"
        );
        used.extend(set);
    }
    ensure!(
        used == t.glyphs.keys().cloned().collect(),
        "표면 적용 글자와 손질 글리프 불일치: {font}"
    );
    for (ch, rows) in &t.glyphs {
        ensure!(
            ch.chars().count() == 1
                && rows.len() == height
                && rows
                    .iter()
                    .all(|r| r.len() == rows[0].len() && r.bytes().all(|c| c == b'#' || c == b'.'))
                && rows.iter().any(|r| r.contains('#')),
            "손질 글리프 형식 오류: {font}/{ch}"
        );
    }
    ensure!(
        t.trailing_tracking
            .iter()
            .all(|(ch, &v)| t.glyphs.contains_key(ch) && v > 0),
        "추가 자간은 손질 글리프에만 양의 정수로 둔다: {font}"
    );
    Ok((t.surfaces, sha256(raw)))
}

/// 마스크의 `glyph_overrides`가 표면 `surface`의 폰트별 대체표 SHA-256·적용 글자와 같은지 확인한다.
pub fn verify(masks: &serde_json::Value, surface: &str) -> Result<()> {
    let fonts = SURFACES
        .iter()
        .find(|s| s.0 == surface)
        .context("미등록 손질 글리프 표면")?
        .1;
    let binding = masks["glyph_overrides"]
        .as_object()
        .context("손질 글리프 결속 누락")?;
    ensure!(
        binding.keys().map(String::as_str).collect::<BTreeSet<_>>()
            == fonts.iter().copied().collect(),
        "손질 글리프 결속 폰트 오류: {surface}"
    );
    for font in fonts {
        let (surfaces, sha) = table(font)?;
        let b = &binding[*font];
        ensure!(
            b.as_object().is_some_and(|o| o.len() == 3)
                && b["sha256"] == sha.as_str()
                && b["surface"] == surface
                && b["chars"] == surfaces[surface].as_str(),
            "손질 글리프 결속 오류: {surface}/{font}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires assets/fonts/galmuri11-glyph-overrides.json and galmuri9-glyph-overrides.json"]
    fn tables_cover_each_surface() {
        for (font, ..) in super::TABLES {
            super::table(font).unwrap();
        }
        let masks = serde_json::json!({"glyph_overrides": {"galmuri11": {"sha256": "0", "surface": "options", "chars": ""}}});
        assert!(super::verify(&masks, "options").is_err());
        assert!(super::verify(&masks, "unknown").is_err());
    }
}
