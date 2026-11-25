use crate::analysis::{self, Highlight, Mistakes, Stats};
use crate::library::{self, Entry, Library, BUTTONS, KINDS, MAPS, MOVES};
use crate::model::{Blind, Death, Demo, Throw, TICKRATE};
use crate::{clips, parse, report};
use eframe::egui::{self, Align, Color32, Layout, RichText, Rounding, Stroke};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::{fs, thread};

const ACCENT: Color32 = Color32::from_rgb(255, 138, 61);
const BG: Color32 = Color32::from_rgb(18, 21, 27);
const CARD: Color32 = Color32::from_rgb(27, 31, 39);
const MUTED: Color32 = Color32::from_rgb(139, 147, 163);
const BAD: Color32 = Color32::from_rgb(255, 99, 99);
const GOOD: Color32 = Color32::from_rgb(110, 214, 140);

type LoadResult = Result<(PathBuf, Demo), String>;

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Demo,
    Library,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Overview,
    Mistakes,
    Clips,
    Grenades,
}

struct PlayerInfo {
    steam: String,
    name: String,
    kills: usize,
}

struct Analysis {
    stats: Stats,
    mistakes: Mistakes,
    highlights: Vec<Highlight>,
    lineups: Vec<Throw>,
}

struct Loaded {
    path: PathBuf,
    demo: Demo,
    players: Vec<PlayerInfo>,
    selected: usize,
    include_singles: bool,
    tab: Tab,
    grenade_filter: usize,
    an: Analysis,
}

enum Screen {
    Idle,
    Loading(String),
    Failed(String),
    Ready(Box<Loaded>),
}

enum Action {
    Pick,
    Reset,
    SaveReport,
    WriteCfgs,
    JoinClips,
    AddToLibrary(Entry),
}

struct Editor {
    entry: Entry,
    is_new: bool,
}

#[derive(Default)]
struct LibUi {
    map_filter: usize,
    kind_filter: usize,
    editor: Option<Editor>,
    confirm_delete: Option<u64>,
}

pub struct App {
    section: Section,
    screen: Screen,
    rx: Option<Receiver<LoadResult>>,
    join_rx: Option<Receiver<Result<String, String>>>,
    notice: Option<String>,
    lib: Library,
    libui: LibUi,
}

// ----------------------------------------------------------------------------
// Данные
// ----------------------------------------------------------------------------

fn build_players(demo: &Demo) -> Vec<PlayerInfo> {
    let mut map: HashMap<String, PlayerInfo> = HashMap::new();
    for d in &demo.deaths {
        if !d.victim_steam.is_empty() {
            map.entry(d.victim_steam.clone()).or_insert(PlayerInfo {
                steam: d.victim_steam.clone(),
                name: d.victim.clone(),
                kills: 0,
            });
        }
        if !d.attacker_steam.is_empty() {
            let e = map.entry(d.attacker_steam.clone()).or_insert(PlayerInfo {
                steam: d.attacker_steam.clone(),
                name: d.attacker.clone(),
                kills: 0,
            });
            if d.attacker_steam != d.victim_steam && d.attacker_team != d.victim_team {
                e.kills += 1;
            }
        }
    }
    let mut list: Vec<PlayerInfo> = map.into_values().collect();
    list.sort_by(|a, b| b.kills.cmp(&a.kills).then(a.name.cmp(&b.name)));
    list
}

fn analyze(demo: &Demo, steam: &str, singles: bool) -> Analysis {
    Analysis {
        stats: analysis::stats(demo, steam),
        mistakes: analysis::mistakes(demo, steam),
        highlights: analysis::highlights(demo, steam, if singles { 1 } else { 2 }),
        lineups: analysis::lineups(demo, steam),
    }
}

impl Loaded {
    fn new(path: PathBuf, demo: Demo) -> Option<Self> {
        let players = build_players(&demo);
        let first = players.first()?;
        let an = analyze(&demo, &first.steam, false);
        Some(Loaded {
            path,
            demo,
            players,
            selected: 0,
            include_singles: false,
            tab: Tab::Overview,
            grenade_filter: 0,
            an,
        })
    }

    fn refresh(&mut self) {
        let steam = self.players[self.selected].steam.clone();
        self.an = analyze(&self.demo, &steam, self.include_singles);
    }
}

fn out_dir(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "demo".to_string());
    path.with_file_name(format!("{}_разбор", stem))
}

fn open_path(path: &Path) {
    #[cfg(windows)]
    let _ = Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(path).spawn();
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let _ = Command::new("xdg-open").arg(path).spawn();
}

