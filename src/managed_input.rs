//! Managed build inputs that this repository does not ship: translation drafts,
//! glyph masks, wording decisions and source-text transcriptions.
//!
//! Paths are relative to the working directory, like the graphics manifests under
//! `assets/graphics/`. Run the build from a directory that holds the `assets/` and
//! `config/` inputs listed in README.md. A missing input is an error, never a skip.

use anyhow::{Context, Result};

pub(crate) fn read(path: &str) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("관리 입력 없음: {path} (README의 빌드 입력 참조)"))
}

pub(crate) fn read_string(path: &str) -> Result<String> {
    String::from_utf8(read(path)?).with_context(|| format!("관리 입력 UTF-8 오류: {path}"))
}
