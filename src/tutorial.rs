//! 원본 제어를 보존한 튜토리얼 초안·추가 글리프 재배치. 명시적 개발 입력이다.
use crate::{source::sha256, text};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const DRAFT_INPUT: &str = "config/tutorial-korean-draft.json";
const MASKS_INPUT: &str = "assets/fonts/dialogue-glyph-masks.json";
const MAPPING_INPUT: &str = "config/source-glyphs.json";
const FONT: usize = 0x26a3e8;
const ORIGINAL_SLOTS: usize = 384;
const EXPANSION: usize = 0x800000;
const SIZE: usize = 0x1000000;

#[derive(Deserialize)]
struct Draft {
    source_id: String,
    stream_id: String,
    source_stream_sha256: String,
    status: String,
    pages: Vec<Page>,
}
#[derive(Deserialize)]
struct Page {
    id: String,
    lines: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Font {
    status: String,
    font_sha256: String,
    size_px: usize,
    baseline: usize,
    license: String,
    glyphs: Vec<Glyph>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Glyph {
    character: char,
    rows: Vec<String>,
}

pub fn transform(source: &[u8]) -> Result<(Vec<u8>, serde_json::Value)> {
    crate::source::Profile::supported()?.verify(source)?;
    let draft_text = crate::managed_input::read_string(DRAFT_INPUT)?;
    let masks_text = crate::managed_input::read_string(MASKS_INPUT)?;
    let mapping_text = crate::managed_input::read_string(MAPPING_INPUT)?;
    let draft: Draft = serde_json::from_str(&draft_text)?;
    let font: Font = serde_json::from_str(&masks_text)?;
    ensure!(
        draft.source_id == "apyj-rev0"
            && draft.stream_id == "ja-00"
            && draft.status == "needs_review",
        "튜토리얼 개발 입력 식별/상태 오류"
    );
    ensure!(
        font.status == "development" && font.size_px == 16 && font.baseline == 12,
        "개발 폰트 조건 변경"
    );
    ensure!(draft.pages.len() == 13, "튜토리얼 본문 분모 변경");
    let mut repertoire = BTreeSet::new();
    let story = crate::story::load()?;
    repertoire.extend(story.repertoire());
    for (i, page) in draft.pages.iter().enumerate() {
        ensure!(
            page.id == format!("tutorial-body-{:02}", i + 1),
            "본문 ID/순서 오류"
        );
        ensure!(
            !page.lines.is_empty() && page.lines.len() <= 7,
            "본문 행 수 초과"
        );
        for line in &page.lines {
            ensure!(
                !line.is_empty() && line.chars().count() <= 6 && line.trim() == line,
                "빈 행/행 끝 공백/6열 초과: {}",
                page.id
            );
            repertoire.extend(line.chars());
        }
    }
    let required: BTreeSet<char> = repertoire
        .iter()
        .copied()
        .filter(|&c| supplied_char(c))
        .collect();
    // 관리 마스크 순서가 코드 순서다. 미사용 기존 글자도 남겨 코드 이동을 피한다.
    let added: Vec<char> = font.glyphs.iter().map(|g| g.character).collect();
    validate_glyph_order(&added, &required)?;
    ensure!(
        ORIGINAL_SLOTS + added.len() < 0xff00,
        "폰트와 제어코드 충돌"
    );
    let mapping: serde_json::Value = serde_json::from_str(&mapping_text)?;
    let ja = &mapping["languages"]["ja"];
    ensure!(
        ja["font_sha256"] == sha256(&source[FONT..FONT + ORIGINAL_SLOTS * 256]),
        "원본 글리프 매핑 해시 불일치"
    );
    let mut original_codes = BTreeMap::new();
    for (hex, value) in ja["by_code"].as_object().context("원본 코드표 없음")? {
        let code = u16::from_str_radix(hex, 16)?;
        let spelling = value.as_str().context("원본 문자 값 오류")?;
        let mut chars = spelling.chars();
        let ch = chars.next().context("빈 원본 문자")?;
        ensure!(chars.next().is_none(), "복수 문자 원본 매핑");
        original_codes
            .entry(ch)
            .and_modify(|old: &mut u16| *old = (*old).min(code))
            .or_insert(code);
    }
    let mut codes = BTreeMap::new();
    for (i, &ch) in added.iter().enumerate() {
        codes.insert(ch, (ORIGINAL_SLOTS + i) as u16);
    }
    for &ch in &repertoire {
        if let std::collections::btree_map::Entry::Vacant(entry) = codes.entry(ch) {
            let code = *original_codes
                .get(&ch)
                .with_context(|| format!("미지원 문자: {ch}"))?;
            ensure!(
                usize::from(code) < ORIGINAL_SLOTS,
                "원본 문자 코드 범위 초과"
            );
            entry.insert(code);
        }
    }
    let mut font_bytes = source[FONT..FONT + ORIGINAL_SLOTS * 256].to_vec();
    ensure!(
        font_bytes[..256].iter().all(|&b| b == 0),
        "공백 글리프 변경"
    );
    for glyph in &font.glyphs {
        ensure!(
            glyph.rows.len() == 16
                && glyph
                    .rows
                    .iter()
                    .all(|r| r.len() == 16 && r.bytes().all(|b| b == b'0' || b == b'1')),
            "16×16 글리프 마스크 오류"
        );
        ensure!(glyph.rows.iter().any(|r| r.contains('1')), "빈 추가 글리프");
        validate_digit_placement(glyph.character, &glyph.rows)?;
        for ty in [0, 8] {
            for tx in [0, 8] {
                for y in 0..8 {
                    for x in 0..8 {
                        font_bytes.push(if glyph.rows[ty + y].as_bytes()[tx + x] == b'1' {
                            13
                        } else {
                            0
                        });
                    }
                }
            }
        }
    }
    let old_start = 0x54c94c;
    let (tokens, old_end) = text::parse(source, old_start, 0x555f26)?;
    ensure!(
        sha256(&source[old_start..old_end]) == draft.source_stream_sha256,
        "튜토리얼 원문 해시 불일치"
    );
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for token in &tokens {
        if token.text_or_newline() {
            if let Some(last) = runs.last_mut().filter(|last| last.1 == token.offset) {
                last.1 += 2;
            } else {
                runs.push((token.offset, token.offset + 2));
            }
        }
    }
    ensure!(
        runs.len() == draft.pages.len(),
        "문자 연속 구간/본문 수 불일치"
    );
    let mut stream = Vec::new();
    let mut cursor = old_start;
    for (&(first, end), page) in runs.iter().zip(&draft.pages) {
        stream.extend_from_slice(&source[cursor..first]);
        for line in &page.lines {
            for ch in line.chars() {
                stream.extend_from_slice(&codes[&ch].to_le_bytes());
            }
            stream.extend_from_slice(&0xff80u16.to_le_bytes());
        }
        cursor = end;
    }
    stream.extend_from_slice(&source[cursor..old_end]);
    let (new_tokens, new_end) = text::parse(&stream, 0, stream.len())?;
    ensure!(
        new_end == stream.len() && text::protected(&tokens) == text::protected(&new_tokens),
        "보호 제어·인자·순서 변경"
    );
    let reverse: BTreeMap<u16, char> = codes.iter().map(|(&c, &code)| (code, c)).collect();
    let decoded: String = new_tokens
        .iter()
        .filter(|t| t.code < 0xff00)
        .map(|t| {
            reverse
                .get(&t.code)
                .copied()
                .context("재추출 코드 매핑 누락")
        })
        .collect::<Result<_>>()?;
    let authored = draft
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .cloned()
        .collect::<String>();
    ensure!(decoded == authored, "재추출 문안 불일치");
    let stream_at = (EXPANSION + font_bytes.len() + 3) & !3;
    // 반각 숫자(시안 A)는 튜토리얼 본문에만 쓴다. 이야기 창은 STAGE 표제·두 자리 번호의 원본 숫자를 유지한다.
    let story_codes = story_code_table(&codes, &original_codes)?;
    let encoded_stories = story.encode(source, &story_codes)?;
    let mut tail = (stream_at + stream.len() + 3) & !3;
    let mut story_positions = Vec::new();
    for encoded in &encoded_stories {
        story_positions.push(tail);
        tail = (tail + encoded.bytes.len() + 3) & !3;
    }
    ensure!(tail <= 0x900000, "대사 전용 확장 영역 용량 초과");
    let mut target = source.to_vec();
    let mut writes = Vec::new();
    let mut write_word = |at: usize, before: u32, after: u32| -> Result<()> {
        ensure!(
            source[at..at + 4] == before.to_le_bytes(),
            "원본 기대 워드 불일치: {at:X}"
        );
        ensure!(
            !writes
                .iter()
                .any(|&(other, _, _)| at < other + 4 && other < at + 4),
            "작성 범위 충돌"
        );
        target[at..at + 4].copy_from_slice(&after.to_le_bytes());
        writes.push((at, before, after));
        Ok(())
    };
    for (before, positions) in [
        (0x0826a3e8, vec![0xa620, 0xa81c, 0xa8f4, 0xac94, 0xae38]),
        (0x082723e8, vec![0xaca8, 0xae4c]),
        (0x0827a3e8, vec![0xacd8, 0xae7c]),
    ] {
        for at in positions {
            write_word(at, before, 0x08800000 + before - 0x0826a3e8)?;
        }
    }
    for at in [0xacd0, 0xae74] {
        write_word(at, 383, (font_bytes.len() / 256 - 1) as u32)?;
    }
    write_word(0x555f28, 0x0854c94c, 0x08000000 + stream_at as u32)?;
    for (s, &at) in encoded_stories.iter().zip(&story_positions) {
        write_word(
            0x555f28 + s.index * 4,
            0x08000000 + s.original as u32,
            0x08000000 + at as u32,
        )?;
    }
    ensure!(
        writes.len() == 12 + encoded_stories.len(),
        "대사 작성 필드 수 변경"
    );
    for (i, (&old, &new)) in source.iter().zip(&target).enumerate() {
        ensure!(
            old == new || writes.iter().any(|&(at, _, _)| (at..at + 4).contains(&i)),
            "원본 보호 범위 침범"
        );
    }
    target.resize(SIZE, 0xff);
    target[EXPANSION..EXPANSION + font_bytes.len()].copy_from_slice(&font_bytes);
    target[stream_at..stream_at + stream.len()].copy_from_slice(&stream);
    for (s, &at) in encoded_stories.iter().zip(&story_positions) {
        target[at..at + s.bytes.len()].copy_from_slice(&s.bytes);
    }
    // 개발 입력에 포함되지 않은 양언어 스트림은 원본 경계와 포인터로 검증한다.
    let mut unchanged = 0;
    for (root, low, high) in [
        (0x555f28, 0x54c94c, 0x555f26),
        (0x565120, 0x556078, 0x565120),
    ] {
        for index in 0..84 {
            if root == 0x555f28 && (index == 0 || encoded_stories.iter().any(|s| s.index == index))
            {
                continue;
            }
            let at = root + index * 4;
            ensure!(
                source[at..at + 4] == target[at..at + 4],
                "비대상 대사 포인터 변경"
            );
            let ptr = u32::from_le_bytes(source[at..at + 4].try_into()?) as usize - 0x08000000;
            ensure!((low..high).contains(&ptr), "대사 포인터 범위 오류");
            let (_, end) = text::parse(source, ptr, high)?;
            ensure!(source[ptr..end] == target[ptr..end], "비대상 대사 변경");
            unchanged += 1;
        }
    }
    let report = serde_json::json!({"pages":13,"font_slots":font_bytes.len()/256,
        "expanded_used_end":tail,"font_bytes":font_bytes.len(),"font_offset":EXPANSION,"stream_offset":stream_at,
        "stream_bytes":stream.len(),"added_glyphs":added.len(),"unchanged_streams":unchanged,
        "protected_controls":text::protected(&new_tokens).len(),"writes":writes,
        "source_stream_preserved":source[old_start..old_end]==target[old_start..old_end],
        "draft_sha256":sha256(draft_text.as_bytes()),"masks_sha256":sha256(masks_text.as_bytes()),
        "mapping_sha256":sha256(mapping_text.as_bytes()),"font_sha256":font.font_sha256,"license":font.license,
        "masks_input":"assets/fonts/dialogue-glyph-masks.json","story_draft_sha256":sha256(&crate::managed_input::read("assets/translations/story-draft.json")?),"stories":encoded_stories.iter().zip(&story_positions).map(|(s,&at)|serde_json::json!({"stream":s.report,"offset":at})).collect::<Vec<_>>(),
        "story_digit_codes":"original","final_write_audit":"pass","reextracted_wording":"pass","distribution_eligible":false});
    Ok((target, report))
}

/// 이야기 창용 코드표: 숫자만 원본 코드로 되돌린다.
fn story_code_table(
    codes: &BTreeMap<char, u16>,
    original: &BTreeMap<char, u16>,
) -> Result<BTreeMap<char, u16>> {
    let mut table = codes.clone();
    for (ch, code) in table.iter_mut().filter(|(ch, _)| ch.is_ascii_digit()) {
        let source = *original
            .get(ch)
            .with_context(|| format!("원본 숫자 코드 없음: {ch}"))?;
        ensure!(
            usize::from(source) < ORIGINAL_SLOTS,
            "원본 숫자 코드 범위 오류"
        );
        *code = source;
    }
    Ok(table)
}

/// 관리 마스크로 공급하는 문자. 숫자는 사용자 채택 시안 A의 반각 글리프다.
fn supplied_char(c: char) -> bool {
    matches!(c, '\u{ac00}'..='\u{d7a3}' | '.' | ',' | '!' | '0'..='9')
}

/// 숫자 마스크는 반각 폭을 칸 오른쪽(x 7..16)에 둔다. 16px 고정 칸에서 뒤 음절과 붙여 읽히게 한다.
const DIGIT_INK_START: usize = 7;

fn validate_digit_placement(character: char, rows: &[String]) -> Result<()> {
    if character.is_ascii_digit() {
        ensure!(
            rows.iter().all(|r| !r[..DIGIT_INK_START].contains('1')),
            "반각 숫자 배치 오류: {character}"
        );
    }
    Ok(())
}

fn validate_glyph_order(order: &[char], required: &BTreeSet<char>) -> Result<()> {
    let supplied: BTreeSet<char> = order.iter().copied().collect();
    ensure!(supplied.len() == order.len(), "추가 글리프 중복");
    ensure!(required.is_subset(&supplied), "필수 추가 글리프 누락");
    ensure!(
        supplied.iter().all(|&c| supplied_char(c)),
        "추가 글리프 허용 문자 범위 오류"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appended_glyphs_keep_existing_codes() {
        let order = ['힐', '옆', '릭', '출'];
        let required = BTreeSet::from(['힐', '릭', '출']);
        assert!(validate_glyph_order(&order, &required).is_ok());
    }

    #[test]
    fn malformed_glyph_inventory_is_rejected() {
        let required = BTreeSet::from(['릭']);
        for order in [vec!['출'], vec!['릭', '릭'], vec!['릭', 'A']] {
            assert!(validate_glyph_order(&order, &required).is_err());
        }
    }

    #[test]
    fn digits_are_supplied_as_right_placed_halfwidth_glyphs() {
        let required = BTreeSet::from(['릭', '2']);
        assert!(validate_glyph_order(&['릭', '2'], &required).is_ok());
        let right: Vec<String> = (0..16).map(|_| "0000000011110000".to_string()).collect();
        let left: Vec<String> = (0..16).map(|_| "0011110000000000".to_string()).collect();
        assert!(validate_digit_placement('2', &right).is_ok());
        assert!(validate_digit_placement('2', &left).is_err());
        assert!(validate_digit_placement('릭', &left).is_ok());
    }

    #[test]
    fn story_table_keeps_original_digit_codes() {
        let codes = BTreeMap::from([('릭', 500), ('2', 870)]);
        let original = BTreeMap::from([('2', 18)]);
        let story = story_code_table(&codes, &original).unwrap();
        assert_eq!(story[&'2'], 18);
        assert_eq!(story[&'릭'], 500);
        assert!(story_code_table(&codes, &BTreeMap::new()).is_err());
    }
}