fn save_report(l: &Loaded) -> Result<PathBuf, String> {
    let p = &l.players[l.selected];
    let stem = l
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let html = report::build(&stem, &p.name, &l.an.stats, &l.an.mistakes, &l.an.highlights, &l.an.lineups);
    let dir = out_dir(&l.path);
    fs::create_dir_all(&dir).map_err(|e| format!("Не могу создать папку: {e}"))?;
    let file = dir.join("report.html");
    fs::write(&file, html).map_err(|e| format!("Не могу записать отчёт: {e}"))?;
    open_path(&file);
    Ok(file)
}

fn tips(l: &Loaded) -> Vec<String> {
    let st = &l.an.stats;
    let m = &l.an.mistakes;
    let mut t = Vec::new();
    if st.deaths > 0 && m.untraded.len() * 2 >= st.deaths {
        t.push(format!(
            "{} из {} смертей без размена. Держись ближе к напарникам, чтобы тебя могли разменять.",
            m.untraded.len(),
            st.deaths
        ));
    }
    if m.first_deaths.len() >= 3 {
        t.push(format!(
            "Ты умирал первым в раунде {} раз. Реже выходи в дуэль первым без флешки и поддержки.",
            m.first_deaths.len()
        ));
    }
    if m.team_flashes.len() >= 3 {
        t.push(format!(
            "Ты ослепил своих {} раз. Проверяй, куда летит флешка, и предупреждай напарников.",
            m.team_flashes.len()
        ));
    }
    if m.self_flashes >= 2 {
        t.push(format!("Ты ослепил самого себя {} раз. Бросай флешки дальше от себя.", m.self_flashes));
    }
    if t.is_empty() {
        t.push("Явных системных ошибок не видно. Подробности смотри на вкладке «Ошибки».".to_string());
    }
    t
}

fn kind_ru(kind: &str) -> &str {
    match kind {
        "flashbang" => "Флешка",
        "hegrenade" => "Граната",
        "smoke" => "Смок",
        "fire" => "Молотов / зажигалка",
        other => other,
    }
}

/// Из броска в демке делаем заготовку для библиотеки.
fn entry_from_throw(t: &Throw) -> Entry {
    let mut e = blank_entry();
    e.kind = t.kind.clone();
    e.setpos = format!(
        "setpos {:.1} {:.1} {:.1};setang {:.1} {:.1} 0",
        t.pos.0, t.pos.1, t.pos.2, t.pitch, t.yaw
    );
    if let Some(f) = t.flags {
        e.jump = f & 1 == 0;
        e.crouch = f & 2 != 0;
    }
    if let Some((x, y, z)) = t.land {
        e.to = format!("Точка падения (координаты {:.0} {:.0} {:.0})", x, y, z);
    }
    e.notes = format!("Взято из демки, раунд {}.", t.round);
    e
}

fn blank_entry() -> Entry {
    Entry {
        kind: "smoke".to_string(),
        button: BUTTONS[0].to_string(),
        movement: MOVES[0].to_string(),
        ..Default::default()
    }
}

fn throw_line(e: &Entry) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !e.button.is_empty() {
        parts.push(e.button.clone());
    }
    if e.jump {
        parts.push("прыжок (jumpthrow)".to_string());
    }
    if e.crouch {
        parts.push("присед".to_string());
    }
    if !e.movement.is_empty() {
        parts.push(e.movement.clone());
    }
    parts.join("  ·  ")
}

fn flags_text(t: &Throw) -> String {
    match t.flags {
        None => "—".to_string(),
        Some(f) => {
            let mut p = Vec::new();
            if f & 1 == 0 {
                p.push("прыжок");
            }
            if f & 2 != 0 {
                p.push("присед");
            }
            if p.is_empty() {
                "с земли".to_string()
            } else {
                p.join(", ")
            }
        }
    }
}

// ----------------------------------------------------------------------------
// Мелкие элементы оформления
// ----------------------------------------------------------------------------

fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(CARD)
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, add)
        .inner
}

fn stat_card(ui: &mut egui::Ui, value: &str, label: &str, color: Color32) {
    card(ui, |ui| {
        ui.set_min_width(110.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(value).size(28.0).strong().color(color));
            ui.label(RichText::new(label).color(MUTED));
        });
    });
}

fn accent_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(Color32::BLACK))
            .fill(ACCENT)
            .rounding(8.0),
    )
}

fn th(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong().color(MUTED));
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).size(18.0).strong());
}

fn thumb(ui: &mut egui::Ui, name: &str, height: f32) {
    let path = library::image_path(name);
    let uri = format!("file://{}", path.to_string_lossy().replace('\\', "/"));
    ui.add(
        egui::Image::new(uri)
            .max_height(height)
            .max_width(height * 1.9)
            .rounding(6.0),
    );
}

fn deaths_grid(ui: &mut egui::Ui, id: &str, list: &[Death]) {
    if list.is_empty() {
        ui.label(RichText::new("Нет").color(MUTED));
        return;
    }
    egui::Grid::new(id).striped(true).spacing([28.0, 6.0]).show(ui, |ui| {
        th(ui, "Раунд");
        th(ui, "Кто убил");
        th(ui, "Оружие");
        ui.end_row();
        for d in list {
            ui.label(d.round.to_string());
            ui.label(&d.attacker);
            ui.label(&d.weapon);
            ui.end_row();
        }
    });
}

