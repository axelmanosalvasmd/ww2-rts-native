//! Additional native consumers. These do not qualify the missing Classic/Horde checks.
use super::*;
use ww2_core::{environment::DamageContext, match_combat::DamageWorld};

fn ordinary() -> Match {
    let base: Value =
        serde_json::from_str(include_str!("../fixtures/base-ordinary-checks.json")).unwrap();
    make(&base, &base["cases"][0])
}

fn own(m: &Match, owner: usize, kind: &str) -> u32 {
    m.world
        .game
        .units
        .values()
        .find(|u| u.owner == owner && m.rules.kinds[u.kind].name == kind)
        .unwrap()
        .id
}

fn command(m: &mut Match, value: Value) {
    let command = Command::from_json(&value).unwrap();
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
}

#[test]
fn base20_json_maker_refusals_preserve_real_constructor_state() {
    let mut m = ordinary();
    let hq = own(&m, 0, "hq");
    let foreign = own(&m, 1, "hq");
    let nonmaker = own(&m, 0, "rifle");
    // NaN and Infinity are not JSON values. Their original Source identities
    // remain in the existing eight-variant production component fixture.
    for from in [
        json!(foreign),
        json!(nonmaker),
        json!(hq.to_string()),
        Value::Null,
        json!(0),
        json!({}),
    ] {
        let before = projection(&m);
        let controls = m.commands.units.clone();
        let production = m.production.clone();
        let command = Command::from_json(&json!({"t":"buy", "unit":"rifle", "from":from}))
            .unwrap();
        let Command::Buy { from, .. } = &command else {
            panic!("Buy must retain explicit maker presence");
        };
        assert!(from.is_some());
        assert_eq!(
            m.execute_command(0, &command, false).unwrap(),
            Some(ww2_core::match_commands::Denial::Needs)
        );
        assert_eq!(projection(&m), before);
        assert_eq!(m.commands.units, controls);
        assert_eq!(m.production, production);
    }
    // Positive controls use the same real HQ and its canonical allocator.
    for explicit in [true, false] {
        let before = m.next_unit_id;
        let wallet = m.start.economy.players[0].mp;
        let mut buy = json!({"t":"buy", "unit":"rifle"});
        if explicit {
            buy["from"] = json!(hq);
        }
        command(&mut m, buy);
        assert_eq!(m.next_unit_id, before + 1);
        let newborn = &m.world.game.units[&before];
        assert_eq!(newborn.owner, 0);
        assert_eq!(m.rules.kinds[newborn.kind].name, "rifle");
        assert_eq!(m.recruit_metadata[&before].building, hq);
        assert!(m.start.economy.players[0].mp < wallet);
        assert!(m.production.building(hq).unwrap().jobs().is_empty());
    }
}

fn section_hit(m: &mut Match, cell: usize, amount: f64, contact: &str) -> f64 {
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let skip = BTreeSet::new();
    let mut hit = DamageContext::plain(&skip);
    hit.owner = Some(1);
    hit.contact_id = Some(contact);
    let mut damage = DamageWorld {
        world: &mut m.world,
        combat: &mut m.combat,
        grid: &mut m.grid,
        rules: &m.rules,
        logistics: &m.logistics,
        production: &m.production,
        mode: m.options.mode,
        tech_enabled: m.options.tech,
        environment: &mut m.environment,
        environment_seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    };
    damage.section_hit(cell, amount, &hit).unwrap().0
}

#[test]
fn base30_paid_barracks_supported_breach_assist_and_retirement() {
    let mut m = ordinary();
    let builder = own(&m, 0, "rifle");
    let site = m.next_unit_id;
    command(
        &mut m,
        json!({"t":"build", "ids":[builder], "kind":"barracks", "x":50, "z":80}),
    );
    assert_eq!(m.world.game.units[&site].built, Some(0.0));
    for _ in 0..650 {
        m.advance_tick().unwrap();
    }
    assert_eq!(m.world.game.units[&site].built, Some(1.0));
    let full = Catalog::compiled()
        .unwrap()
        .unit("barracks")
        .unwrap()
        .get("hpPer")
        .unwrap()
        .number()
        .unwrap();
    assert!((m.world.game.units[&site].hp - full).abs() < 0.001);
    // The admitted Conquest caller produces immediately. Do not manufacture
    // the original Classic queued MG or claim that missing history here.
    let recruit = m.next_unit_id;
    command(&mut m, json!({"t":"buy", "unit":"mg", "from":site}));
    assert_eq!(m.recruit_metadata[&recruit].building, site);
    let jobs = m.production.building(site).unwrap().clone();
    let structure = m
        .world
        .structures
        .iter()
        .find(|s| s.entity_id == Some(site))
        .unwrap()
        .clone();
    let cell = structure.cells[0];
    let section_cell = u32::try_from(cell).unwrap();
    let max = m.world.game.sections[&section_cell].max_hp.unwrap();
    let hp = m.world.game.units[&site].hp;
    let taken = section_hit(&mut m, cell, max, "building-contact");
    assert!(taken > 0.0);
    assert!((m.world.game.units[&site].hp - (hp - taken)).abs() < 0.001);
    assert!(m.world.game.sections[&section_cell].failed);
    assert_eq!(m.production.building(site), Some(&jobs));
    assert_eq!(section_hit(&mut m, cell, max, "building-contact"), 0.0);
    let wallet = m.start.economy.players[0].mp;
    let spent = m.story[0].mp_spent;
    command(&mut m, json!({"t":"assist", "ids":[builder], "id":site}));
    for _ in 0..2 {
        let report = m.advance_tick().unwrap();
        assert!(report.phases.contains(&TickPhase::Construction));
        assert!(!report.unavailable.contains(&TickPhase::Construction));
    }
    let repaired = &m.world.game.sections[&section_cell];
    assert!(!repaired.failed && repaired.hp > 0.0);
    assert_eq!(m.world.game.chars[cell], b'K');
    assert!(m.start.economy.players[0].mp >= wallet);
    assert_eq!(m.story[0].mp_spent, spent);
    assert!(m.world.game.units.contains_key(&recruit));
    for (index, &cell) in structure.cells.iter().enumerate() {
        section_hit(&mut m, cell, f64::INFINITY, &format!("total:{index}"));
    }
    assert!(m.world.game.units[&site].hp <= 0.0);
    let report = m.advance_tick().unwrap();
    assert!(report.retired.iter().any(|r| r.unit.id == site));
    assert!(!m.world.game.units.contains_key(&site));
    assert!(structure.cells.iter().all(|c| !m.building_cells.contains_key(c)));
}
