mod backup_error;
mod backup_error_hook;
mod bootstrap;
mod card_labels;
mod card_surfaces;
mod character_names;
mod clear_labels;
mod counter_label;
mod course_cards;
mod difficulty_labels;
mod effects;
mod glyph_overrides;
mod indexed_graphics;
mod language_labels;
mod link_cards;
mod link_connection;
mod link_messages;
mod link_status;
mod lz10;
mod managed_input;
mod navigation;
mod pause_heading;
mod pipeline;
mod ranking_courses;
mod ranking_obj_headings;
mod ranking_units;
mod saving;
mod selection_cards;
mod source;
mod story;
mod task_text;
mod text;
mod ticket;
mod tutorial;
mod tutorial_annotations;
mod tutorial_headings;
mod ui_panels;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "모두의 뿌요뿌요 한국어 패치 제작 도구")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 지원 원본 ROM의 해시와 헤더를 검증한다.
    VerifySource {
        #[arg(long)]
        rom: PathBuf,
    },
    /// 장식 효과의 원본 PNG와 색·쓰기 명세를 로컬로 내보낸다.
    ExportEffects {
        #[arg(long)]
        rom: PathBuf,
        #[arg(long)]
        out_dir: PathBuf,
    },
    /// 관리 입력 전체를 합치는 누적 빌드. 기본은 비배포 개발 빌드다.
    BuildFoundation {
        #[arg(long)]
        rom: PathBuf,
        /// 관리 한국어 효과 대신 정확한 크기·색의 6표현 PNG를 선택하는 개발 진단 옵션.
        #[arg(long)]
        effects_dir: Option<PathBuf>,
        #[arg(long)]
        out_dir: PathBuf,
        /// 사용자가 승인한 배포 버전. 주면 BPS 비배포 표식을 떼고 산출물 이름에 버전을 붙인다.
        #[arg(long, requires = "release_channel", conflicts_with = "effects_dir")]
        release_version: Option<String>,
        /// 배포 채널(예: beta). `--release-version`과 함께 쓴다.
        #[arg(long, requires = "release_version")]
        release_channel: Option<String>,
    },
    /// 세 언어명만 한국어로 바꾸는 비배포 개발 빌드. 전체 한글판이 아니다.
    BuildLanguageLabels {
        #[arg(long)]
        rom: PathBuf,
        #[arg(long)]
        out_dir: PathBuf,
    },
    /// 무수정 ROM/BPS 왕복으로 빌드 기반만 검증한다. 한글 패치가 아니다.
    BuildBaseline {
        #[arg(long)]
        rom: PathBuf,
        /// 기존 디렉터리를 덮어쓰지 않는다.
        #[arg(long)]
        out_dir: PathBuf,
    },
}

fn main() -> Result<()> {
    let command = Cli::parse().command;
    let rom = match &command {
        Command::VerifySource { rom }
        | Command::BuildBaseline { rom, .. }
        | Command::BuildLanguageLabels { rom, .. }
        | Command::BuildFoundation { rom, .. }
        | Command::ExportEffects { rom, .. } => rom,
    };
    let bytes = std::fs::read(rom).with_context(|| format!("ROM 읽기: {}", rom.display()))?;
    let profile = source::Profile::supported()?;
    profile.verify(&bytes)?;
    let report = match command {
        Command::VerifySource { .. } => serde_json::json!({
            "kind": "source_verification", "profile": profile, "verified": true,
        }),
        Command::ExportEffects { out_dir, .. } => effects::export(&bytes, &out_dir)?,
        Command::BuildFoundation {
            out_dir,
            effects_dir,
            release_version,
            release_channel,
            ..
        } => {
            let release = release_version
                .zip(release_channel)
                .map(|(version, channel)| pipeline::Release { version, channel });
            pipeline::build_foundation(
                &bytes,
                &profile,
                &out_dir,
                effects_dir.as_deref(),
                release.as_ref(),
            )?
        }
        Command::BuildLanguageLabels { out_dir, .. } => {
            pipeline::build_language_labels(&bytes, &profile, &out_dir)?
        }
        Command::BuildBaseline { out_dir, .. } => {
            pipeline::build_baseline(&bytes, &profile, &out_dir)?
        }
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