fn flashes_grid(ui: &mut egui::Ui, list: &[Blind]) {
    if list.is_empty() {
        ui.label(RichText::new("Нет").color(MUTED));
        return;
    }
    egui::Grid::new("flashes").striped(true).spacing([28.0, 6.0]).show(ui, |ui| {
        th(ui, "Раунд");
        th(ui, "Кого ослепил");
        th(ui, "Ослепление");
        ui.end_row();
        for b in list {
            ui.label(b.round.to_string());
            ui.label(&b.victim);
            ui.label(format!("{:.1} c", b.duration));
            ui.end_row();
        }
    });
}

fn death_plot(ui: &mut egui::Ui, pts: &[(f32, f32)]) {
    if pts.is_empty() {
        ui.label(RichText::new("Смертей нет.").color(MUTED));
        return;
    }
    if pts.iter().all(|p| p.0 == 0.0 && p.1 == 0.0) {
        ui.label(
            RichText::new("Позиции смертей недоступны: парсер не отдал координаты.").color(MUTED),
        );
        return;
    }
    let size = 420.0;
    let pad = 20.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 10.0, CARD);

    let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for (x, y) in pts {
        x0 = x0.min(*x);
        x1 = x1.max(*x);
        y0 = y0.min(*y);
        y1 = y1.max(*y);
    }
    let span = (x1 - x0).max(y1 - y0).max(1.0);
    for (x, y) in pts {
        let px = rect.left() + pad + (x - x0) / span * (size - 2.0 * pad);
        let py = rect.bottom() - pad - (y - y0) / span * (size - 2.0 * pad);
        painter.circle_filled(
            egui::pos2(px, py),
            6.0,
            Color32::from_rgba_unmultiplied(255, 77, 77, 170),
        );
    }
    ui.label(RichText::new("Схема в координатах игры, без картинки радара.").color(MUTED));
}

// ----------------------------------------------------------------------------
// Раздел «Разбор демки»
// ----------------------------------------------------------------------------

fn idle_ui(ui: &mut egui::Ui, hovering: bool, action: &mut Option<Action>) {
    ui.vertical_centered(|ui| {
        ui.add_space(50.0);
        let (fill, border) = if hovering {
            (Color32::from_rgb(45, 38, 32), ACCENT)
        } else {
            (CARD, Color32::from_rgb(60, 66, 80))
        };
        egui::Frame::none()
            .fill(fill)
            .stroke(Stroke::new(2.0, border))
            .rounding(Rounding::same(16.0))
            .inner_margin(egui::Margin::symmetric(70.0, 50.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Перетащи демку сюда").size(28.0).strong());
                ui.label(RichText::new("файл .dem из CS2").color(MUTED));
                ui.add_space(16.0);
                if accent_button(ui, "Выбрать файл…").clicked() {
                    *action = Some(Action::Pick);
                }
            });
        ui.add_space(26.0);
        ui.label(
            RichText::new("Где взять демку: CS2 → Смотреть → Ваши матчи → Скачать.").color(MUTED),
        );
        ui.label(
            RichText::new("Если файл заканчивается на .bz2, сначала распакуй его (7-Zip или WinRAR).")
                .color(MUTED),
        );
    });
}

fn loading_ui(ui: &mut egui::Ui, name: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(120.0);
        ui.add(egui::Spinner::new().size(40.0));
        ui.add_space(14.0);
        ui.label(RichText::new("Читаю демку…").size(22.0).strong());
        ui.label(RichText::new(name).color(MUTED));
        ui.label(RichText::new("Обычно это занимает несколько секунд.").color(MUTED));
    });
}

fn failed_ui(ui: &mut egui::Ui, msg: &str, action: &mut Option<Action>) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        card(ui, |ui| {
            ui.set_max_width(640.0);
            ui.label(RichText::new("Не получилось разобрать демку").size(22.0).strong().color(BAD));
            ui.add_space(8.0);
            ui.label(msg);
        });
        ui.add_space(16.0);
        if accent_button(ui, "Выбрать другую демку").clicked() {
            *action = Some(Action::Reset);
        }
    });
}

