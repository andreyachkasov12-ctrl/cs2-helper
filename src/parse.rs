use crate::model::*;
use anyhow::Result;
use std::collections::HashMap;

struct RawEvent {
    name: String,
    tick: i32,
    fields: Vec<(String, String)>,
}

impl RawEvent {
    fn s(&self, key: &str) -> String {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
    fn f(&self, key: &str) -> f32 {
        self.s(key).parse().unwrap_or(0.0)
    }
    fn i(&self, key: &str) -> i64 {
        let v = self.s(key);
        v.parse::<i64>()
            .unwrap_or_else(|_| v.parse::<f64>().map(|x| x as i64).unwrap_or(0))
    }
    fn b(&self, key: &str) -> bool {
        self.s(key) == "true"
    }
}

/// Превращает Debug-вид значения (например `F32(12.5)`, `String("abc")`) в чистую строку.
fn clean(debug: String) -> String {
    match (debug.find('('), debug.ends_with(')')) {
        (Some(i), true) => debug[i + 1..debug.len() - 1].trim_matches('"').to_string(),
        _ => debug,
    }
}

// ============================================================================
// Работа с парсером. Сначала пробуем со свойствами игроков (команда, позиция,
// углы), если не получилось, повторяем без них.
// ============================================================================
fn read_raw_inner(bytes: &[u8], with_props: bool) -> Result<Vec<RawEvent>> {
    use ahash::AHashMap;
    use parser::first_pass::parser_settings::ParserInputs;
    use parser::parse_demo::{Parser, ParsingMode};
    use parser::second_pass::parser_settings::create_huffman_lookup_table;

    let events: Vec<String> = [
        "begin_new_match",
        "round_start",
        "player_death",
        "player_blind",
        "weapon_fire",
        "flashbang_detonate",
        "hegrenade_detonate",
        "smokegrenade_detonate",
        "inferno_startburn",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    // (короткое имя, внутреннее имя в игре)
    let prop_names = [
        ("team_num", "m_iTeamNum"),
        ("X", "CBodyComponent.m_skeletonInstance.m_vecX"),
        ("Y", "CBodyComponent.m_skeletonInstance.m_vecY"),
        ("Z", "CBodyComponent.m_skeletonInstance.m_vecZ"),
        ("pitch", "m_angEyeAngles[0]"),
        ("yaw", "m_angEyeAngles[1]"),
    ];

    let mut real_name_to_og_name: AHashMap<String, String> = AHashMap::default();
    let mut props: Vec<String> = Vec::new();
    if with_props {
        for (friendly, internal) in prop_names.iter() {
            props.push(internal.to_string());
            real_name_to_og_name.insert(internal.to_string(), friendly.to_string());
        }
    }

    let huf = create_huffman_lookup_table();

    let settings = ParserInputs {
        real_name_to_og_name,
        wanted_players: vec![],
        wanted_player_props: props,
        wanted_other_props: vec![],
        wanted_prop_states: AHashMap::default(),
        wanted_events: events,
        wanted_ticks: vec![],
        parse_ents: with_props,
        parse_projectiles: false,
        parse_grenades: false,
        parse_weapons: false,
        vector_props_as_lists: false,
        only_header: false,
        only_convars: false,
        order_by_steamid: false,
        list_props: false,
        fallback_bytes: None,
        huffman_lookup_table: &huf,
    };

    let mut p = Parser::new(settings, ParsingMode::ForceSingleThreaded);
    let output = p.parse_demo(bytes)?;

    Ok(output
        .game_events
        .iter()
        .map(|e| RawEvent {
            name: e.name.clone(),
            tick: e.tick,
            fields: e
                .fields
                .iter()
                .map(|f| {
                    let v = match &f.data {
                        Some(v) => clean(format!("{:?}", v)),
                        None => String::new(),
                    };
                    (f.name.clone(), v)
                })
                .collect(),
        })
        .collect())
}

fn read_raw(bytes: &[u8]) -> Result<Vec<RawEvent>> {
    let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        read_raw_inner(bytes, true)
    }));
    match first {
        Ok(Ok(v)) if !v.is_empty() => Ok(v),
        _ => {
            eprintln!("Свойства игроков не прочитались, разбираю без позиций и команд.");
            read_raw_inner(bytes, false)
        }
    }
}

// ---------------------------------------------------------------------------
// Дальше обычный Rust, от API парсера не зависит.
// ---------------------------------------------------------------------------

struct Det {
    tick: i32,
    kind: String,
    steam: String,
    pos: (f32, f32, f32),
}

fn grenade_kind(weapon: &str) -> Option<&'static str> {
    match weapon.trim_start_matches("weapon_") {
        "flashbang" => Some("flashbang"),
        "hegrenade" => Some("hegrenade"),
        "smokegrenade" => Some("smoke"),
        "molotov" | "incgrenade" => Some("fire"),
        _ => None,
    }
}

