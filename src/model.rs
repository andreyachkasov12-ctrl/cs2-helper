/// Частота тиков в демках CS2 (матчмейкинг, Faceit, Premier).
pub const TICKRATE: i32 = 64;

#[derive(Clone, Debug)]
pub struct Death {
    pub tick: i32,
    pub round: u32,
    pub attacker: String,
    pub attacker_steam: String,
    pub attacker_team: i64,
    pub victim: String,
    pub victim_steam: String,
    pub victim_team: i64,
    pub weapon: String,
    pub headshot: bool,
    pub victim_pos: (f32, f32),
}

#[derive(Clone, Debug)]
pub struct Blind {
    pub tick: i32,
    pub round: u32,
    pub thrower: String,
    pub thrower_steam: String,
    pub thrower_team: i64,
    pub victim: String,
    pub victim_steam: String,
    pub victim_team: i64,
    pub duration: f32,
}

#[derive(Clone, Debug)]
pub struct Throw {
    pub tick: i32,
    pub round: u32,
    pub thrower: String,
    pub steam: String,
    pub kind: String,
    pub pos: (f32, f32, f32),
    pub pitch: f32,
    pub yaw: f32,
    pub land: Option<(f32, f32, f32)>,
}

#[derive(Default, Debug)]
pub struct Demo {
    pub deaths: Vec<Death>,
    pub blinds: Vec<Blind>,
    pub throws: Vec<Throw>,
    pub rounds: u32,
}