fn overview_ui(ui: &mut egui::Ui, l: &Loaded) {
    let st = &l.an.stats;
    let m = &l.an.mistakes;
    let hs = if st.kills > 0 { st.headshots * 100 / st.kills } else { 0 };

    ui.horizontal_wrapped(|ui| {
        stat_card(ui, &st.kills.to_string(), "киллы", GOOD);
        stat_card(ui, &st.deaths.to_string(), "смерти", BAD);
        stat_card(ui, &format!("{}%", hs), "хедшоты", Color32::WHITE);
        stat_card(ui, &st.rounds.to_string(), "раундов", Color32::WHITE);
    });

    section_title(ui, "Главное из разбора");
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        for t in tips(l) {
            ui.label(format!("•  {}", t));
            ui.add_space(4.0);
        }
    });

    section_title(ui, "Ошибки в цифрах");
    ui.horizontal_wrapped(|ui| {
        stat_card(ui, &m.untraded.len().to_string(), "смертей без размена", BAD);
        stat_card(ui, &m.first_deaths.len().to_string(), "первых смертей", BAD);
        stat_card(ui, &m.team_flashes.len().to_string(), "флешей в своих", BAD);
        stat_card(ui, &m.self_flashes.to_string(), "флешей в себя", BAD);
    });

    section_title(ui, "Что найдено");
    ui.horizontal_wrapped(|ui| {
        stat_card(ui, &l.an.highlights.len().to_string(), "клипов", ACCENT);
        stat_card(ui, &l.an.lineups.len().to_string(), "гранат с точкой", ACCENT);
    });
}

fn mistakes_ui(ui: &mut egui::Ui, an: &Analysis) {
    let m = &an.mistakes;
    egui::CollapsingHeader::new(
        RichText::new(format!("Смерти без размена ({})", m.untraded.len())).size(17.0).strong(),
    )
    .default_open(true)
    .show(ui, |ui| deaths_grid(ui, "untraded", &m.untraded));

    egui::CollapsingHeader::new(
        RichText::new(format!("Первые смерти в раунде ({})", m.first_deaths.len())).size(17.0).strong(),
    )
    .default_open(true)
    .show(ui, |ui| deaths_grid(ui, "first", &m.first_deaths));

    egui::CollapsingHeader::new(
        RichText::new(format!("Флеши в своих ({})", m.team_flashes.len())).size(17.0).strong(),
    )
    .default_open(true)
    .show(ui, |ui| flashes_grid(ui, &m.team_flashes));

    egui::CollapsingHeader::new(RichText::new("Где ты умирал").size(17.0).strong())
        .default_open(true)
        .show(ui, |ui| death_plot(ui, &m.death_points));
}

fn clips_ui(ui: &mut egui::Ui, l: &Loaded, action: &mut Option<Action>, join_busy: bool) {
    ui.horizontal(|ui| {
        if accent_button(ui, "Создать .cfg для клипов").clicked() {
            *action = Some(Action::WriteCfgs);
        }
        if join_busy {
            ui.add(egui::Spinner::new());
            ui.label(RichText::new("Склеиваю ролики…").color(MUTED));
        } else if ui.button("Склеить записанные ролики…").clicked() {
            *action = Some(Action::JoinClips);
        }
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Как записать: положи .cfg в game/csgo/cfg, в демке выполни в консоли exec имя_файла и \
             запиши момент в OBS. Склейка нужна ffmpeg в PATH.",
        )
        .color(MUTED),
    );
    ui.add_space(8.0);

    if l.an.highlights.is_empty() {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label("Клипов нет. Включи «Показывать одиночные килы» вверху, чтобы клип был на каждый килл.");
        });
        return;
    }

    for (i, c) in l.an.highlights.iter().enumerate() {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            let secs = (c.end_tick - c.start_tick) as f32 / TICKRATE as f32;
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("#{}", i + 1)).color(MUTED));
                ui.label(RichText::new(&c.label).size(20.0).strong().color(ACCENT));
                ui.label(format!("раунд {}", c.round));
                ui.label(RichText::new(format!("{:.0} c   тики {}–{}", secs, c.start_tick, c.end_tick)).color(MUTED));
            });
            let kills = c
                .kills
                .iter()
                .map(|k| {
                    format!("{} ({}{})", k.victim, k.weapon, if k.headshot { ", HS" } else { "" })
                })
                .collect::<Vec<_>>()
                .join(",  ");
            ui.label(kills);
        });
        ui.add_space(4.0);
    }
}

