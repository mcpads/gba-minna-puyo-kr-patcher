//! 규칙·무한 카드의 공통 격자와 관리 폰트 마스크를 원본에서 합성한다.
use crate::{
    selection_cards::{offset, pointer},
    source::sha256,
};
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
struct Draft {
    source_id: String,
    policy: String,
    cards: Vec<Text>,
}
#[derive(Deserialize)]
struct Text {
    index: usize,
    source: usize,
    source_sha256: String,
    source_text: String,
    korean_text: String,
    status: String,
}
#[derive(Deserialize)]
struct Masks {
    policy: String,
    draft_sha256: String,
    font_sha256: String,
    font_size: usize,
    license: String,
    fill: u8,
    edge: u8,
    cards: Vec<Mask>,
}
#[derive(Deserialize)]
struct Mask {
    index: usize,
    rows: Vec<String>,
}

pub fn rules(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    transform(
        source,
        &crate::managed_input::read("assets/translations/rule-cards.json")?,
        &crate::managed_input::read("assets/fonts/rule-card-masks.json")?,
        0x54ba40,
        0x54ba5c,
        6,
        Some("rule-cards"),
    )
}
pub fn endless(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    transform(
        source,
        &crate::managed_input::read("assets/translations/endless-cards.json")?,
        &crate::managed_input::read("assets/fonts/endless-card-masks.json")?,
        0x54b9cc,
        0x54b9d4,
        2,
        None,
    )
}
fn transform(
    source: &[u8],
    draft_raw: &[u8],
    mask_raw: &[u8],
    ja_root: usize,
    en_root: usize,
    count: usize,
    overrides: Option<&str>,
) -> Result<(Vec<u8>, serde_json::Value)> {
    let draft: Draft = serde_json::from_slice(draft_raw)?;
    let masks: Masks = serde_json::from_slice(mask_raw)?;
    let value: serde_json::Value = serde_json::from_slice(mask_raw)?;
    // 손질 글리프 대체표(Galmuri11 OFL 파생)를 쓰는 카드만 결속을 가진다.
    match overrides {
        Some(surface) => crate::glyph_overrides::verify(&value, surface)?,
        None => ensure!(
            value.get("glyph_overrides").is_none(),
            "손질 글리프 미사용 카드의 결속 기록"
        ),
    }
    ensure!(
        draft.source_id == "apyj-rev0"
            && draft.policy == "development"
            && masks.policy == "development"
            && masks.draft_sha256 == sha256(draft_raw),
        "선택 카드 입력 정책/해시 오류"
    );
    ensure!(
        masks.font_sha256 == "2c709890595668f7bdb6df408420fda957dde0288e95b31a1cc17a2ab98b4b4f"
            && masks.font_size == 12
            && masks.license == "licenses/fonts/galmuri-v2.40.3-LICENSE.txt"
            && masks.fill == 48
            && masks.edge == 39
            && value["box"] == serde_json::json!([0, 54, 80, 16]),
        "선택 카드 폰트/경계 오류"
    );
    ensure!(
        draft.cards.len() == count && masks.cards.len() == count,
        "선택 카드 분모 오류"
    );
    ensure!(
        (0..80)
            .flat_map(|y| (0..80).map(move |x| offset(x, y)))
            .collect::<BTreeSet<_>>()
            == (0..6400).collect(),
        "카드 픽셀 대응 오류"
    );
    let witnesses: Vec<_> = [(0x54ba40, 7), (0x54ba5c, 7), (0x54b9cc, 2), (0x54b9d4, 2)]
        .into_iter()
        .flat_map(|(at, n)| (0..n).map(move |i| pointer(source, at + i * 4)))
        .collect();
    ensure!(
        witnesses.iter().all(|&at| at + 6400 <= source.len()),
        "공통 카드 배경 소스 경계 오류"
    );
    let sources: Vec<_> = [ja_root, en_root]
        .into_iter()
        .flat_map(|at| (0..count).map(move |i| pointer(source, at + i * 4)))
        .collect();
    let mut grid = [[0u8; 80]; 16];
    for (yy, row) in grid.iter_mut().enumerate() {
        for (x, pixel) in row.iter_mut().enumerate() {
            let colors: BTreeSet<_> = witnesses
                .iter()
                .map(|&at| source[at + offset(x, yy + 54)])
                .filter(|v| ![39, 48].contains(v))
                .collect();
            ensure!(
                colors.len() == 1 && colors.iter().all(|v| [75, 76, 77].contains(v)),
                "선택 카드 배경 불일치"
            );
            *pixel = *colors.first().unwrap();
        }
    }
    let mut target = source.to_vec();
    let mut allowed = BTreeSet::new();
    let mut reports = Vec::new();
    for (i, (text, mask)) in draft.cards.iter().zip(&masks.cards).enumerate() {
        let at = sources[i];
        ensure!(
            text.index == i
                && mask.index == i
                && text.source == at
                && text.source_sha256 == sha256(&source[at..at + 6400])
                && text.status == "needs_review"
                && !text.source_text.is_empty()
                && !text.korean_text.is_empty(),
            "선택 카드 문안 결속 오류"
        );
        ensure!(
            mask.rows.len() == 16
                && mask
                    .rows
                    .iter()
                    .all(|r| r.len() == 80 && r.bytes().all(|c| b"012".contains(&c))),
            "선택 카드 마스크 오류"
        );
        for y in (0..80).filter(|y| !(54..70).contains(y)) {
            for x in 0..80 {
                let p = offset(x, y);
                ensure!(
                    source[at + p] == source[sources[i + count] + p],
                    "선택 카드 이름표 밖 양언어 차이"
                );
            }
        }
        for (yy, (row, background)) in mask.rows.iter().zip(&grid).enumerate() {
            for (x, (code, &pixel)) in row.bytes().zip(background).enumerate() {
                let p = offset(x, yy + 54);
                if [39, 48].contains(&source[at + p]) {
                    target[at + p] = pixel;
                    allowed.insert(at + p);
                }
                if code != b'0' {
                    target[at + p] = if code == b'2' { 48 } else { 39 };
                    allowed.insert(at + p);
                }
            }
        }
        reports.push(serde_json::json!({"index":i,"text":text.korean_text,"source":at}));
    }
    for (at, (&a, &b)) in source.iter().zip(&target).enumerate() {
        ensure!(
            a == b || allowed.contains(&at),
            "선택 카드 보호 바이트 변경"
        );
    }
    Ok((
        target,
        serde_json::json!({"cards":reports,"masks_sha256":sha256(mask_raw),"background_witness_slots":18,"same_coordinate_background_pixels":1280,"periodic_copy_pixels":0,"distribution_eligible":false,"final_write_audit":"pass"}),
    ))
}
