use crate::analysis::*;
use crate::model::*;
use std::fmt::Write;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn secs(ticks: i32) -> String {
    format!("{:.1} c", ticks as f32 / TICKRATE as f32)
}

fn kind_ru(kind: &str) -> &str {
    match kind {
        "flashbang" => "Флешка",
        "hegrenade" => "Граната",
        "smoke" => "Смок",
        "fire" => "Молотов/зажигалка",
        other => other,
    }
}

/// Точки смертей на схеме без подложки радара (радары Valve не распространяем).
fn death_svg(points: &[(f32, f32)]) -> String {
    if points.is_empty() {
        return "<p>Смертей нет.</p>".to_string();
    }
    let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for (x, y) in points {
        x0 = x0.min(*x);
        x1 = x1.max(*x);
        y0 = y0.min(*y);
        y1 = y1.max(*y);
    }
    let size = 520.0;
    let pad = 20.0;
    let span = (x1 - x0).max(y1 - y0).max(1.0);
    let mut s = format!(
        "<svg viewBox='0 0 {0} {0}' width='100%' style='max-width:520px;background:#1b1f27;border-radius:8px'>",
        size
    );
    for (x, y) in points {
        let px = pad + (x - x0) / span * (size - 2.0 * pad);
        let py = size - pad - (y - y0) / span * (size - 2.0 * pad);
        let _ = write!(s, "<circle cx='{:.1}' cy='{:.1}' r='7' fill='#ff4d4d' fill-opacity='.65'/>", px, py);
    }
    s.push_str("</svg>");
    s
}

pub fn build(
    demo_name: &str,
    player: &str,
    st: &Stats,
    m: &Mistakes,
    hl: &[Highlight],
    lineups: &[Throw],
) -> String {
    let mut h = String::new();
    let _ = write!(
        h,
        "<!doctype html><html lang='ru'><head><meta charset='utf-8'>\
<meta name='viewport' content='width=device-width,initial-scale=1'>\
<title>CS2 разбор: {p}</title><style>\
body{{font-family:system-ui,sans-serif;background:#12151b;color:#e6e9ef;margin:0;padding:24px;max-width:960px;margin:auto}}\
h1,h2{{margin:.6em 0 .3em}}h2{{border-bottom:1px solid #2a2f3a;padding-bottom:6px;margin-top:1.6em}}\
.cards{{display:flex;gap:12px;flex-wrap:wrap}}.card{{background:#1b1f27;border-radius:10px;padding:12px 18px}}\
.card b{{font-size:26px;display:block}}\
table{{width:100%;border-collapse:collapse;font-size:14px}}td,th{{padding:6px 8px;border-bottom:1px solid #2a2f3a;text-align:left}}\
code{{background:#1b1f27;padding:2px 6px;border-radius:5px;font-size:12px;user-select:all}}\
.muted{{color:#8b93a3}}.wrap{{overflow-x:auto}}\
</style></head><body>",
        p = esc(player)
    );

    let _ = write!(h, "<h1>Разбор: {}</h1><p class='muted'>{}</p>", esc(player), esc(demo_name));

    let hs = if st.kills > 0 { st.headshots * 100 / st.kills } else { 0 };
    let _ = write!(
        h,
        "<div class='cards'>\
<div class='card'><b>{}</b>киллы</div><div class='card'><b>{}</b>смерти</div>\
<div class='card'><b>{}%</b>хедшоты</div><div class='card'><b>{}</b>раундов</div></div>",
        st.kills, st.deaths, hs, st.rounds
    );

    // --- Ошибки ---
    h.push_str("<h2>Ошибки</h2><div class='cards'>");
    let _ = write!(
        h,
        "<div class='card'><b>{}</b>смертей без размена</div>\
<div class='card'><b>{}</b>первых смертей в раунде</div>\
<div class='card'><b>{}</b>флешей в своих</div>\
<div class='card'><b>{}</b>флешей в себя</div></div>",
        m.untraded.len(),
        m.first_deaths.len(),
        m.team_flashes.len(),
        m.self_flashes
    );

    h.push_str("<h3>Смерти без размена</h3><div class='wrap'><table><tr><th>Раунд</th><th>Кто убил</th><th>Оружие</th></tr>");
    for d in &m.untraded {
        let _ = write!(h, "<tr><td>{}</td><td>{}</td><td>{}</td></tr>", d.round, esc(&d.attacker), esc(&d.weapon));
    }
    h.push_str("</table></div>");

    h.push_str("<h3>Первые смерти в раунде</h3><div class='wrap'><table><tr><th>Раунд</th><th>Кто убил</th><th>Оружие</th></tr>");
    for d in &m.first_deaths {
        let _ = write!(h, "<tr><td>{}</td><td>{}</td><td>{}</td></tr>", d.round, esc(&d.attacker), esc(&d.weapon));
    }
    h.push_str("</table></div>");

    h.push_str("<h3>Флеши в своих</h3><div class='wrap'><table><tr><th>Раунд</th><th>Кого</th><th>Ослепление</th></tr>");
    for b in &m.team_flashes {
        let _ = write!(h, "<tr><td>{}</td><td>{}</td><td>{:.1} c</td></tr>", b.round, esc(&b.victim), b.duration);
    }
    h.push_str("</table></div>");

    h.push_str("<h3>Где ты умирал</h3>");
    h.push_str(&death_svg(&m.death_points));
    h.push_str("<p class='muted'>Схема в координатах игры, без картинки радара.</p>");

    // --- Нарезки ---
    h.push_str("<h2>Нарезки киллов</h2><p class='muted'>Файлы .cfg лежат в папке clips. В демке открой консоль и выполни <code>exec имя_файла</code>.</p>");
    h.push_str("<div class='wrap'><table><tr><th>#</th><th>Раунд</th><th>Клип</th><th>Киллы</th><th>Тики</th><th>Длина</th></tr>");
    for (i, c) in hl.iter().enumerate() {
        let kills = c
            .kills
            .iter()
            .map(|k| format!("{} ({}{})", esc(&k.victim), esc(&k.weapon), if k.headshot { ", HS" } else { "" }))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(
            h,
            "<tr><td>{}</td><td>{}</td><td><b>{}</b></td><td>{}</td><td>{}–{}</td><td>{}</td></tr>",
            i + 1,
            c.round,
            c.label,
            kills,
            c.start_tick,
            c.end_tick,
            secs(c.end_tick - c.start_tick)
        );
    }
    h.push_str("</table></div>");

    // --- Раскидки ---
    h.push_str("<h2>Твои гранаты</h2><p class='muted'>Чтобы встать на точку и повторить бросок: включи sv_cheats 1 и вставь команду в консоль (клик по ней выделяет всё).</p>");
    h.push_str("<div class='wrap'><table><tr><th>Раунд</th><th>Тип</th><th>Куда упала</th><th>Команда</th></tr>");
    for t in lineups {
        let land = t
            .land
            .map(|(x, y, z)| format!("{:.0} {:.0} {:.0}", x, y, z))
            .unwrap_or_default();
        let _ = write!(
            h,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td><code>setpos {:.1} {:.1} {:.1};setang {:.1} {:.1} 0</code></td></tr>",
            t.round,
            kind_ru(&t.kind),
            land,
            t.pos.0,
            t.pos.1,
            t.pos.2,
            t.pitch,
            t.yaw
        );
    }
    h.push_str("</table></div></body></html>");
    h
}