fn grenades_ui(ui: &mut egui::Ui, l: &mut Loaded, action: &mut Option<Action>) {
    let kinds = [
        ("Все", ""),
        ("Флешки", "flashbang"),
        ("Смоки", "smoke"),
        ("Гранаты", "hegrenade"),
        ("Молотовы", "fire"),
    ];
    ui.horizontal(|ui| {
        for (i, (name, _)) in kinds.iter().enumerate() {
            if ui.selectable_label(l.grenade_filter == i, *name).clicked() {
                l.grenade_filter = i;
            }
        }
    });
    ui.label(
        RichText::new(
            "Кнопка «В библиотеку» переносит бросок в раздел «Раскидки»: там добавишь карту, скрины и описание.",
        )
        .color(MUTED),
    );
    ui.add_space(6.0);

    let want = kinds[l.grenade_filter].1;
    let items: Vec<&Throw> = l
        .an
        .lineups
        .iter()
        .filter(|t| want.is_empty() || t.kind == want)
        .collect();

    if items.is_empty() {
        ui.label(RichText::new("Гранат нет.").color(MUTED));
        return;
    }
    if items.iter().all(|t| t.pos.0 == 0.0 && t.pos.1 == 0.0) {
        ui.label(
            RichText::new("Позиции бросков недоступны: парсер не отдал координаты игрока.").color(BAD),
        );
    }

    egui::Grid::new("grenades").striped(true).spacing([28.0, 8.0]).show(ui, |ui| {
        th(ui, "Раунд");
        th(ui, "Тип");
        th(ui, "Куда упала");
        th(ui, "Бросок");
        th(ui, "");
        ui.end_row();
        for t in items {
            ui.label(t.round.to_string());
            ui.label(kind_ru(&t.kind));
            let land = t
                .land
                .map(|(x, y, z)| format!("{:.0} {:.0} {:.0}", x, y, z))
                .unwrap_or_default();
            ui.label(land);
            ui.label(flags_text(t));
            if ui.button("В библиотеку").clicked() {
                *action = Some(Action::AddToLibrary(entry_from_throw(t)));
            }
            ui.end_row();
        }
    });
}

fn ready_ui(ui: &mut egui::Ui, l: &mut Loaded, action: &mut Option<Action>, join_busy: bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Игрок").color(MUTED));
        let current = l.players[l.selected].name.clone();
        let mut changed = false;
        egui::ComboBox::from_id_salt("player")
            .selected_text(current)
            .width(240.0)
            .show_ui(ui, |ui| {
                for (i, p) in l.players.iter().enumerate() {
                    if ui
                        .selectable_value(&mut l.selected, i, format!("{}  ({} киллов)", p.name, p.kills))
                        .clicked()
                    {
                        changed = true;
                    }
                }
            });
        if changed {
            l.refresh();
        }
        if ui.checkbox(&mut l.include_singles, "Показывать одиночные килы в нарезках").changed() {
            l.refresh();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Другая демка").clicked() {
                *action = Some(Action::Reset);
            }
            if ui.button("Сохранить отчёт HTML").clicked() {
                *action = Some(Action::SaveReport);
            }
        });
    });
    ui.label(
        RichText::new(
            l.path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        )
        .color(MUTED),
    );
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        for (tab, name) in [
            (Tab::Overview, "Обзор"),
            (Tab::Mistakes, "Ошибки"),
            (Tab::Clips, "Нарезки"),
            (Tab::Grenades, "Гранаты"),
        ] {
            if ui
                .selectable_label(l.tab == tab, RichText::new(name).size(17.0))
                .clicked()
            {
                l.tab = tab;
            }
            ui.add_space(6.0);
        }
    });
    ui.separator();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match l.tab {
        Tab::Overview => overview_ui(ui, l),
        Tab::Mistakes => mistakes_ui(ui, &l.an),
        Tab::Clips => clips_ui(ui, l, action, join_busy),
        Tab::Grenades => grenades_ui(ui, l, action),
    });
}

// ----------------------------------------------------------------------------
// Раздел «Раскидки» (библиотека)
// ----------------------------------------------------------------------------

fn library_ui(ui: &mut egui::Ui, lib: &mut Library, st: &mut LibUi, notice: &mut Option<String>) {
    if st.editor.is_some() {
        editor_ui(ui, lib, st, notice);
    } else {
        list_ui(ui, lib, st);
    }
}

