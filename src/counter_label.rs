//! HUD31의4물리 복제본에 크기를 유지한 상쇄 문구를 쓴다.
use crate::source::sha256;
use anyhow::{Result, ensure};
use serde::Deserialize;
#[derive(Deserialize)]
struct Label {
    source_id: String,
    policy: String,
    source_wording_sha256: String,
    source_text: String,
    korean_text: String,
    status: String,
    consumer_status: String,
    font_sha256: String,
    font_size: usize,
    license: String,
    width: usize,
    height: usize,
    edge: u8,
    fill: u8,
    blocks: Vec<Block>,
    rows: Vec<String>,
}
#[derive(Deserialize)]
struct Block {
    source: usize,
    source_sha256: String,
}
pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let raw = &crate::managed_input::read("assets/fonts/counter-label.json")?;
    let m: Label = serde_json::from_slice(raw)?;
    ensure!(
        m.source_id == "apyj-rev0"
            && m.policy == "development"
            && m.source_wording_sha256
                == sha256(&crate::managed_input::read("config/hud-source-text.json")?)
            && m.source_text == "相殺"
            && m.korean_text == "상쇄"
            && m.status == "needs_review"
            && m.consumer_status == "unresolved"
            && m.width == 16
            && m.height == 8
            && m.font_sha256 == "3882bd35066c26b0392cd4963ff9b3c151041dec34adc9d5633d137d1d9b9855"
            && m.font_size == 8
            && m.license == "licenses/fonts/galmuri-v2.40.3-LICENSE.txt"
            && m.edge == 51
            && m.fill == 60
            && m.blocks.len() == 4
            && m.rows.len() == 8
            && m.rows
                .iter()
                .all(|r| r.len() == 16 && r.bytes().all(|c| b"012".contains(&c))),
        "상쇄 문구 입력 결속/범위 오류"
    );
    let mut target = source.to_vec();
    for (block, address) in m
        .blocks
        .iter()
        .zip([0x1b2d68, 0x1a7f68, 0x580f10, 0x5b834c])
    {
        ensure!(
            block.source == address
                && sha256(&source[address..address + 128]) == block.source_sha256
                && source[address..address + 128] == source[0x1b2d68..0x1b2de8]
                && source[address..address + 128]
                    .iter()
                    .all(|v| [0, 51, 60].contains(v)),
            "상쇄 원본/복제 관계 오류"
        );
        for p in 0..128 {
            let x = p / 64 * 8 + p % 8;
            let y = p % 64 / 8;
            target[address + p] = match m.rows[y].as_bytes()[x] {
                b'0' => 0,
                b'1' => m.edge,
                _ => m.fill,
            };
        }
    }
    let changed = source.iter().zip(&target).filter(|(a, b)| a != b).count();
    Ok((
        target,
        serde_json::json!({"references":5,"physical_blocks":4,"owned_bytes":512,"changed_bytes":changed,
        "masks_sha256":sha256(raw),"consumer_status":"unresolved","distribution_eligible":false,
        "runtime_verification":"deferred_until_cumulative_insertion"}),
    ))
}
