//! 선택 카드의 원본6조각 픽셀 주소 대응.
pub(crate) fn offset(x: usize, y: usize) -> usize {
    let (base, px, py, width) = if y < 64 && x < 64 {
        (0, 0, 0, 64)
    } else if y < 32 {
        (4096, 64, 0, 16)
    } else if y < 64 {
        (4608, 64, 32, 16)
    } else if x < 32 {
        (5120, 0, 64, 32)
    } else if x < 64 {
        (5632, 32, 64, 32)
    } else {
        (6144, 64, 64, 16)
    };
    let (x, y) = (x - px, y - py);
    base + (y / 8 * (width / 8) + x / 8) * 64 + y % 8 * 8 + x % 8
}
pub(crate) fn pointer(source: &[u8], at: usize) -> usize {
    u32::from_le_bytes(source[at..at + 4].try_into().unwrap()) as usize - 0x08000000
}
