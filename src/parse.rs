use crate::model::*;
use anyhow::Result;

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
// ЕДИНСТВЕННОЕ место, зависящее от API demoparser. Написано по памяти и НЕ
// компилировалось. Если cargo ругается — правь только эту функцию, ориентируясь
// на примеры в репозитории LaihoE/demoparser.
//
// Идея: просим игровые события и к каждому игроку-участнику (user_/attacker_)
// приклеиваем свойства team_num, X, Y, Z, pitch, yaw.
// ============================================================================
fn read_raw(bytes: &[u8]) -> Result<Vec<RawEvent>> {
    use ahash::AHashMap;
    use parser::first_pass::parser_settings::ParserInputs;
    use parser::parse_demo::{Parser, ParsingMode};

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

    let props: Vec<String> = ["team_num", "X", "Y", "Z", "pitch", "yaw"]
        .iter()
        .map(|s| s.to_string())
        .collect();

     use parser::first_pass::parser_settings::create_huffman_lookup_table;
    let huf = create_huffman_lookup_table();

    let settings = ParserInputs {
        real_name_to_og_name: AHashMap::default(),
        wanted_players: vec![],
        wanted_player_props: props,
        wanted_other_props: vec![],
        wanted_prop_states: AHashMap::default(),
        wanted_events: events,
        wanted_ticks: vec![],
        parse_ents: true,
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