fn list_ui(ui: &mut egui::Ui, lib: &mut Library, st: &mut LibUi) {
    ui.horizontal(|ui| {
        if accent_button(ui, "+ Добавить раскидку").clicked() {
            st.editor = Some(Editor {
                entry: blank_entry(),
                is_new: true,
            });
        }
        ui.label(RichText::new(format!("Всего раскидок: {}", lib.entries.len())).color(MUTED));
    });
    ui.add_space(4.0);

    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(st.map_filter == 0, "Все карты").clicked() {
            st.map_filter = 0;
        }
        for (i, m) in MAPS.iter().enumerate() {
            if ui.selectable_label(st.map_filter == i + 1, *m).clicked() {
                st.map_filter = i + 1;
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(st.kind_filter == 0, "Все типы").clicked() {
            st.kind_filter = 0;
        }
        for (i, (name, _)) in KINDS.iter().enumerate() {
            if ui.selectable_label(st.kind_filter == i + 1, *name).clicked() {
                st.kind_filter = i + 1;
            }
        }
    });
    ui.separator();

    let map_f: Option<&str> = if st.map_filter == 0 { None } else { Some(MAPS[st.map_filter - 1]) };
    let kind_f: Option<&str> = if st.kind_filter == 0 { None } else { Some(KINDS[st.kind_filter - 1].1) };

    let mut to_edit: Option<Entry> = None;
    let mut to_delete: Option<u64> = None;
    let mut shown = 0usize;

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for e in lib.entries.iter().rev() {
            if let Some(m) = map_f {
                if e.map != m {
                    continue;
                }
            }
            if let Some(k) = kind_f {
                if e.kind != k {
                    continue;
                }
            }
            shown += 1;
            card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&e.title).size(19.0).strong());
                    ui.label(RichText::new(format!("{}  ·  {}", e.map, kind_ru(&e.kind))).color(ACCENT));
                });
                if !e.from.is_empty() {
                    ui.label(format!("Откуда: {}", e.from));
                }
                if !e.to.is_empty() {
                    ui.label(format!("Куда: {}", e.to));
                }
                ui.label(RichText::new(throw_line(e)).strong());
                if !e.notes.is_empty() {
                    ui.label(RichText::new(&e.notes).color(MUTED));
                }
                if !e.images.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for n in &e.images {
                            thumb(ui, n, 170.0);
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button("Редактировать").clicked() {
                        to_edit = Some(e.clone());
                    }
                    if !e.setpos.is_empty() && ui.button("Копировать setpos").clicked() {
                        ui.output_mut(|o| o.copied_text = e.setpos.clone());
                    }
                    if st.confirm_delete == Some(e.id) {
                        ui.label("Удалить?");
                        if ui.button("Да").clicked() {
                            to_delete = Some(e.id);
                        }
                        if ui.button("Нет").clicked() {
                            st.confirm_delete = None;
                        }
                    } else if ui.button("Удалить").clicked() {
                        st.confirm_delete = Some(e.id);
                    }
                });
            });
            ui.add_space(6.0);
        }
        if shown == 0 {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Здесь пока пусто").size(20.0).strong());
                ui.label(
                    RichText::new(
                        "Нажми «Добавить раскидку» или перенеси бросок из разбора демки (вкладка «Гранаты»).",
                    )
                    .color(MUTED),
                );
            });
        }
    });

    if let Some(e) = to_edit {
        st.editor = Some(Editor { entry: e, is_new: false });
    }
    if let Some(id) = to_delete {
        lib.entries.retain(|x| x.id != id);
        st.confirm_delete = None;
        let _ = lib.save();
    }
}

fn editor_ui(ui: &mut egui::Ui, lib: &mut Library, st: &mut LibUi, notice: &mut Option<String>) {
    let mut ed = match st.editor.take() {
        Some(e) => e,
        None => return,
    };
    let mut keep = true;
    let mut save_now = false;

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let title = if ed.is_new { "Новая раскидка" } else { "Редактирование раскидки" };
        ui.label(RichText::new(title).size(22.0).strong());
        ui.add_space(6.0);
        let e = &mut ed.entry;

        ui.label(RichText::new("Название").color(MUTED));
        ui.add(
            egui::TextEdit::singleline(&mut e.title)
                .hint_text("Например: Смок на коннектор с респа")
                .desired_width(f32::INFINITY),
        );

        ui.horizontal(|ui| {
            ui.label(RichText::new("Карта").color(MUTED));
            let shown = if e.map.is_empty() { "Выбери карту".to_string() } else { e.map.clone() };
            egui::ComboBox::from_id_salt("map_edit").selected_text(shown).show_ui(ui, |ui| {
                for m in MAPS.iter() {
                    ui.selectable_value(&mut e.map, m.to_string(), *m);
                }
            });
        });

        ui.horizontal(|ui| {
            ui.label(RichText::new("Тип").color(MUTED));
            for (name, code) in KINDS.iter() {
                ui.selectable_value(&mut e.kind, code.to_string(), *name);
            }
        });

        ui.label(RichText::new("Откуда бросать (где встать)").color(MUTED));
        ui.add(
            egui::TextEdit::singleline(&mut e.from)
                .hint_text("Например: у ящиков на респе Т, прижаться к стене")
                .desired_width(f32::INFINITY),
        );
        ui.label(RichText::new("Куда летит").color(MUTED));
        ui.add(
            egui::TextEdit::singleline(&mut e.to)
                .hint_text("Например: ложится на коннектор")
                .desired_width(f32::INFINITY),
        );

        section_title(ui, "Как бросать");
        ui.horizontal(|ui| {
            ui.label(RichText::new("Кнопка").color(MUTED));
            for b in BUTTONS.iter() {
                ui.selectable_value(&mut e.button, b.to_string(), *b);
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut e.jump, "Прыжок (+jumpthrow)");
            ui.checkbox(&mut e.crouch, "Присед (ctrl)");
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Движение").color(MUTED));
            for m in MOVES.iter() {
                ui.selectable_value(&mut e.movement, m.to_string(), *m);
            }
        });

        ui.label(RichText::new("Заметки").color(MUTED));
        ui.add(
            egui::TextEdit::multiline(&mut e.notes)
                .hint_text("Куда целиться, на что ориентироваться, когда бросать")
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );

        ui.label(RichText::new("Команда для точки (необязательно)").color(MUTED));
        ui.add(
            egui::TextEdit::singleline(&mut e.setpos)
                .hint_text("setpos ...;setang ...")
                .desired_width(f32::INFINITY),
        );

        section_title(ui, "Скриншоты");
        ui.label(
            RichText::new(
                "Сделай скриншот (Win+Shift+S) и нажми «Вставить из буфера». Или добавь файл, или перетащи картинку в окно.",
            )
            .color(MUTED),
        );
        ui.horizontal(|ui| {
            if ui.button("Вставить из буфера").clicked() {
                match library::add_image_from_clipboard() {
                    Ok(name) => e.images.push(name),
                    Err(m) => *notice = Some(m),
                }
            }
            if ui.button("Добавить файл…").clicked() {
                if let Some(p) = rfd::FileDialog::new()
                    .set_title("Выбери картинку")
                    .add_filter("Картинки", &["png", "jpg", "jpeg"])
                    .pick_file()
                {
                    match library::add_image_from_file(&p) {
                        Ok(name) => e.images.push(name),
                        Err(m) => *notice = Some(m),
                    }
                }
            }
        });
        let mut remove: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (i, name) in e.images.iter().enumerate() {
                ui.vertical(|ui| {
                    thumb(ui, name, 140.0);
                    if ui.small_button("Убрать").clicked() {
                        remove = Some(i);
                    }
                });
            }
        });
        if let Some(i) = remove {
            e.images.remove(i);
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if accent_button(ui, "Сохранить").clicked() {
                if e.title.trim().is_empty() || e.map.is_empty() {
                    *notice = Some("Укажи название и карту.".to_string());
                } else {
                    save_now = true;
                }
            }
            if ui.button("Отмена").clicked() {
                keep = false;
            }
        });
    });

    if save_now {
        let mut entry = ed.entry.clone();
        if ed.is_new {
            entry.id = library::now_ms();
            lib.entries.push(entry);
        } else if let Some(slot) = lib.entries.iter_mut().find(|x| x.id == entry.id) {
            *slot = entry;
        } else {
            lib.entries.push(entry);
        }
        if let Err(m) = lib.save() {
            *notice = Some(m);
        }
        keep = false;
    }
    if keep {
        st.editor = Some(ed);
    }
}

