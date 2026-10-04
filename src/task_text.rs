//! 태스크 안내의 두 OBJ 조각과 공용 팝업 단위를 같은 관리 마스크에서 만든다.
use crate::source::sha256;
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::collections::BTreeSet;
#[derive(Deserialize)]
struct Draft {
    source_id: String,
    source_text_sha256: String,
    policy: String,
    entries: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    id: String,
    korean_text: String,
    status: String,
    blocks: Vec<Block>,
}
#[derive(Deserialize)]
struct Block {
    source: usize,
    source_sha256: String,
    width: usize,
    height: usize,
    x: usize,
}
#[derive(Deserialize)]
struct Masks {
    policy: String,
    draft_sha256: String,
    license: String,
    fill: u8,
    edge: u8,
    entries: Vec<Mask>,
}
#[derive(Deserialize)]
struct Mask {
    id: String,
    text: String,
    width: usize,
    height: usize,
    font_sha256: String,
    font_size: usize,
    shadow: String,
    rows: Vec<String>,
}

pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    let draft_raw = &crate::managed_input::read("assets/translations/task-text.json")?;
    let raw = &crate::managed_input::read("assets/fonts/task-text-masks.json")?;
    let draft: Draft = serde_json::from_slice(draft_raw)?;
    let masks: Masks = serde_json::from_slice(raw)?;
    ensure!(
        draft.source_id == "apyj-rev0"
            && draft.policy == "development"
            && masks.policy == "development"
            && masks.draft_sha256 == sha256(draft_raw)
            && draft.source_text_sha256
                == sha256(&crate::managed_input::read("config/hud-source-text.json")?),
        "태스크 문안 결속 오류"
    );
    ensure!(
        masks.fill == 60
            && masks.edge == 51
            && masks.license == "licenses/fonts/galmuri-v2.40.3-LICENSE.txt"
            && masks.entries.len() == 6
            && draft.entries.len() == 6,
        "태스크 입력 정책/분모 오류"
    );
    let hosts: [(&str, &[usize]); 6] = [
        ("task-colors", &[0x1aeb68, 0x1aed68]),
        ("task-puyo", &[0x1aef68, 0x1af168]),
        ("task-chain", &[0x1af368, 0x1af568]),
        ("chain-unit", &[0x1b2de8, 0x1a7fe8, 0x580f90, 0x5b83cc]),
        ("puyo-unit", &[0x1af768]),
        ("color-unit", &[0x1af7e8]),
    ];
    let mut target = source.to_vec();
    let mut owned = BTreeSet::new();
    let mut reports = Vec::new();
    for (index, ((entry, mask), (id, addresses))) in draft
        .entries
        .iter()
        .zip(&masks.entries)
        .zip(hosts)
        .enumerate()
    {
        let (width, height, block_width, size, font, shadow) = if index < 3 {
            (
                64,
                16,
                32,
                10,
                "5cb68052ee0a15571747e91c20f145e24b51bb459c6cd58226fafee78d9c0b16",
                "outline",
            )
        } else {
            (
                16,
                8,
                16,
                8,
                "3882bd35066c26b0392cd4963ff9b3c151041dec34adc9d5633d137d1d9b9855",
                "down-right",
            )
        };
        ensure!(
            entry.id == id
                && mask.id == id
                && entry.status == "needs_review"
                && entry.korean_text == mask.text
                && !mask.text.is_empty(),
            "태스크 문구 일치 오류"
        );
        ensure!(
            mask.width == width
                && mask.height == height
                && mask.font_size == size
                && mask.font_sha256 == font
                && mask.shadow == shadow
                && mask.rows.len() == height
                && mask
                    .rows
                    .iter()
                    .all(|r| r.len() == width && r.bytes().all(|c| b"012".contains(&c))),
            "태스크 마스크 경계/폰트 오류"
        );
        ensure!(
            entry.blocks.len() == addresses.len(),
            "태스크 공급 분모 오류"
        );
        for (n, (block, &address)) in entry.blocks.iter().zip(addresses).enumerate() {
            let length = block_width * height;
            ensure!(
                block.source == address
                    && block.width == block_width
                    && block.height == height
                    && block.x == if index < 3 { n * 32 } else { 0 }
                    && sha256(&source[address..address + length]) == block.source_sha256
                    && source[address..address + length]
                        .iter()
                        .all(|v| [0, 51, 60].contains(v)),
                "태스크 원본 공급 결속 오류"
            );
            if index == 3 {
                ensure!(
                    source[address..address + length]
                        == source[addresses[0]..addresses[0] + length],
                    "연쇄 단위 복제 관계 변경"
                );
            }
            for p in 0..length {
                let tile = p / 64;
                let pixel = p % 64;
                let x = tile % (block_width / 8) * 8 + pixel % 8;
                let y = tile / (block_width / 8) * 8 + pixel / 8;
                ensure!(owned.insert(address + p), "태스크 블록 작성 중복");
                target[address + p] = match mask.rows[y].as_bytes()[block.x + x] {
                    b'0' => 0,
                    b'1' => 51,
                    b'2' => 60,
                    _ => unreachable!(),
                };
            }
        }
        reports.push(serde_json::json!({"id":id,"text":mask.text,"blocks":addresses,"dimensions":[width,height]}));
    }
    for (i, (&a, &b)) in source.iter().zip(&target).enumerate() {
        ensure!(a == b || owned.contains(&i), "태스크 보호 범위 변경");
    }
    Ok((
        target,
        serde_json::json!({"entries":reports,"masks_sha256":sha256(raw),"owned_bytes":owned.len(),"distribution_eligible":false,"final_write_audit":"pass"}),
    ))
}