/// Если парсер не дал номера команд, вычисляем их по килам: делим игроков
/// на две стороны так, чтобы как можно больше убийств было между сторонами.
fn infer_teams(demo: &mut Demo) {
    let known = demo.deaths.iter().any(|d| {
        d.attacker_team != 0 && d.victim_team != 0 && d.attacker_team != d.victim_team
    });
    if known {
        return;
    }

    let mut index: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for d in &demo.deaths {
        if d.attacker_steam.is_empty()
            || d.victim_steam.is_empty()
            || d.attacker_steam == d.victim_steam
        {
            continue;
        }
        let n = index.len();
        let a = *index.entry(d.attacker_steam.clone()).or_insert(n);
        let n = index.len();
        let v = *index.entry(d.victim_steam.clone()).or_insert(n);
        edges.push((a, v));
    }

    let n = index.len();
    if n < 2 || n > 20 {
        return;
    }

    let mut best_mask = 0u32;
    let mut best = (0usize, usize::MAX); // (убийств между сторонами, перекос по размеру)
    for mask in 0..(1u32 << (n - 1)) {
        let side = |p: usize| -> u32 {
            if p == 0 {
                0
            } else {
                (mask >> (p - 1)) & 1
            }
        };
        let score = edges.iter().filter(|(a, v)| side(*a) != side(*v)).count();
        let ones = mask.count_ones() as usize;
        let imbalance = (n - ones).abs_diff(ones);
        if score > best.0 || (score == best.0 && imbalance < best.1) {
            best = (score, imbalance);
            best_mask = mask;
        }
    }

    let team_of = |steam: &str| -> i64 {
        match index.get(steam) {
            Some(&p) => {
                if p != 0 && (best_mask >> (p - 1)) & 1 == 1 {
                    3
                } else {
                    2
                }
            }
            None => 0,
        }
    };

    for d in demo.deaths.iter_mut() {
        d.attacker_team = team_of(&d.attacker_steam);
        d.victim_team = team_of(&d.victim_steam);
    }
    for b in demo.blinds.iter_mut() {
        b.thrower_team = team_of(&b.thrower_steam);
        b.victim_team = team_of(&b.victim_steam);
    }
}

pub fn load_demo(bytes: &[u8]) -> Result<Demo> {
    let raw = read_raw(bytes)?;
    let mut demo = Demo::default();
    let mut dets: Vec<Det> = Vec::new();
    let mut round = 0u32;

    for e in &raw {
        match e.name.as_str() {
            // Всё до начала реального матча (разминка, рестарты) выбрасываем.
            "begin_new_match" => {
                demo = Demo::default();
                dets.clear();
                round = 0;
            }
            "round_start" => round += 1,
            "player_death" => {
                let victim = e.s("user_name");
                if victim.is_empty() {
                    continue;
                }
                demo.deaths.push(Death {
                    tick: e.tick,
                    round,
                    attacker: e.s("attacker_name"),
                    attacker_steam: e.s("attacker_steamid"),
                    attacker_team: e.i("attacker_team_num"),
                    victim,
                    victim_steam: e.s("user_steamid"),
                    victim_team: e.i("user_team_num"),
                    weapon: e.s("weapon"),
                    headshot: e.b("headshot"),
                    victim_pos: (e.f("user_X"), e.f("user_Y")),
                });
            }
            "player_blind" => demo.blinds.push(Blind {
                tick: e.tick,
                round,
                thrower: e.s("attacker_name"),
                thrower_steam: e.s("attacker_steamid"),
                thrower_team: e.i("attacker_team_num"),
                victim: e.s("user_name"),
                victim_steam: e.s("user_steamid"),
                victim_team: e.i("user_team_num"),
                duration: e.f("blind_duration"),
            }),
            "weapon_fire" => {
                if let Some(kind) = grenade_kind(&e.s("weapon")) {
                    demo.throws.push(Throw {
                        tick: e.tick,
                        round,
                        thrower: e.s("user_name"),
                        steam: e.s("user_steamid"),
                        kind: kind.to_string(),
                        pos: (e.f("user_X"), e.f("user_Y"), e.f("user_Z")),
                        pitch: e.f("user_pitch"),
                        yaw: e.f("user_yaw"),
                        land: None,
                    });
                }
            }
            "flashbang_detonate" | "hegrenade_detonate" | "smokegrenade_detonate"
            | "inferno_startburn" => {
                let kind = match e.name.as_str() {
                    "flashbang_detonate" => "flashbang",
                    "hegrenade_detonate" => "hegrenade",
                    "smokegrenade_detonate" => "smoke",
                    _ => "fire",
                };
                dets.push(Det {
                    tick: e.tick,
                    kind: kind.to_string(),
                    steam: e.s("user_steamid"),
                    pos: (e.f("x"), e.f("y"), e.f("z")),
                });
            }
            _ => {}
        }
    }
    demo.rounds = round;

    infer_teams(&mut demo);

    // Сопоставляем бросок с ближайшим следующим взрывом того же типа.
    let mut used = vec![false; dets.len()];
    for t in demo.throws.iter_mut() {
        let found = dets.iter().enumerate().find(|(i, d)| {
            !used[*i]
                && d.tick >= t.tick
                && d.kind == t.kind
                && (d.steam.is_empty() || d.steam == t.steam)
        });
        if let Some((i, d)) = found {
            let pos = d.pos;
            used[i] = true;
            t.land = Some(pos);
        }
    }

    Ok(demo)
}
