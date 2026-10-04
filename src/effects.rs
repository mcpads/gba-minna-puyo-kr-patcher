//! 일본어 전투 장식 6표현의 RGBA/8bpp 변환과 물리 복제본 작성 경계.
use crate::source::sha256;
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufReader, BufWriter},
    path::Path,
};

struct Effect {
    id: &'static str,
    side: usize,
    offsets: &'static [usize],
    colors: &'static [u8],
    client_palette: Option<usize>,
}
const EFFECTS: [Effect; 6] = [
    Effect {
        id: "win-small",
        side: 32,
        offsets: &[0x1b1568, 0x57f710],
        colors: &[39, 40, 41, 42, 51],
        client_palette: Some(5748304),
    },
    Effect {
        id: "win-large",
        side: 64,
        offsets: &[0x1a4168, 0x5b454c],
        colors: &[39, 40, 41, 42, 51, 60],
        client_palette: Some(5977676),
    },
    Effect {
        id: "lose-small",
        side: 32,
        offsets: &[0x1b1968, 0x57fb10],
        colors: &[51, 74, 75, 76, 77],
        client_palette: Some(5748304),
    },
    Effect {
        id: "lose-large",
        side: 64,
        offsets: &[0x1a5168, 0x5b554c],
        colors: &[51, 60, 71, 72, 73, 74],
        client_palette: Some(5977676),
    },
    Effect {
        id: "all-clear-large",
        side: 64,
        offsets: &[0x1a6168, 0x5b654c],
        colors: &[40, 41, 42, 51, 60],
        client_palette: Some(5977676),
    },
    Effect {
        id: "time-over-large",
        side: 64,
        offsets: &[0x1adb68],
        colors: &[51, 60, 71, 72, 73, 74],
        client_palette: None,
    },
];
fn pixel_offset(x: usize, y: usize, width: usize) -> usize {
    ((y / 8) * (width / 8) + x / 8) * 64 + y % 8 * 8 + x % 8
}
fn colors(source: &[u8], effect: &Effect) -> Result<BTreeMap<u8, [u8; 3]>> {
    let mut palettes = vec![0x2b8f8c, 0x2b8b8c, 0x2b938c, 0x2b898c];
    palettes.extend(effect.client_palette);
    let mut result = BTreeMap::new();
    for &index in effect.colors {
        let mut expected = None;
        for &base in &palettes {
            let at = base + usize::from(index) * 2;
            let color = u16::from_le_bytes(source[at..at + 2].try_into()?) & 0x7fff;
            ensure!(
                expected.is_none_or(|value| value == color),
                "효과 초기 팔레트 불일치: {}",
                effect.id
            );
            expected = Some(color);
        }
        let word = expected.context("팔레트 없음")?;
        result.insert(
            index,
            [0, 5, 10].map(|shift| (((word >> shift) & 31) * 255 / 31) as u8),
        );
    }
    let raw = &source[effect.offsets[0]..effect.offsets[0] + effect.side * effect.side];
    ensure!(
        raw.iter()
            .copied()
            .filter(|&v| v != 0)
            .collect::<BTreeSet<_>>()
            == effect.colors.iter().copied().collect(),
        "원본 효과 사용 색인 변경"
    );
    for &at in effect.offsets {
        ensure!(
            &source[at..at + raw.len()] == raw,
            "효과 물리 복제본 불일치"
        );
    }
    ensure!(
        result.values().collect::<BTreeSet<_>>().len() == result.len(),
        "중복 RGB의 모호한 색인"
    );
    Ok(result)
}
fn decode(raw: &[u8], side: usize, palette: &BTreeMap<u8, [u8; 3]>) -> Result<Vec<u8>> {
    ensure!(raw.len() == side * side, "타일 크기 오류");
    let mut rgba = vec![0; side * side * 4];
    for y in 0..side {
        for x in 0..side {
            let index = raw[pixel_offset(x, y, side)];
            if index != 0 {
                let rgb = palette.get(&index).context("타일의 미등록 색인")?;
                let at = (y * side + x) * 4;
                rgba[at..at + 3].copy_from_slice(rgb);
                rgba[at + 3] = 255;
            }
        }
    }
    Ok(rgba)
}
fn encode(rgba: &[u8], side: usize, palette: &BTreeMap<u8, [u8; 3]>) -> Result<Vec<u8>> {
    ensure!(rgba.len() == side * side * 4, "RGBA 크기 불일치");
    let lookup: BTreeMap<[u8; 3], u8> = palette.iter().map(|(&i, &rgb)| (rgb, i)).collect();
    ensure!(
        lookup.len() == palette.len() && !palette.contains_key(&0),
        "불투명 팔레트 오류"
    );
    let mut tiles = vec![0; side * side];
    for y in 0..side {
        for x in 0..side {
            let at = (y * side + x) * 4;
            let index = match rgba[at + 3] {
                0 => 0,
                255 => *lookup
                    .get(&[rgba[at], rgba[at + 1], rgba[at + 2]])
                    .context("원본 효과 색 집합 밖 픽셀")?,
                _ => anyhow::bail!("반투명 픽셀: {x},{y}"),
            };
            tiles[pixel_offset(x, y, side)] = index;
        }
    }
    Ok(tiles)
}
fn read_png(path: &Path, side: usize) -> Result<Vec<u8>> {
    let mut reader = png::Decoder::new(BufReader::new(fs::File::open(path)?)).read_info()?;
    let info = reader.info();
    ensure!(
        info.width == side as u32 && info.height == side as u32,
        "PNG 크기 불일치: {}",
        path.display()
    );
    ensure!(
        info.color_type == png::ColorType::Rgba
            && info.bit_depth == png::BitDepth::Eight
            && info.animation_control.is_none(),
        "정적 RGBA8 PNG만 지원"
    );
    let mut bytes = vec![0; reader.output_buffer_size().context("PNG 버퍼 크기 오류")?];
    let output = reader.next_frame(&mut bytes)?;
    bytes.truncate(output.buffer_size());
    Ok(bytes)
}
fn write_png(path: &Path, side: usize, rgba: &[u8]) -> Result<()> {
    let mut encoder = png::Encoder::new(
        BufWriter::new(fs::File::create_new(path)?),
        side as u32,
        side as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

pub fn export(source: &[u8], out: &Path) -> Result<serde_json::Value> {
    crate::source::Profile::supported()?.verify(source)?;
    // 모든 원본 범위를 먼저 검사하고 새 폴더만 만든다.
    let palettes = EFFECTS
        .iter()
        .map(|e| colors(source, e))
        .collect::<Result<Vec<_>>>()?;
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(out)?;
    let mut records = Vec::new();
    for (effect, palette) in EFFECTS.iter().zip(palettes) {
        let raw = &source[effect.offsets[0]..effect.offsets[0] + effect.side * effect.side];
        let path = out.join(format!("{}.png", effect.id));
        write_png(&path, effect.side, &decode(raw, effect.side, &palette)?)?;
        ensure!(
            encode(&read_png(&path, effect.side)?, effect.side, &palette)? == raw,
            "원본 PNG 왕복 불일치"
        );
        records.push(serde_json::json!({"id":effect.id,"dimensions":[effect.side,effect.side],
            "offsets":effect.offsets,"colors":palette,"tile_sha256":sha256(raw),"roundtrip":"pass"}));
    }
    let report = serde_json::json!({"kind":"original_effect_references","korean_assets":false,"effects":records,
        "scope":"일본어 원본 참조. 커밋·한국어 채택 입력 아님"});
    fs::write(out.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    Ok(report)
}

/// 관리된 한국어 PNG의 해시와 개발 입력 상태를 확인한다.
pub fn transform_managed(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let directory = Path::new("assets/graphics/effects");
    let raw = fs::read(directory.join("manifest.json"))?;
    let manifest: serde_json::Value = serde_json::from_slice(&raw)?;
    ensure!(
        manifest["source_profile"] == "apyj-rev0"
            && manifest["policy"] == "development_selected_effects"
            && manifest["distribution_eligible"] == false,
        "관리 효과 입력 정책 불일치"
    );
    ensure!(
        manifest["wording_sha256"] == sha256(&fs::read("assets/translations/effects-draft.json")?),
        "효과 문안 결속 불일치"
    );
    let assets = manifest["assets"].as_array().context("효과 목록 없음")?;
    ensure!(assets.len() == EFFECTS.len(), "효과 표현 수 불일치");
    for (asset, effect) in assets.iter().zip(&EFFECTS) {
        let path = directory.join(format!("{}.png", effect.id));
        ensure!(
            asset["id"] == effect.id
                && asset["state"] == "needs_review"
                && asset["engine_input"] == true
                && asset["image"] == path.to_string_lossy().as_ref()
                && asset["target_dimensions"] == serde_json::json!([effect.side, effect.side])
                && asset["image_sha256"] == sha256(&fs::read(&path)?),
            "관리 효과 이미지 결속 불일치: {}",
            effect.id
        );
    }
    let (target, mut report) = transform(source, directory)?;
    report["manifest_sha256"] = sha256(&raw).into();
    report["policy"] = "development_selected_effects".into();
    report["runtime"] = "not_run".into();
    Ok((target, report))
}

/// 명시적으로 선택한 개발 PNG만 변환한다. 리사이즈·색 근사·승인 자동 승격은 없다.
pub fn transform(source: &[u8], directory: &Path) -> Result<(Vec<u8>, serde_json::Value)> {
    crate::source::Profile::supported()?.verify(source)?;
    let mut target = source.to_vec();
    let mut allowed = BTreeSet::new();
    let mut records = Vec::new();
    for effect in &EFFECTS {
        let palette = colors(source, effect)?;
        let path = directory.join(format!("{}.png", effect.id));
        let rgba = read_png(&path, effect.side)?;
        let tiles = encode(&rgba, effect.side, &palette)?;
        let back = decode(&tiles, effect.side, &palette)?;
        ensure!(
            rgba.as_chunks::<4>()
                .0
                .iter()
                .zip(back.as_chunks::<4>().0.iter())
                .all(|(a, b)| a == b || (a[3] == 0 && b[3] == 0)),
            "효과 픽셀 왕복 불일치"
        );
        for &at in effect.offsets {
            for byte in at..at + tiles.len() {
                ensure!(allowed.insert(byte), "효과 작성 범위 충돌");
            }
            target[at..at + tiles.len()].copy_from_slice(&tiles);
        }
        records.push(
            serde_json::json!({"id":effect.id,"input_sha256":sha256(&fs::read(path)?),
            "tile_sha256":sha256(&tiles),"offsets":effect.offsets,
            "same_as_original":tiles==source[effect.offsets[0]..effect.offsets[0]+tiles.len()]}),
        );
    }
    let mut changed = 0;
    for (at, (&a, &b)) in source.iter().zip(&target).enumerate() {
        if a != b {
            ensure!(allowed.contains(&at), "효과 보호 범위 침범");
            changed += 1;
        }
    }
    Ok((
        target,
        serde_json::json!({"effects":records,"physical_writers":11,"changed_bytes":changed,
        "final_write_audit":"pass","policy":"development","distribution_eligible":false}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_translucency_and_unapproved_colors_but_ignores_hidden_rgb() {
        let palette = BTreeMap::from([(51, [65, 65, 65])]);
        let mut pixels = vec![0; 8 * 8 * 4];
        pixels[..3].copy_from_slice(&[123, 34, 56]);
        assert_eq!(encode(&pixels, 8, &palette).unwrap(), vec![0; 64]);
        pixels[3] = 128;
        assert!(encode(&pixels, 8, &palette).is_err());
        pixels[3] = 255;
        assert!(encode(&pixels, 8, &palette).is_err());
        pixels[..3].copy_from_slice(&[65, 65, 65]);
        assert_eq!(encode(&pixels, 8, &palette).unwrap()[0], 51);
    }
    #[test]
    fn tile_boundaries_use_gba_row_order() {
        assert_eq!(pixel_offset(8, 0, 32), 64);
        assert_eq!(pixel_offset(0, 8, 32), 256);
        assert_eq!(pixel_offset(31, 31, 32), 1023);
    }
}
