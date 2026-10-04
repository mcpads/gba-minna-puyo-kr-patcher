//! 1인용 직접 OAM 제목의 원래 두 조각과 색을 유지한다.
use crate::source::sha256;
use anyhow::{Result, ensure};
use serde_json::{Value, json};

pub fn transform(source: &[u8]) -> Result<(Vec<u8>, Value)> {
    let draft_raw = &crate::managed_input::read("assets/translations/pause-heading.json")?;
    let mask_raw = &crate::managed_input::read("assets/fonts/pause-heading-masks.json")?;
    let draft: Value = serde_json::from_slice(draft_raw)?;
    let mask: Value = serde_json::from_slice(mask_raw)?;
    let start = 0x1a8a68;
    let end = start + 1024;
    ensure!(
        draft["source_id"] == "apyj-rev0"
            && draft["policy"] == "development"
            && draft["status"] == "needs_review"
            && draft["source"] == start
            && draft["source_text"] == "きゅーけい"
            && draft["source_sha256"] == sha256(&source[start..end])
            && mask["draft_sha256"] == sha256(draft_raw)
            && mask["policy"] == "development"
            && mask["text"] == draft["korean_text"]
            && mask["text"].as_str().is_some_and(|s| !s.is_empty()),
        "일시정지 문안 결속 오류"
    );
    ensure!(
        mask["width"] == 64
            && mask["height"] == 16
            && mask["font_size"] == 12
            && mask["font_sha256"]
                == "2c709890595668f7bdb6df408420fda957dde0288e95b31a1cc17a2ab98b4b4f"
            && mask["license"] == "licenses/fonts/galmuri-v2.40.3-LICENSE.txt"
            && mask["fill"] == 43
            && mask["edge"] == 37,
        "일시정지 조판/폰트 오류"
    );
    ensure!(
        source[start..end]
            .iter()
            .all(|v| [0, 37, 38, 40, 41, 42, 43].contains(v)),
        "일시정지 원본 색 변경"
    );
    for c in [37, 43] {
        ensure!(
            source[0x2b938c + c * 2..0x2b938e + c * 2]
                == source[0x2b898c + c * 2..0x2b898e + c * 2],
            "일시정지 팔레트 차이"
        );
    }
    let rows = mask["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("마스크 행 누락"))?;
    ensure!(rows.len() == 16, "일시정지 높이 오류");
    let mut target = source.to_vec();
    for (y, row) in rows.iter().enumerate() {
        let row = row
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("마스크 행 형식 오류"))?;
        ensure!(
            row.len() == 64 && row.bytes().all(|v| b"012".contains(&v)),
            "일시정지 마스크 오류"
        );
        for (x, c) in row.bytes().enumerate() {
            let p =
                start + (x / 32) * 512 + ((y / 8) * 4 + (x % 32) / 8) * 64 + (y % 8) * 8 + x % 8;
            target[p] = match c {
                b'0' => 0,
                b'1' => 37,
                b'2' => 43,
                _ => unreachable!(),
            };
        }
    }
    ensure!(
        source[..start] == target[..start] && source[end..] == target[end..],
        "일시정지 보호 범위 변경"
    );
    Ok((
        target,
        json!({"text":mask["text"],"masks_sha256":sha256(mask_raw),"owned_bytes":1024,"distribution_eligible":false,"final_write_audit":"pass"}),
    ))
}
