use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub size: usize,
    pub sha256: String,
    pub title: String,
    pub game_code: String,
    pub revision: u8,
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl Profile {
    pub fn supported() -> Result<Self> {
        Ok(serde_json::from_str(include_str!("../config/source.json"))?)
    }

    pub fn verify(&self, bytes: &[u8]) -> Result<()> {
        ensure!(bytes.len() >= 0xc0, "GBA 헤더가 잘렸습니다");
        ensure!(bytes.len() == self.size, "지원 원본 크기 불일치");
        ensure!(sha256(bytes) == self.sha256, "지원 원본 SHA-256 불일치");
        ensure!(
            bytes[0xa0..0xac]
                .strip_suffix(&[0])
                .unwrap_or(&bytes[0xa0..0xac])
                == self.title.as_bytes(),
            "타이틀 불일치"
        );
        ensure!(
            &bytes[0xac..0xb0] == self.game_code.as_bytes(),
            "게임 코드 불일치"
        );
        ensure!(bytes[0xbc] == self.revision, "리비전 불일치");
        let checksum = bytes[0xa0..0xbd]
            .iter()
            .fold(0u8, |a, &b| a.wrapping_sub(b))
            .wrapping_sub(0x19);
        ensure!(bytes[0xbd] == checksum, "GBA 헤더 체크섬 불일치");
        Ok(())
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