// ----------------------------------------------------------------------------
// Приложение
// ----------------------------------------------------------------------------

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<PathBuf>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let mut v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = BG;
        v.extreme_bg_color = Color32::from_rgb(12, 14, 18);
        v.hyperlink_color = ACCENT;
        v.selection.bg_fill = Color32::from_rgb(150, 85, 40);
        v.widgets.inactive.rounding = Rounding::same(8.0);
        v.widgets.hovered.rounding = Rounding::same(8.0);
        v.widgets.active.rounding = Rounding::same(8.0);
        cc.egui_ctx.set_visuals(v);

        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        cc.egui_ctx.set_style(style);

        let mut app = App {
            section: Section::Demo,
            screen: Screen::Idle,
            rx: None,
            join_rx: None,
            notice: None,
            lib: library::load(),
            libui: LibUi::default(),
        };
        if let Some(p) = start {
            app.start_load(&cc.egui_ctx, p);
        }
        app
    }

    fn start_load(&mut self, ctx: &egui::Context, path: PathBuf) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        thread::spawn(move || {
            let res: LoadResult = fs::read(&path)
                .map_err(|e| format!("Не могу прочитать файл: {e}"))
                .and_then(|bytes| parse::load_demo(&bytes).map_err(|e| format!("{e:#}")))
                .and_then(|demo| {
                    if demo.deaths.is_empty() {
                        Err("В демке не нашлось убийств. Это точно демка CS2?".to_string())
                    } else {
                        Ok((path.clone(), demo))
                    }
                });
            let _ = tx.send(res);
            ctx2.request_repaint();
        });
        self.rx = Some(rx);
        self.screen = Screen::Loading(name);
    }

    fn poll(&mut self) {
        if let Some(res) = self.rx.as_ref().map(|r| r.try_recv()) {
            match res {
                Ok(Ok((path, demo))) => {
                    self.rx = None;
                    self.screen = match Loaded::new(path, demo) {
                        Some(l) => Screen::Ready(Box::new(l)),
                        None => Screen::Failed("В демке не нашлось игроков с убийствами.".to_string()),
                    };
                }
                Ok(Err(e)) => {
                    self.rx = None;
                    self.screen = Screen::Failed(e);
                }
                Err(TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.screen = Screen::Failed(
                        "Парсер аварийно завершился на этой демке. Пришли файл разработчику.".to_string(),
                    );
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        if let Some(res) = self.join_rx.as_ref().map(|r| r.try_recv()) {
            match res {
                Ok(Ok(p)) => {
                    self.join_rx = None;
                    if let Some(parent) = Path::new(&p).parent() {
                        open_path(parent);
                    }
                    self.notice = Some(format!("Ролик готов: {}", p));
                }
                Ok(Err(e)) => {
                    self.join_rx = None;
                    self.notice = Some(format!("Не удалось склеить: {}", e));
                }
                Err(TryRecvError::Disconnected) => {
                    self.join_rx = None;
                    self.notice = Some("Склейка завершилась с ошибкой.".to_string());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Pick => {
                if let Some(p) = rfd::FileDialog::new()
                    .set_title("Выбери демку CS2")
                    .add_filter("Демка CS2", &["dem"])
                    .pick_file()
                {
                    self.start_load(ctx, p);
                }
            }
            Action::Reset => {
                self.screen = Screen::Idle;
            }
            Action::SaveReport => {
                if let Screen::Ready(l) = &self.screen {
                    self.notice = Some(match save_report(l) {
                        Ok(p) => format!("Отчёт сохранён: {}", p.display()),
                        Err(e) => e,
                    });
                }
            }
            Action::WriteCfgs => {
                if let Screen::Ready(l) = &self.screen {
                    let steam = l.players[l.selected].steam.clone();
                    let dir = out_dir(&l.path).join("clips");
                    self.notice = Some(match clips::write_cfgs(&dir, &steam, &l.an.highlights) {
                        Ok(()) => {
                            open_path(&dir);
                            format!("Готово: {} файлов .cfg в папке {}", l.an.highlights.len(), dir.display())
                        }
                        Err(e) => format!("Ошибка: {e:#}"),
                    });
                }
            }
            Action::JoinClips => {
                if self.join_rx.is_none() {
                    if let Some(dir) = rfd::FileDialog::new()
                        .set_title("Папка с записанными клипами (.mp4)")
                        .pick_folder()
                    {
                        let (tx, rx) = channel();
                        let ctx2 = ctx.clone();
                        thread::spawn(move || {
                            let r = clips::join(&dir, "highlights.mp4")
                                .map(|p| p.display().to_string())
                                .map_err(|e| format!("{e:#}"));
                            let _ = tx.send(r);
                            ctx2.request_repaint();
                        });
                        self.join_rx = Some(rx);
                    }
                }
            }
            Action::AddToLibrary(entry) => {
                self.libui.editor = Some(Editor { entry, is_new: true });
                self.section = Section::Library;
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();

        // Перетащили файл в окно: демку грузим, картинку прикрепляем к открытой раскидке.
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            if let Some(p) = f.path {
                let ext = p
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                match ext.as_str() {
                    "dem" => {
                        self.section = Section::Demo;
                        self.start_load(ctx, p);
                    }
                    "png" | "jpg" | "jpeg" => {
                        if let Some(ed) = self.libui.editor.as_mut() {
                            match library::add_image_from_file(&p) {
                                Ok(name) => ed.entry.images.push(name),
                                Err(m) => self.notice = Some(m),
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let join_busy = self.join_rx.is_some();

        let mut action: Option<Action> = None;
        let mut screen = std::mem::replace(&mut self.screen, Screen::Idle);

        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(14, 16, 21))
                    .inner_margin(egui::Margin::symmetric(20.0, 12.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("CS2").size(22.0).strong().color(ACCENT));
                    ui.label(RichText::new("Helper").size(22.0).strong());
                    ui.add_space(28.0);
                    if ui
                        .selectable_label(self.section == Section::Demo, RichText::new("Разбор демки").size(17.0))
                        .clicked()
                    {
                        self.section = Section::Demo;
                    }
                    if ui
                        .selectable_label(self.section == Section::Library, RichText::new("Раскидки").size(17.0))
                        .clicked()
                    {
                        self.section = Section::Library;
                    }
                });
            });

        if let Some(msg) = self.notice.clone() {
            egui::TopBottomPanel::bottom("notice").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(msg);
                    if ui.button("OK").clicked() {
                        self.notice = None;
                    }
                });
            });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::same(20.0)))
            .show(ctx, |ui| {
                if self.section == Section::Library {
                    library_ui(ui, &mut self.lib, &mut self.libui, &mut self.notice);
                } else {
                    match &mut screen {
                        Screen::Idle => idle_ui(ui, hovering, &mut action),
                        Screen::Loading(name) => loading_ui(ui, name),
                        Screen::Failed(msg) => failed_ui(ui, msg, &mut action),
                        Screen::Ready(l) => ready_ui(ui, l, &mut action, join_busy),
                    }
                }
            });

        self.screen = screen;
        if let Some(a) = action {
            self.apply(ctx, a);
        }
    }
}
