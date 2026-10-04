//! 전체 글자 제거 바탕과 폰트로 준비한 카드의 일괄 삽입. 미확보 카드는 명시한다.
use crate::{
    indexed_graphics::{read, text, verify_file},
    selection_cards::{offset, pointer},
    source::sha256,
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeSet, fs, path::Path};

pub fn transform(source: &[u8], characters: bool) -> Result<(Vec<u8>, serde_json::Value)> {
    crate::source::Profile::supported()?.verify(source)?;
    let (root, mask_path, draft_path, table, palette, count) = if characters {
        (
            "assets/graphics/character-surfaces",
            "assets/fonts/character-name-masks.json",
            "assets/translations/character-names.json",
            0x54ba78,
            0x2ba18c,
            17,
        )
    } else {
        (
            "assets/graphics/course-surfaces",
            "assets/fonts/course-card-masks.json",
            "assets/translations/course-cards.json",
            0x54b9dc,
            0x2bbb8c,
            6,
        )
    };
    let raw = fs::read(Path::new(root).join("manifest.json"))?;
    let manifest: serde_json::Value = serde_json::from_slice(&raw)?;
    let masks_raw = fs::read(mask_path)?;
    let masks: serde_json::Value = serde_json::from_slice(&masks_raw)?;
    let draft_raw = fs::read(draft_path)?;
    let draft: serde_json::Value = serde_json::from_slice(&draft_raw)?;
    ensure!(
        manifest["policy"] == "asset_preparation_only"
            && manifest["source"]["sha256"] == sha256(source)
            && manifest["palette_sha256"] == sha256(&source[palette..palette + 512])
            && masks["draft_sha256"] == sha256(&draft_raw),
        "카드 원본/폰트/문안 결속 오류"
    );
    // 이름표 마스크는 손질 글리프 대체표(Galmuri11 OFL 파생)의 카드별 표면 글자로 만든다.
    crate::glyph_overrides::verify(
        &masks,
        if characters {
            "character-names"
        } else {
            "course-cards"
        },
    )?;
    let mut target = source.to_vec();
    let mut seen = BTreeSet::new();
    let mut reports = Vec::new();
    for asset in manifest["assets"].as_array().context("카드 목록 누락")? {
        let index = asset["index"].as_u64().context("카드 순번 누락")? as usize;
        ensure!(index < count && seen.insert(index), "카드 순번/중복 오류");
        let start = pointer(source, table + index * 4);
        ensure!(
            asset["source_offset"] == start
                && asset["source_sha256"] == sha256(&source[start..start + 6400])
                && asset["font_mask_sha256"] == sha256(&masks_raw)
                && asset["korean_text"] == draft["cards"][index]["korean_text"],
            "카드 개별 입력 결속 오류"
        );
        let pixels = if asset["method"] == "palette_replacement_and_font" {
            ensure!(characters && index == 5, "색인 치환 대상 오류");
            restored_card(source, start, asset, &masks["cards"][index])?
        } else {
            verify_file(asset, "background")?;
            verify_file(asset, "prompt")?;
            read(
                &Path::new(root).join(text(asset, "image")?),
                text(asset, "image_sha256")?,
                text(asset, "pixel_sha256")?,
                [80, 80],
                &source[palette..palette + 512],
            )?
        };
        let mut protected = 0;
        let mut changed = 0;
        for y in 0..80 {
            for x in 0..80 {
                let editable = if characters {
                    (4..76).contains(&x) && (64..76).contains(&y) && !(index == 1 && x >= 65)
                } else {
                    y >= 64 && !(index == 4 && x < 8)
                };
                let at = start + offset(x, y);
                let pixel = pixels[y * 80 + x];
                if !editable {
                    ensure!(
                        pixel == source[at],
                        "카드 원화/영문 보호 위반: {index}/{x}/{y}"
                    );
                    // 코스 카드4의 원본 G(x<8)는 글자 마스크가 덮을 수 없다(보충 외곽선은 x=8부터).
                    if !characters && y >= 64 {
                        let mark = masks["cards"][index]["rows"][y - 64]
                            .as_str()
                            .context("카드 글자 행 누락")?;
                        ensure!(
                            mark.as_bytes().get(x) == Some(&b'0'),
                            "카드 보호 영역 글자 마스크: {index}/{x}/{y}"
                        );
                    }
                    protected += 1;
                } else {
                    let mark = masks["cards"][index]["rows"][y - 64]
                        .as_str()
                        .context("카드 글자 행 누락")?;
                    ensure!(mark.len() == 80, "카드 글자 행 너비 오류");
                    match mark.as_bytes()[x] {
                        b'0' => (),
                        b'1' => ensure!(pixel == 39, "카드 외곽선 불일치"),
                        b'2' => ensure!(pixel == 48, "카드 글자 불일치"),
                        _ => anyhow::bail!("카드 글자 값 오류"),
                    }
                }
                changed += usize::from(pixel != source[at]);
                target[at] = pixel;
            }
        }
        ensure!(
            asset["protected_pixels"] == protected,
            "카드 보호 픽셀 수 변경"
        );
        reports.push(
            serde_json::json!({"index":index,"source":start,"changed_bytes":changed,
            "protected_pixels":protected,"image_sha256":asset["image_sha256"],
            "restoration_sha256":asset["restoration_sha256"],"pixel_sha256":asset["pixel_sha256"]}),
        );
    }
    let missing: Vec<_> = (0..count).filter(|i| !seen.contains(i)).collect();
    ensure!(missing.is_empty(), "선택 카드 누락 변경");
    if characters {
        ensure!(
            manifest["missing_indices"] == serde_json::json!(missing),
            "누락 선언 불일치"
        );
    }
    Ok((
        target,
        serde_json::json!({"policy":"development_candidates","distribution_eligible":false,
        "manifest_sha256":sha256(&raw),"cards":reports,"missing_indices":missing,
        "composition":"managed_background_and_font","runtime":"not_run"}),
    ))
}

