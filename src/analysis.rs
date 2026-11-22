use crate::model::*;
use std::collections::BTreeMap;

/// Окно размена: если напарники убили убийцу за это время, смерть считается разменянной.
const TRADE_WINDOW: i32 = 5 * TICKRATE;
/// Килы дальше друг от друга, чем это, считаются отдельными клипами.
const STREAK_GAP: i32 = 10 * TICKRATE;
/// Сколько секунд показать до первого и после последнего килла.
const PRE_SECS: i32 = 4;
const POST_SECS: i32 = 2;

pub struct Stats {
    pub kills: usize,
    pub deaths: usize,
    pub headshots: usize,
    pub rounds: u32,
}

pub struct Mistakes {
    pub untraded: Vec<Death>,
    pub first_deaths: Vec<Death>,
    pub team_flashes: Vec<Blind>,
    pub self_flashes: usize,
    pub death_points: Vec<(f32, f32)>,
}

pub struct Highlight {
    pub round: u32,
    pub label: String,
    pub kills: Vec<Death>,
    pub start_tick: i32,
    pub end_tick: i32,
}

fn is_real_kill(d: &Death, steam: &str) -> bool {
    d.attacker_steam == steam && d.attacker_steam != d.victim_steam && d.attacker_team != d.victim_team
}

pub fn stats(demo: &Demo, steam: &str) -> Stats {
    let kills: Vec<&Death> = demo.deaths.iter().filter(|d| is_real_kill(d, steam)).collect();
    Stats {
        kills: kills.len(),
        deaths: demo.deaths.iter().filter(|d| d.victim_steam == steam).count(),
        headshots: kills.iter().filter(|d| d.headshot).count(),
        rounds: demo.rounds,
    }
}

pub fn mistakes(demo: &Demo, steam: &str) -> Mistakes {
    let my_deaths: Vec<&Death> = demo.deaths.iter().filter(|d| d.victim_steam == steam).collect();

    // Смерти без размена: никто из команды не убил убийцу в окне после смерти.
    let untraded = my_deaths
        .iter()
        .filter(|d| !d.attacker_steam.is_empty() && d.attacker_steam != d.victim_steam)
        .filter(|d| {
            !demo.deaths.iter().any(|t| {
                t.round == d.round
                    && t.tick > d.tick
                    && t.tick - d.tick <= TRADE_WINDOW
                    && t.victim_steam == d.attacker_steam
                    && t.attacker_team == d.victim_team
            })
        })
        .map(|d| (*d).clone())
        .collect();

    // Первые смерти в раундах.
    let mut first_by_round: BTreeMap<u32, &Death> = BTreeMap::new();
    for d in &demo.deaths {
        first_by_round
            .entry(d.round)
            .and_modify(|cur| {
                if d.tick < cur.tick {
                    *cur = d;
                }
            })
            .or_insert(d);
    }
    let first_deaths = first_by_round
        .values()
        .filter(|d| d.victim_steam == steam)
        .map(|d| (*d).clone())
        .collect();

    // Флеши в своих (ослепление 1.5 с и больше) и в себя.
    let team_flashes = demo
        .blinds
        .iter()
        .filter(|b| {
            b.thrower_steam == steam
                && b.victim_steam != steam
                && b.victim_team == b.thrower_team
                && b.duration >= 1.5
        })
        .cloned()
        .collect();
    let self_flashes = demo
        .blinds
        .iter()
        .filter(|b| b.thrower_steam == steam && b.victim_steam == steam && b.duration >= 1.5)
        .count();

    Mistakes {
        untraded,
        first_deaths,
        team_flashes,
        self_flashes,
        death_points: my_deaths.iter().map(|d| d.victim_pos).collect(),
    }
}

/// Группирует килы игрока в клипы. min_kills = 1 даёт клип на каждый килл.
pub fn highlights(demo: &Demo, steam: &str, min_kills: usize) -> Vec<Highlight> {
    let mut kills: Vec<&Death> = demo.deaths.iter().filter(|d| is_real_kill(d, steam)).collect();
    kills.sort_by_key(|d| (d.round, d.tick));

    let mut groups: Vec<Vec<Death>> = Vec::new();
    for k in kills {
        match groups.last_mut() {
            Some(g)
                if g.last().map(|l| l.round == k.round && k.tick - l.tick <= STREAK_GAP)
                    == Some(true) =>
            {
                g.push(k.clone())
            }
            _ => groups.push(vec![k.clone()]),
        }
    }

    groups
        .into_iter()
        .filter(|g| g.len() >= min_kills)
        .map(|g| {
            let label = match g.len() {
                1 => "1K",
                2 => "2K",
                3 => "3K",
                4 => "4K",
                _ => "ACE",
            }
            .to_string();
            Highlight {
                round: g[0].round,
                label,
                start_tick: (g[0].tick - PRE_SECS * TICKRATE).max(0),
                end_tick: g[g.len() - 1].tick + POST_SECS * TICKRATE,
                kills: g,
            }
        })
        .collect()
}

pub fn lineups(demo: &Demo, steam: &str) -> Vec<Throw> {
    demo.throws
        .iter()
        .filter(|t| t.steam == steam && t.land.is_some())
        .cloned()
        .collect()
}
