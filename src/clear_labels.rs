//! 작은 전소거 문구와 독립 상태 표시의 일본어 공급원을 교체한다.
use crate::source::sha256;
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
struct Draft {
    source_id: String,
    policy: String,
    entries: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    id: String,
    source_text: String,
    korean_text: String,
    status: String,
    width: usize,
    height: usize,
    blocks: Vec<Block>,
}
#[derive(Deserialize)]
struct Block {
    source: usize,
    source_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Masks {
    policy: String,
    draft_sha256: String,
    entries: Vec<Mask>,
}
/// 마스크 문자 `0`은 투명, `1`은 8방향 1픽셀 외곽선, 나머지는 채움이며 `palette`로 색인에 대응한다.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mask {
    id: String,
    text: String,
    font_sha256: String,
    font_size: usize,
    license: String,
    palette: BTreeMap<char, u8>,
    #[serde(default)]
    decision: Option<String>,
    #[serde(default)]
    derivation: Vec<String>,
    rows: Vec<String>,
}
const OUTLINE: char = '1';
const OUTLINE_COLOR: u8 = 51;
const GALMURI: &str = "licenses/fonts/galmuri-v2.40.3-LICENSE.txt";
const NEODGM: &str = "licenses/fonts/neodgm-v1.600-OFL.txt";
/// 표식 고정 형식: 외곽선은 채움의 8방향 이웃 전부이고 그 밖에는 없으며, 채움 색 띠는 행 단위다.
fn check_mask_shape(mask: &Mask, w: usize, palette: &[(char, u8)]) -> Result<()> {
    ensure!(
        mask.palette.len() == palette.len()
            && palette.iter().all(|(c, v)| mask.palette.get(c) == Some(v))
            && mask.palette.get(&OUTLINE) == Some(&OUTLINE_COLOR),
        "전소거 색 대응 차이"
    );
    let grid: Vec<Vec<char>> = mask.rows.iter().map(|r| r.chars().collect()).collect();
    ensure!(
        grid.len() == 16
            && grid.iter().all(
                |r| r.len() == w && r.iter().all(|c| *c == '0' || mask.palette.contains_key(c))
            ),
        "전소거 마스크 문자/치수 차이"
    );
    let fill = |x: isize, y: isize| {
        y >= 0
            && x >= 0
            && (y as usize) < grid.len()
            && (x as usize) < w
            && !matches!(grid[y as usize][x as usize], '0' | OUTLINE)
    };
    let mut used = BTreeSet::new();
    for (y, row) in grid.iter().enumerate() {
        let mut band = BTreeSet::new();
        for (x, &c) in row.iter().enumerate() {
            used.insert(c);
            let (xi, yi) = (x as isize, y as isize);
            let near = (-1..=1).any(|dy| (-1..=1).any(|dx| fill(xi + dx, yi + dy)));
            ensure!(
                (c == OUTLINE) == (!fill(xi, yi) && near),
                "전소거 외곽선 형식 차이: {} {x},{y}",
                mask.id
            );
            if fill(xi, yi) {
                band.insert(c);
            }
        }
        ensure!(band.len() <= 1, "전소거 채움 띠 행 차이: {} {y}", mask.id);
    }
    ensure!(
        mask.palette.keys().all(|c| used.contains(c)),
        "전소거 미사용 색 대응: {}",
        mask.id
    );
    Ok(())
}
pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let draft_raw = &crate::managed_input::read("assets/translations/clear-labels.json")?;
    let mask_raw = &crate::managed_input::read("assets/fonts/clear-label-masks.json")?;
    let draft: Draft = serde_json::from_slice(draft_raw)?;
    let masks: Masks = serde_json::from_slice(mask_raw)?;
    ensure!(
        draft.source_id == "apyj-rev0"
            && draft.policy == "development"
            && masks.policy == "development"
            && masks.draft_sha256 == sha256(draft_raw),
        "전소거 입력 결속 오류"
    );
    ensure!(
        draft.entries.len() == 2 && masks.entries.len() == 2,
        "전소거 입력 정책 오류"
    );
    type LabelSpec = (
        &'static str,
        &'static str,
        usize,
        usize,
        &'static str,
        &'static str,
        &'static [(char, u8)],
        &'static [usize],
    );
    let specs: [LabelSpec; 2] = [
        (
            "all-clear-compact",
            "全消し!",
            32,
            8,
            "3882bd35066c26b0392cd4963ff9b3c151041dec34adc9d5633d137d1d9b9855",
            GALMURI,
            &[('1', 51), ('2', 42)],
            &[0x1b1d68, 0x57ff10],
        ),
        (
            "all-clear-indicator",
            "全",
            16,
            16,
            // neodgm 16px 파생 손질 글리프(OFL). 손질 내용은 마스크 `derivation`이 기록한다.
            "d61b60eccb731f8ca9c7da582e4a05a94db66b570471809950aa9a7261b941d6",
            NEODGM,
            &[('1', 51), ('2', 42), ('3', 41), ('4', 40)],
            &[0x1b1f68, 0x1a7168, 0x580110, 0x5b754c],
        ),
    ];
    let mut target = source.to_vec();
    let mut owned = BTreeSet::new();
    let mut report = Vec::new();
    for ((entry, mask), (id, original, w, size, font, license, palette, addresses)) in
        draft.entries.iter().zip(&masks.entries).zip(specs)
    {
        ensure!(
            entry.id == id
                && mask.id == id
                && entry.source_text == original
                && entry.status == "needs_review"
                && entry.korean_text == mask.text
                && !mask.text.is_empty(),
            "전소거 문안 차이"
        );
        ensure!(
            entry.width == w
                && entry.height == 16
                && mask.font_size == size
                && mask.font_sha256 == font
                && mask.license == license
                && entry.blocks.len() == addresses.len(),
            "전소거 마스크/공급 분모 차이"
        );
        // 손질 글리프(neodgm 파생)는 결정 출처와 손질 내용을 마스크에 명시해야 한다.
        ensure!(
            (license == NEODGM)
                == (mask.decision.as_deref().is_some_and(|d| !d.is_empty())
                    && !mask.derivation.is_empty()
                    && mask.derivation.iter().all(|d| !d.is_empty())),
            "전소거 파생 글리프 기록 차이"
        );
        check_mask_shape(mask, w, palette)?;
        for (block, &at) in entry.blocks.iter().zip(addresses) {
            ensure!(
                block.source == at
                    && block.source_sha256 == sha256(&source[at..at + w * 16])
                    && source[at..at + w * 16] == source[addresses[0]..addresses[0] + w * 16]
                    && source[at..at + w * 16]
                        .iter()
                        .all(|v| [0, 40, 41, 42, 51].contains(v)),
                "전소거 원본/복제 차이"
            );
            for (y, row) in mask.rows.iter().enumerate() {
                for (x, code) in row.chars().enumerate() {
                    let p = at + (y / 8 * (w / 8) + x / 8) * 64 + y % 8 * 8 + x % 8;
                    ensure!(owned.insert(p), "전소거 작성 중복");
                    target[p] = if code == '0' { 0 } else { mask.palette[&code] };
                }
            }
        }
        report.push(serde_json::json!({"id":id,"text":mask.text,"sources":addresses}));
    }
    for (i, (&a, &b)) in source.iter().zip(&target).enumerate() {
        ensure!(a == b || owned.contains(&i), "전소거 보호 범위 변경");
    }
    Ok((
        target,
        serde_json::json!({"entries":report,"masks_sha256":sha256(mask_raw),"owned_bytes":owned.len(),"distribution_eligible":false,"runtime_verification":"deferred_until_cumulative_insertion","final_write_audit":"pass"}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    const INDICATOR: &[(char, u8)] = &[('1', 51), ('2', 42), ('3', 41), ('4', 40)];
    fn indicator() -> Mask {
        let masks: Masks = serde_json::from_slice(
            &crate::managed_input::read("assets/fonts/clear-label-masks.json").unwrap(),
        )
        .unwrap();
        masks.entries.into_iter().nth(1).unwrap()
    }
    #[test]
    #[ignore = "requires assets/fonts/clear-label-masks.json"]
    fn managed_indicator_follows_outline_and_band_shape() {
        check_mask_shape(&indicator(), 16, INDICATOR).unwrap();
    }
    #[test]
    #[ignore = "requires assets/fonts/clear-label-masks.json"]
    fn indicator_rejects_broken_outline_band_or_palette() {
        let mut gap = indicator();
        gap.rows[0].replace_range(1..2, "0");
        assert!(check_mask_shape(&gap, 16, INDICATOR).is_err());
        let mut band = indicator();
        band.rows[1].replace_range(2..3, "2");
        assert!(check_mask_shape(&band, 16, INDICATOR).is_err());
        let mut palette = indicator();
        palette.palette.insert('3', 42);
        assert!(check_mask_shape(&palette, 16, INDICATOR).is_err());
    }
}