fn restored_card(
    source: &[u8],
    start: usize,
    asset: &serde_json::Value,
    mask: &serde_json::Value,
) -> Result<Vec<u8>> {
    verify_file(asset, "restoration")?;
    let restoration: serde_json::Value =
        serde_json::from_slice(&fs::read(text(asset, "restoration")?)?)?;
    ensure!(
        restoration["policy"] == "development_palette_replacement"
            && restoration["state"] == "needs_review"
            && restoration["source_offset"] == start
            && restoration["source_sha256"] == sha256(&source[start..start + 6400]),
        "색인 치환 원본 결속 오류"
    );
    let original: Vec<u8> = (0..6400)
        .map(|i| source[start + offset(i % 80, i / 80)])
        .collect();
    let mut pixels = original.clone();
    let mut seen = BTreeSet::new();
    for change in restoration["replacements"]
        .as_array()
        .context("색인 치환 누락")?
    {
        let row = change.as_array().context("색인 치환 형식 오류")?;
        ensure!(row.len() == 3, "색인 치환 길이 오류");
        let at = row[0].as_u64().context("색인 위치 누락")? as usize;
        let value = row[2].as_u64().context("대체 색인 누락")?;
        ensure!(
            at < 6400
                && (4..76).contains(&(at % 80))
                && (64..76).contains(&(at / 80))
                && seen.insert(at),
            "색인 치환 영역/중복 오류"
        );
        ensure!(
            [39, 48].contains(&original[at])
                && row[1] == original[at]
                && (value == 12 || (80..=255).contains(&value)),
            "원문 글자/대체 원화 색인 오류"
        );
        pixels[at] = value as u8;
    }
    ensure!(seen.len() == 479, "원문 글자 픽셀 개수 변경");
    ensure!(
        original
            .iter()
            .enumerate()
            .all(|(i, v)| ![39, 48].contains(v) || seen.contains(&i))
            && restoration["background_pixel_sha256"] == sha256(&pixels),
        "원문 글자 잔존/치환 바탕 변경"
    );
    let rows = mask["rows"].as_array().context("이름 자형 누락")?;
    ensure!(rows.len() == 16, "이름 자형 높이 오류");
    for (y, row) in rows.iter().enumerate() {
        let row = row.as_str().context("이름 자형 형식 오류")?;
        ensure!(row.len() == 80, "이름 자형 너비 오류");
        for (x, mark) in row.bytes().enumerate() {
            match mark {
                b'0' => (),
                b'1' | b'2' => {
                    ensure!((4..76).contains(&x) && y < 12, "이름 자형 보호 영역 침범");
                    pixels[(64 + y) * 80 + x] = if mark == b'1' { 39 } else { 48 };
                }
                _ => anyhow::bail!("이름 자형 값 오류"),
            }
        }
    }
    ensure!(
        asset["pixel_sha256"] == sha256(&pixels),
        "색인 치환 최종 픽셀 변경"
    );
    Ok(pixels)
}
