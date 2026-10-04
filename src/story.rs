//! 일반 12×3 대사창의 개발 번역. 원본 비문자 제어와 스트림은 보존한다.
use crate::{source::sha256, text};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
pub struct Story {
    source_id: String,
    policy: String,
    status: String,
    pub streams: Vec<Stream>,
}
#[derive(Deserialize)]
pub struct Stream {
    id: String,
    pub index: usize,
    source_stream_sha256: String,
    pages: Vec<Page>,
}
#[derive(Deserialize)]
struct Page {
    id: String,
    lines: Vec<String>,
}
pub struct Encoded {
    pub index: usize,
    pub original: usize,
    pub bytes: Vec<u8>,
    pub report: serde_json::Value,
}
pub fn load() -> Result<Story> {
    let s: Story = serde_json::from_str(&crate::managed_input::read_string(
        "assets/translations/story-draft.json",
    )?)?;
    ensure!(
        s.source_id == "apyj-rev0" && s.policy == "development" && s.status == "needs_review",
        "이야기 개발 정책 오류"
    );
    let mut ids = BTreeSet::new();
    for stream in &s.streams {
        ensure!(
            (1..84).contains(&stream.index)
                && stream.id == format!("ja-{:02}", stream.index)
                && ids.insert(stream.index),
            "이야기 스트림 ID/중복 오류"
        );
        let mut pages = BTreeSet::new();
        for p in &stream.pages {
            ensure!(
                pages.insert(&p.id) && !p.lines.is_empty() && p.lines.len() <= 3,
                "이야기 페이지 ID/행 수 오류"
            );
            for line in &p.lines {
                ensure!(
                    // STAGE/VS 안내의 들여쓰기와 공백 한 칸인 중간 행도 문자다.
                    !line.is_empty() && line.chars().count() <= 12,
                    "이야기 12열 초과/빈 행: {}",
                    p.id
                );
            }
        }
    }
    Ok(s)
}
impl Story {
    pub fn repertoire(&self) -> BTreeSet<char> {
        self.streams
            .iter()
            .flat_map(|s| s.pages.iter())
            .flat_map(|p| p.lines.iter())
            .flat_map(|s| s.chars())
            .collect()
    }
    pub fn encode(&self, source: &[u8], codes: &BTreeMap<char, u16>) -> Result<Vec<Encoded>> {
        let mut output = Vec::new();
        for s in &self.streams {
            let p = 0x555f28 + s.index * 4;
            let start = u32::from_le_bytes(source[p..p + 4].try_into()?) as usize - 0x08000000;
            ensure!(
                (0x54c94c..0x555f26).contains(&start),
                "이야기 포인터 범위 오류"
            );
            let (tokens, end) = text::parse(source, start, 0x555f26)?;
            ensure!(
                sha256(&source[start..end]) == s.source_stream_sha256,
                "이야기 원문 해시 오류"
            );
            let mut runs: Vec<(usize, usize)> = Vec::new();
            for t in &tokens {
                if t.text_or_newline() {
                    if let Some(last) = runs.last_mut().filter(|v| v.1 == t.offset) {
                        last.1 += 2;
                    } else {
                        runs.push((t.offset, t.offset + 2));
                    }
                }
            }
            ensure!(runs.len() == s.pages.len(), "이야기 문자 구간 분모 오류");
            let mut bytes = Vec::new();
            let mut cursor = start;
            for (&(first, last), page) in runs.iter().zip(&s.pages) {
                bytes.extend_from_slice(&source[cursor..first]);
                for line in &page.lines {
                    for c in line.chars() {
                        bytes.extend_from_slice(
                            &codes
                                .get(&c)
                                .with_context(|| format!("이야기 문자 누락: {c}"))?
                                .to_le_bytes(),
                        );
                    }
                    bytes.extend_from_slice(&0xff80u16.to_le_bytes());
                }
                cursor = last;
            }
            bytes.extend_from_slice(&source[cursor..end]);
            let (new_tokens, new_end) = text::parse(&bytes, 0, bytes.len())?;
            let mut cursor = 0usize;
            let mut window_open = false;
            for token in &new_tokens {
                match token.code {
                    0xff0e => {
                        ensure!(token.args == [0], "이야기 일반창 이외 형식");
                        window_open = true;
                        cursor = 0;
                    }
                    0xff0d => {
                        ensure!(window_open, "창 설정 전 지우기");
                        cursor = 0;
                    }
                    0xff80 => cursor = cursor.div_ceil(12) * 12,
                    0..=0xfeff => {
                        ensure!(window_open && cursor < 36, "이야기 실제 커서 범위 초과");
                        cursor += 1;
                    }
                    _ => {}
                }
            }
            ensure!(
                new_end == bytes.len() && text::protected(&tokens) == text::protected(&new_tokens),
                "이야기 보호 제어 변경"
            );
            let reverse: BTreeMap<_, _> = codes.iter().map(|(&c, &n)| (n, c)).collect();
            let decoded: String = new_tokens
                .iter()
                .filter(|t| t.code < 0xff00)
                .map(|t| reverse.get(&t.code).copied().context("재추출 문자 누락"))
                .collect::<Result<_>>()?;
            let authored = s
                .pages
                .iter()
                .flat_map(|p| p.lines.iter())
                .cloned()
                .collect::<String>();
            ensure!(decoded == authored, "이야기 재추출 문안 불일치");
            output.push(Encoded{index:s.index,original:start,report:serde_json::json!({"id":s.id,"pages":s.pages.len(),"source_start":start,"source_end":end,"bytes":bytes.len(),"protected_controls":text::protected(&tokens).len(),"reextracted_wording":"pass"}),bytes});
        }
        Ok(output)
    }
}
