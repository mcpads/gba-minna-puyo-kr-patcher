use anyhow::{Context, Result, ensure};
use expected_write::{ImageRegion, RegionKind, WritePlan};
use retro_patch_utility::bps::{BpsLimits, apply_patch, create_patch};
use std::{fs, path::Path};

use crate::source::{Profile, sha256};

/// 현재는 전체 원본을 보호하는 무수정 계획이다. 채택한 패치만 이후 이 경계에 추가한다.
pub fn baseline_artifacts(source: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let plan = WritePlan::new().region(ImageRegion {
        id: "original_rom".into(),
        range: 0..source.len(),
        kind: RegionKind::Protected,
        reason: "아직 채택한 한글화 패치가 없음".into(),
    });
    let target = plan.apply(source, None)?;
    plan.audit(source, &target, None)?;
    let patch = create_patch(
        source,
        &target,
        b"Infrastructure baseline; not a Korean patch",
    )?;
    let limits = BpsLimits::new(patch.len(), source.len(), target.len(), 1024, 1_000_000);
    let reapplied = apply_patch(source, &patch, limits)?;
    ensure!(reapplied.target == target, "BPS 재적용 결과 불일치");
    Ok((target, patch))
}

pub fn build_baseline(
    source: &[u8],
    profile: &Profile,
    out_dir: &Path,
) -> Result<serde_json::Value> {
    profile.verify(source)?;
    let (target, patch) = baseline_artifacts(source)?;
    let report = serde_json::json!({
        "kind": "infrastructure_baseline",
        "korean_patch": false,
        "source": profile,
        "target_sha256": sha256(&target),
        "patch_sha256": sha256(&patch),
        "changed_bytes": 0,
        "expected_write_audit": "pass",
        "bps_roundtrip": "pass",
        "runtime_verification": "not_run",
        "outputs": ["baseline.gba", "baseline.bps"],
    });
    let encoded_report = serde_json::to_vec_pretty(&report)?;
    if let Some(parent) = out_dir.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    // 기존 ROM·산출물·심볼릭 링크를 덮어쓰지 않는다.
    fs::create_dir(out_dir)
        .with_context(|| format!("새 출력 디렉터리 필요: {}", out_dir.display()))?;
    let result: Result<()> = (|| {
        fs::write(out_dir.join("baseline.gba"), target)?;
        fs::write(out_dir.join("baseline.bps"), patch)?;
        fs::write(out_dir.join("report.json"), encoded_report)?;
        Ok(())
    })();
    if result.is_err() {
        // 이 호출에서 새로 만든 출력 묶음만 정리한다.
        let _ = fs::remove_dir_all(out_dir);
    }
    result?;
    Ok(report)
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;

/// 개발 마스크를 명시적으로 선택하는 비배포 경로. ROM 안 미확인 빈 공간은 쓰지 않는다.
pub fn build_language_labels(
    source: &[u8],
    profile: &Profile,
    out_dir: &Path,
) -> Result<serde_json::Value> {
    let (target, transform) = crate::language_labels::transform(source)?;
    let patch = create_patch(
        source,
        &target,
        b"DEVELOPMENT ONLY - language menu and labels - NOT FOR DISTRIBUTION",
    )?;
    let limits = BpsLimits::new(patch.len(), source.len(), target.len(), 1024, 1_000_000);
    let reapplied = apply_patch(source, &patch, limits)?;
    ensure!(reapplied.target == target, "BPS 재적용 불일치");
    ensure!(
        reapplied.info.metadata
            == b"DEVELOPMENT ONLY - language menu and labels - NOT FOR DISTRIBUTION",
        "비배포 BPS 표식 불일치"
    );
    let report = serde_json::json!({
        "kind":"language_labels_development", "policy":"development", "distribution_eligible":false,
        "non_distribution_marker":"BPS metadata verified; standalone ROM has no embedded marker",
        "scope":"시작 언어 화면의 제목·한국어 선택지·일본어 설명과 양언어 옵션의 日本語 → 한국어만",
        "source":profile, "target_sha256":sha256(&target), "patch_sha256":sha256(&patch),
        "transform":transform, "bps_roundtrip":"pass", "runtime_verification":"not_run",
        "remaining":["최종 폰트·배치 검수", "나머지 일본어 문구와 그래픽 제작", "누적 한국어 빌드·전체 진행 검수"]
    });
    let encoded = serde_json::to_vec_pretty(&report)?;
    if let Some(parent) = out_dir.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(out_dir)
        .with_context(|| format!("새 출력 디렉터리 필요: {}", out_dir.display()))?;
    let result: Result<()> = (|| {
        fs::write(out_dir.join("language-labels-dev.gba"), target)?;
        fs::write(out_dir.join("language-labels-dev.bps"), patch)?;
        fs::write(out_dir.join("report.json"), encoded)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(out_dir);
    }
    result?;
    Ok(report)
}

/// 사용자가 승인한 배포 버전과 채널.
pub struct Release {
    pub version: String,
    pub channel: String,
}

impl Release {
    fn validate(&self) -> Result<()> {
        let parts: Vec<&str> = self.version.split('.').collect();
        ensure!(
            parts.len() == 3
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
            "배포 버전은 X.Y.Z 숫자 형식이어야 함: {}",
            self.version
        );
        ensure!(
            !self.channel.is_empty()
                && self
                    .channel
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
            "배포 채널은 영소문자·숫자여야 함: {}",
            self.channel
        );
        Ok(())
    }
}

/// 각 컴포넌트가 같은 원본을 읽게 하고 실제 변경이 겹치면 합성을 거부한다.
pub fn build_foundation(
    source: &[u8],
    profile: &Profile,
    out_dir: &Path,
    effects_dir: Option<&Path>,
    release: Option<&Release>,
) -> Result<serde_json::Value> {
    if let Some(release) = release {
        release.validate()?;
        ensure!(effects_dir.is_none(), "배포 빌드는 관리 효과 입력만 사용");
    }
    let (labels, label_report) = crate::language_labels::transform(source)?;
    let (mut target, tutorial_report) = crate::tutorial::transform(source)?;
    let tutorial_sha256 = sha256(&target);
    for (i, (&original, &label)) in source.iter().zip(&labels).enumerate() {
        if original != label {
            ensure!(target[i] == original, "튜토리얼·언어명 작성 충돌: {i:X}");
            target[i] = label;
        }
    }
    let (navigation, navigation_report) = crate::navigation::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&navigation).enumerate() {
        if original != pixel {
            ensure!(target[i] == original, "메뉴·다른 컴포넌트 작성 충돌: {i:X}");
            target[i] = pixel;
        }
    }
    let (options, options_report) = crate::ui_panels::options(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&options).enumerate() {
        if original != pixel {
            ensure!(target[i] == original, "옵션·다른 컴포넌트 작성 충돌: {i:X}");
            target[i] = pixel;
        }
    }
    let (rules, rules_report) = crate::ui_panels::rules(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&rules).enumerate() {
        if original != pixel {
            ensure!(target[i] == original, "규칙·다른 컴포넌트 작성 충돌: {i:X}");
            target[i] = pixel;
        }
    }
    let (headings, headings_report) = crate::tutorial_headings::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&headings).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "튜토리얼 제목·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (annotations, annotations_report) = crate::tutorial_annotations::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&annotations).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "튜토리얼 그림·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (messages, messages_report) = crate::link_messages::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&messages).enumerate() {
        if original != pixel {
            ensure!(target[i] == original, "종료/오류 안내 작성 충돌: {i:X}");
            target[i] = pixel;
        }
    }
    let (connection, connection_report) = crate::link_connection::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&connection).enumerate() {
        if original != pixel {
            ensure!(target[i] == original, "통신 연결 안내 작성 충돌: {i:X}");
            target[i] = pixel;
        }
    }
    let (status, status_report) = crate::link_status::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&status).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "통신 상태·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (names, names_report) = crate::character_names::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&names).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "이름표·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (cards, cards_report) = crate::card_labels::rules(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&cards).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "규칙 카드·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (endless, endless_report) = crate::card_labels::endless(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&endless).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "무한 카드·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (task, task_report) = crate::task_text::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&task).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "태스크 문구·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (pause, pause_report) = crate::pause_heading::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&pause).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "일시정지·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (clear, clear_report) = crate::clear_labels::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&clear).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "전소거·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (difficulty, difficulty_report) = crate::difficulty_labels::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&difficulty).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "난이도·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (link_cards, link_cards_report) = crate::link_cards::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&link_cards).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "통신 카드·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (ranking, ranking_report) = crate::ui_panels::ranking(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&ranking).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "랭킹 제목·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (ranking_units, ranking_units_report) = crate::ranking_units::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&ranking_units).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "랭킹 단위·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (bootstrap, bootstrap_report) = crate::bootstrap::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&bootstrap).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "부트스트랩·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (saving, saving_report) = crate::saving::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&saving).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "저장 안내·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (ranking_courses, ranking_courses_report) = crate::ranking_courses::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&ranking_courses).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "랭킹 코스·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (ranking_obj_headings, ranking_obj_headings_report) =
        crate::ranking_obj_headings::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&ranking_obj_headings).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "랭킹 OBJ 제목·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (counter_label, counter_label_report) = crate::counter_label::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&counter_label).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "상쇄 문구·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let (course_cards, course_cards_report) = crate::course_cards::transform(source)?;
    for (i, (&original, &pixel)) in source.iter().zip(&course_cards).enumerate() {
        if original != pixel {
            ensure!(
                target[i] == original,
                "코스 카드·다른 컴포넌트 작성 충돌: {i:X}"
            );
            target[i] = pixel;
        }
    }
    let effects_report = {
        let (effect_target, report) = match effects_dir {
            Some(directory) => crate::effects::transform(source, directory)?,
            None => crate::effects::transform_managed(source)?,
        };
        for (i, (&original, &effect)) in source.iter().zip(&effect_target).enumerate() {
            if original != effect {
                ensure!(target[i] == original, "효과·다른 컴포넌트 작성 충돌: {i:X}");
                target[i] = effect;
            }
        }
        Some(report)
    };
    let backup_report = crate::backup_error::apply(
        source,
        &mut target,
        tutorial_report["expanded_used_end"]
            .as_u64()
            .context("대사 확장 끝 누락")? as usize,
    )?;
    // 사용자 결정: 게임 제목은 타이틀 화면과 갤러리 모두 원본으로 보존한다.
    let artwork: serde_json::Value = serde_json::from_str(&crate::managed_input::read_string(
        "config/artwork-decisions.json",
    )?)?;
    ensure!(
        artwork["game_title"]["action"] == "preserve_original",
        "게임 제목 보존 결정 변경"
    );
    for (start, len) in [
        (0xc20, 8),
        (0xc2c, 8),
        (0x38488, 0x2c00),
        (0x3b088, 1024),
        (0x2b838c, 512),
        (0x17b4a8, 0x9400),
        (0x1848a8, 1200),
        (0x2be38c, 512),
        (0x54c41c, 4),
        (0x54c37c, 4),
        (0x54c46c, 4),
        (0x54c3cc, 4),
    ] {
        ensure!(
            source[start..start + len] == target[start..start + len],
            "원본 제목/갤러리 보호 범위 변경: {start:X}"
        );
    }
    let title_report = serde_json::json!({"policy":"preserve_original", "written_bytes":0,
        "decision":"config/artwork-decisions.json", "source_graphics_and_loader":"identical"});
    let gallery_report = serde_json::json!({"policy":"preserve_original", "written_bytes":0,
        "decision":"config/artwork-decisions.json", "source_graphics_and_pointers":"identical"});
    let ticket_report = crate::ticket::apply(source, &mut target)?;
    // 배포 빌드는 사용자 결정에 따라 BPS 메타데이터를 비운다.
    let marker: &[u8] = match release {
        Some(_) => b"",
        None => {
            b"DEVELOPMENT ONLY - foundation and explicitly selected graphics - NOT FOR DISTRIBUTION"
        }
    };
    let patch = create_patch(source, &target, marker)?;
    let reapplied = apply_patch(
        source,
        &patch,
        BpsLimits::new(patch.len(), source.len(), target.len(), 1024, 1_000_000),
    )?;
    ensure!(
        reapplied.target == target && reapplied.info.metadata == marker,
        "BPS 왕복/메타데이터 불일치"
    );
    let mut report = serde_json::json!({"kind":"foundation_development","policy":"development",
        "distribution_eligible":false,"source":profile,"target_sha256":sha256(&target),
        "patch_sha256":sha256(&patch),"tutorial_component_sha256":tutorial_sha256,

        "component_write_conflicts":0,"runtime_verification":"not_run",
        "non_distribution_marker":"BPS metadata verified; standalone ROM has no embedded marker",
        "remaining":["문안·폰트·화면 검수","누적 ROM 런타임","나머지 메뉴·그래픽·대사 변환"]});
    if let Some(release) = release {
        let fields = report.as_object_mut().context("빌드 보고서 객체 오류")?;
        fields.remove("non_distribution_marker");
        fields.extend([
            ("kind".to_owned(), serde_json::json!("foundation_release")),
            (
                "policy".to_owned(),
                serde_json::json!("user_approved_release"),
            ),
            ("distribution_eligible".to_owned(), serde_json::json!(true)),
            (
                "release_version".to_owned(),
                serde_json::json!(release.version),
            ),
            (
                "release_channel".to_owned(),
                serde_json::json!(release.channel),
            ),
            ("bps_metadata".to_owned(), serde_json::json!("empty")),
        ]);
    }
    report
        .as_object_mut()
        .context("빌드 보고서 객체 오류")?
        .extend([
            (
                "language_labels".to_owned(),
                serde_json::json!(label_report),
            ),
            ("tutorial".to_owned(), serde_json::json!(tutorial_report)),
            (
                "navigation".to_owned(),
                serde_json::json!(navigation_report),
            ),
            ("options".to_owned(), serde_json::json!(options_report)),
            ("rules".to_owned(), serde_json::json!(rules_report)),
            (
                "character_names".to_owned(),
                serde_json::json!(names_report),
            ),
            ("rule_cards".to_owned(), serde_json::json!(cards_report)),
            (
                "endless_cards".to_owned(),
                serde_json::json!(endless_report),
            ),
            ("task_text".to_owned(), serde_json::json!(task_report)),
            ("pause_heading".to_owned(), serde_json::json!(pause_report)),
            ("clear_labels".to_owned(), serde_json::json!(clear_report)),
            (
                "difficulty_labels".to_owned(),
                serde_json::json!(difficulty_report),
            ),
            (
                "link_cards".to_owned(),
                serde_json::json!(link_cards_report),
            ),
            (
                "ranking_headings".to_owned(),
                serde_json::json!(ranking_report),
            ),
            (
                "ranking_units".to_owned(),
                serde_json::json!(ranking_units_report),
            ),
            (
                "ranking_courses".to_owned(),
                serde_json::json!(ranking_courses_report),
            ),
            (
                "ranking_obj_headings".to_owned(),
                serde_json::json!(ranking_obj_headings_report),
            ),
            (
                "counter_label".to_owned(),
                serde_json::json!(counter_label_report),
            ),
            ("saving".to_owned(), serde_json::json!(saving_report)),
            ("bootstrap".to_owned(), serde_json::json!(bootstrap_report)),
            (
                "course_cards".to_owned(),
                serde_json::json!(course_cards_report),
            ),
            (
                "tutorial_headings".to_owned(),
                serde_json::json!(headings_report),
            ),
            (
                "tutorial_annotations".to_owned(),
                serde_json::json!(annotations_report),
            ),
            ("link_status".to_owned(), serde_json::json!(status_report)),
            (
                "link_connection".to_owned(),
                serde_json::json!(connection_report),
            ),
            (
                "link_messages".to_owned(),
                serde_json::json!(messages_report),
            ),
            ("backup_error".to_owned(), serde_json::json!(backup_report)),
            ("title_logo".to_owned(), serde_json::json!(title_report)),
            ("gallery_logo".to_owned(), serde_json::json!(gallery_report)),
            ("ticket".to_owned(), serde_json::json!(ticket_report)),
            ("effects".to_owned(), serde_json::json!(effects_report)),
            ("bps_roundtrip".to_owned(), serde_json::json!("pass")),
        ]);
    let encoded = serde_json::to_vec_pretty(&report)?;
    if let Some(parent) = out_dir.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(out_dir)
        .with_context(|| format!("새 출력 디렉터리 필요: {}", out_dir.display()))?;
    let stem = match release {
        Some(release) => format!("minna-puyo-ko-{}-{}", release.version, release.channel),
        None => "foundation-dev".to_owned(),
    };
    let result: Result<()> = (|| {
        fs::write(out_dir.join(format!("{stem}.gba")), target)?;
        fs::write(out_dir.join(format!("{stem}.bps")), patch)?;
        fs::write(out_dir.join("report.json"), encoded)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(out_dir);
    }
    result?;
    Ok(report)
}
