use crate::analysis::Highlight;
use crate::model::TICKRATE;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const STEAM64_BASE: u64 = 76561197960265728;

/// Пишет по одному .cfg на каждый клип и общий список clips.txt.
pub fn write_cfgs(dir: &Path, steam64: &str, hl: &[Highlight]) -> Result<()> {
    fs::create_dir_all(dir)?;
    let account_id = steam64.parse::<u64>().map(|s| s.saturating_sub(STEAM64_BASE)).unwrap_or(0);
    let mut list = String::new();

    for (i, c) in hl.iter().enumerate() {
        let name = format!("clip_{:02}_{}_R{}", i + 1, c.label, c.round);
        let secs = (c.end_tick - c.start_tick) as f32 / TICKRATE as f32;
        let cfg = format!(
            "// {label}, раунд {round}. Остановить запись на тике {end} (через ~{secs:.0} c)\n\
demo_gototick {start}\n\
spec_player_by_accountid {acc}\n\
demo_resume\n",
            label = c.label,
            round = c.round,
            end = c.end_tick,
            secs = secs,
            start = c.start_tick,
            acc = account_id
        );
        fs::write(dir.join(format!("{}.cfg", name)), cfg)?;
        list.push_str(&format!(
            "{name}: тики {}–{} ({secs:.0} c)\n",
            c.start_tick, c.end_tick
        ));
    }
    fs::write(dir.join("clips.txt"), list)?;
    Ok(())
}

fn run(cmd: &mut Command, what: &str) -> Result<std::process::Output> {
    let out = cmd
        .output()
        .with_context(|| format!("не удалось запустить {} (ffmpeg установлен и есть в PATH?)", what))?;
    if !out.status.success() {
        bail!("{} завершился с ошибкой:\n{}", what, String::from_utf8_lossy(&out.stderr));
    }
    Ok(out)
}

fn duration(file: &Path) -> Result<f32> {
    let out = run(
        Command::new("ffprobe").args([
            "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0",
        ]).arg(file),
        "ffprobe",
    )?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().parse()?)
}

/// Склеивает все .mp4 из папки в один файл с плавным появлением и затуханием каждого клипа.
pub fn join(dir: &Path, out_name: &str) -> Result<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().map(|e| e.eq_ignore_ascii_case("mp4")) == Some(true)
                && p.file_name().map(|n| n != out_name) == Some(true)
                && !p.file_name().unwrap().to_string_lossy().starts_with("_tmp_")
        })
        .collect();
    files.sort();
    if files.is_empty() {
        bail!("в папке {} нет .mp4 файлов", dir.display());
    }

    let fade = 0.25f32;
    let mut tmp_files = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let d = duration(f)?;
        let st = (d - fade).max(0.0);
        let tmp = dir.join(format!("_tmp_{:03}.mp4", i));
        run(
            Command::new("ffmpeg")
                .args(["-y", "-i"])
                .arg(f)
                .args([
                    "-vf",
                    &format!("fade=t=in:st=0:d={fade},fade=t=out:st={st}:d={fade}"),
                    "-af",
                    &format!("afade=t=in:st=0:d={fade},afade=t=out:st={st}:d={fade}"),
                    "-c:v", "libx264", "-crf", "18", "-preset", "fast", "-c:a", "aac",
                ])
                .arg(&tmp),
            "ffmpeg",
        )?;
        tmp_files.push(tmp);
    }

    let list_path = dir.join("_tmp_list.txt");
    let list: String = tmp_files
        .iter()
        .map(|p| format!("file '{}'\n", p.file_name().unwrap().to_string_lossy()))
        .collect();
    fs::write(&list_path, list)?;

    let out = dir.join(out_name);
    run(
        Command::new("ffmpeg")
            .args(["-y", "-f", "concat", "-safe", "0", "-i"])
            .arg(&list_path)
            .args(["-c", "copy"])
            .arg(&out),
        "ffmpeg",
    )?;

    for t in tmp_files {
        let _ = fs::remove_file(t);
    }
    let _ = fs::remove_file(list_path);
    Ok(out)
}
