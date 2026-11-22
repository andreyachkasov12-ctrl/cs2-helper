mod analysis;
mod clips;
mod model;
mod parse;
mod report;

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

const HELP: &str = "\
CS2 Helper

Разбор демки (можно просто перетащить .dem на exe):
  cs2-helper матч.dem [--player НИК] [--steamid ID] [--min N] [--no-pause]

Склейка записанных клипов в один ролик (нужен ffmpeg в PATH):
  cs2-helper join папка_с_mp4 [имя.mp4]
";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let pause = !args.iter().any(|a| a == "--no-pause");

    if let Err(e) = run(&args) {
        eprintln!("\nОшибка: {:#}", e);
    }
    if pause {
        print!("\nНажми Enter, чтобы закрыть окно...");
        let _ = io::stdout().flush();
        let _ = io::stdin().read_line(&mut String::new());
    }
}

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn run(args: &[String]) -> Result<()> {
    match args.first().map(|s| s.as_str()) {
        None | Some("--help") | Some("-h") => {
            println!("{}", HELP);
            Ok(())
        }
        Some("join") => {
            let dir = args.get(1).context("укажи папку с .mp4")?;
            let name = args.get(2).filter(|a| !a.starts_with("--")).cloned().unwrap_or("highlights.mp4".into());
            let out = clips::join(Path::new(dir), &name)?;
            println!("Готово: {}", out.display());
            Ok(())
        }
        Some(path) => analyze(Path::new(path), args),
    }
}

fn choose_player(demo: &model::Demo, args: &[String]) -> Result<(String, String)> {
    // steam -> (ник, килы)
    let mut players: HashMap<String, (String, usize)> = HashMap::new();
    for d in &demo.deaths {
        if !d.victim_steam.is_empty() {
            players.entry(d.victim_steam.clone()).or_insert((d.victim.clone(), 0));
        }
        if !d.attacker_steam.is_empty() {
            let e = players.entry(d.attacker_steam.clone()).or_insert((d.attacker.clone(), 0));
            if d.attacker_steam != d.victim_steam && d.attacker_team != d.victim_team {
                e.1 += 1;
            }
        }
    }
    let mut list: Vec<(String, String, usize)> =
        players.into_iter().map(|(s, (n, k))| (s, n, k)).collect();
    list.sort_by(|a, b| b.2.cmp(&a.2));

    if let Some(id) = opt(args, "--steamid") {
        let p = list.iter().find(|p| p.0 == id).context("такого steamid нет в демке")?;
        return Ok((p.0.clone(), p.1.clone()));
    }
    if let Some(nick) = opt(args, "--player") {
        let p = list
            .iter()
            .find(|p| p.1.eq_ignore_ascii_case(&nick))
            .context("игрока с таким ником нет в демке")?;
        return Ok((p.0.clone(), p.1.clone()));
    }

    println!("Игроки в матче:");
    for (i, p) in list.iter().enumerate() {
        println!("  {}. {} ({} киллов)", i + 1, p.1, p.2);
    }
    print!("Введи номер игрока: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let n: usize = line.trim().parse().context("нужно ввести число")?;
    let p = list.get(n.wrapping_sub(1)).context("нет игрока с таким номером")?;
    Ok((p.0.clone(), p.1.clone()))
}

fn analyze(path: &Path, args: &[String]) -> Result<()> {
    if !path.exists() {
        bail!("файл не найден: {}", path.display());
    }
    println!("Читаю демку, подожди немного...");
    let bytes = fs::read(path).with_context(|| format!("не могу прочитать {}", path.display()))?;
    let demo = parse::load_demo(&bytes)?;
    if demo.deaths.is_empty() {
        bail!("в демке не нашлось событий убийств (это точно демка CS2?)");
    }

    let (steam, nick) = choose_player(&demo, args)?;
    let min_kills: usize = opt(args, "--min").and_then(|v| v.parse().ok()).unwrap_or(2);

    let st = analysis::stats(&demo, &steam);
    let mi = analysis::mistakes(&demo, &steam);
    let hl = analysis::highlights(&demo, &steam, min_kills);
    let lu = analysis::lineups(&demo, &steam);

    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("demo".into());
    let out_dir: PathBuf = path.with_file_name(format!("{}_разбор", stem));
    fs::create_dir_all(&out_dir)?;

    let html = report::build(&stem, &nick, &st, &mi, &hl, &lu);
    let report_path = out_dir.join("report.html");
    fs::write(&report_path, html)?;
    clips::write_cfgs(&out_dir.join("clips"), &steam, &hl)?;

    println!("Готово. Папка: {}", out_dir.display());
    println!("Киллы: {}, клипов: {}, гранат: {}", st.kills, hl.len(), lu.len());

    // Открываем отчёт в браузере.
    if cfg!(windows) {
        let _ = Command::new("cmd").args(["/C", "start", ""]).arg(&report_path).spawn();
    } else if cfg!(target_os = "macos") {
        let _ = Command::new("open").arg(&report_path).spawn();
    } else {
        let _ = Command::new("xdg-open").arg(&report_path).spawn();
    }
    Ok(())
}
