//! 검토용 색인 PNG를 원본 팔레트와 결속해 읽는다. 생성/분석 캐시는 사용하지 않는다.
use crate::source::sha256;
use anyhow::{Context, Result, ensure};
use std::{fs, io::Cursor, path::Path};

pub fn read(
    path: &Path,
    expected_sha: &str,
    pixel_sha: &str,
    size: [usize; 2],
    palette: &[u8],
) -> Result<Vec<u8>> {
    let raw = fs::read(path)?;
    ensure!(
        sha256(&raw) == expected_sha,
        "색인 이미지 해시 변경: {}",
        path.display()
    );
    let mut reader = png::Decoder::new(Cursor::new(raw)).read_info()?;
    let info = reader.info();
    ensure!(
        info.width as usize == size[0]
            && info.height as usize == size[1]
            && info.color_type == png::ColorType::Indexed
            && info.bit_depth == png::BitDepth::Eight
            && info.trns.is_none()
            && info.animation_control.is_none(),
        "색인 PNG 형식 오류"
    );
    let colors: Vec<u8> = palette
        .as_chunks::<2>()
        .0
        .iter()
        .flat_map(|pair| {
            let word = u16::from_le_bytes([pair[0], pair[1]]) as u32;
            [0, 5, 10].map(move |shift| (((word >> shift) & 31) * 255 / 31) as u8)
        })
        .collect();
    ensure!(
        info.palette.as_deref() == Some(colors.as_slice()),
        "원본 팔레트 불일치"
    );
    let mut pixels = vec![0; reader.output_buffer_size().context("PNG 크기 누락")?];
    let frame = reader.next_frame(&mut pixels)?;
    ensure!(
        frame.buffer_size() == size[0] * size[1],
        "PNG 픽셀 길이 오류"
    );
    pixels.truncate(frame.buffer_size());
    ensure!(sha256(&pixels) == pixel_sha, "색인 픽셀 해시 변경");
    Ok(pixels)
}

pub fn text<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("문자열 필드 누락: {key}"))
}

pub fn verify_file(value: &serde_json::Value, key: &str) -> Result<()> {
    let path = text(value, key)?;
    ensure!(
        sha256(&fs::read(path)?) == text(value, &format!("{key}_sha256"))?,
        "자산 결속 변경: {path}"
    );
    Ok(())
}
