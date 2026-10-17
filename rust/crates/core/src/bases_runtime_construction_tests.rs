//! Ordinary Conquest construction journeys through the canonical Match runner.
//! Classic Sudden Death, logistics ticks, depot nodes and Wall/Gate commands remain outside this suite.

use crate::{
    combat::CombatEvent,
    economy::{Faction, Mode, Technology},
    economy_state::ArmySize,
    environment::DamageContext,
    game::Point,
    map::AuthoritativeMap,
    match_combat::DamageWorld,
    match_commands::{Command, Denial},
    match_firing::LaunchEvent,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::{TickEvent, TickReport},
    math,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use serde_json::json;
use std::collections::BTreeSet;

pub(crate) fn scene() -> (Match, u32) {
    scene_with_tech(false)
}

fn scene_with_tech(tech: bool) -> (Match, u32) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Base runtime arena", "w":80,"h":80,
            "rows":vec![".".repeat(80);80], "heights":vec!["0".repeat(80);80],
            "spawns":[{"x":6,"y":6},{"x":73,"y":73}],
            "points":[{"x":40,"y":4,"vp":0,"mp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "Crew".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "Opponent".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    m.start.economy.players[0].mp = 20_000.0;
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for c in m.commands.units.values_mut() {
        c.hold_fire = true;
        c.auto_cast = false;
        c.auto_retreat = false;
    }
    let worker = add_worker(&mut m, Point { x: 38.0, z: 43.0 });
    m.initialize_tick_scene(0, Default::default()).unwrap();
    (m, worker)
}

fn refresh(m: &mut Match) {
    m.grid.rebuild(&m.world.game.units);
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    for owner in 0..m.world.game.players.len() {
        m.refresh_recipient_routing(owner).unwrap();
    }
}

fn add_actor(m: &mut Match, owner: usize, kind: &str, at: Point) -> u32 {
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        owner,
        m.rules.kind(kind).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[owner].spawn,
        Some(at),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        m.start.players.len(),
        vec![staged],
    )
    .unwrap()
    .publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    m.world.game.units.get_mut(&id).unwrap().hold_pos = true;
    let c = m.commands.units.get_mut(&id).unwrap();
    c.hold_fire = true;
    c.auto_cast = false;
    c.auto_retreat = false;
    refresh(m);
    id
}

pub(crate) fn add_worker(m: &mut Match, at: Point) -> u32 {
    add_actor(m, 0, "engineer", at)
}

pub(crate) fn build(m: &mut Match, worker: u32, kind: &str, at: Point) -> u32 {
    let site = m.next_unit_id;
    assert_eq!(
        m.execute_command(0, &build_command(&[worker], kind, at), false)
            .unwrap(),
        None,
        "{kind} site admission"
    );
    assert!(m.world.game.units.contains_key(&site));
    site
}

fn build_command(workers: &[u32], kind: &str, at: Point) -> Command {
    Command::Build {
        ids: workers.iter().map(|id| i64::from(*id)).collect(),
        kind: kind.into(),
        at: Some(at),
        direction: None,
        queue: false,
    }
}

pub(crate) fn ticks(m: &mut Match, count: usize) {
    for _ in 0..count {
        m.advance_tick().unwrap();
    }
}

fn finish_site(m: &mut Match, site: u32, limit: usize) {
    for _ in 0..limit {
        if m.world.game.units[&site].built == Some(1.0) {
            return;
        }
        m.advance_tick().unwrap();
    }
    assert_eq!(
        m.world.game.units[&site].built,
        Some(1.0),
        "site must finish"
    );
}

fn assist(m: &mut Match, workers: &[u32], site: u32) {
    assert_eq!(
        m.execute_command(
            0,
            &Command::Assist {
                ids: workers.iter().map(|id| i64::from(*id)).collect(),
                target: i64::from(site),
                at: Some(m.world.game.units[&site].at()),
                queue: false,
            },
            false,
        )
        .unwrap(),
        None
    );
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-8,
        "actual {actual}, expected {expected}"
    );
}

fn damage_world<'a>(m: &'a mut Match, order: &'a [u32]) -> DamageWorld<'a> {
    DamageWorld {
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
        unit_order: order,
        wind_angle: 0.0,
        wind_strength: 0.5,
    }
}

fn knock_out(m: &mut Match, id: u32) {
    // Deliver the resolved lethal contact through the combat event actuator.
    // The following tick still owns removal, section failure and rubble publication.
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    struct NoRetarget;
    impl crate::match_combat::RetargetSink for NoRetarget {
        fn reset_retarget(&mut self, _: u32) {}
    }
    damage_world(m, &order)
        .apply_events(
            &[CombatEvent::Health {
                unit: id,
                value: 0.0,
            }],
            &mut NoRetarget,
            &mut BTreeSet::new(),
        )
        .unwrap();
}

fn remove_starting_barracks(m: &mut Match) {
    let ids = m
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "barracks")
        .map(|u| u.id)
        .collect::<Vec<_>>();
    assert!(
        !ids.is_empty(),
        "the real constructor supplies starting Barracks"
    );
    for id in ids {
        knock_out(m, id);
    }
    ticks(m, 1);
    assert!(
        !m.world
            .game
            .units
            .values()
            .any(|u| { u.owner == 0 && m.rules.kinds[u.kind].name == "barracks" })
    );
}

fn pillbox_fired(report: &TickReport, site: u32) -> bool {
    report.events.iter().any(|event| match event {
        TickEvent::Firing(LaunchEvent::SmallArms(shot)) => shot.shooter == site,
        TickEvent::Firing(LaunchEvent::Flight { shooter, .. }) => *shooter == site,
        _ => false,
    })
}

#[test]
fn base29_ordinary_crews_finish_sites_and_unfinished_producers_and_pillboxes_stay_inactive() {
    // Source sim.js:5375-5380,5587-5603,5352. Classic Sudden Death remains held.
    let (mut m, worker) = scene();
    remove_starting_barracks(&mut m);
    let second = add_worker(&mut m, Point { x: 43.0, z: 38.0 });
    let distant = add_worker(&mut m, Point { x: 28.0, z: 43.0 });
    let site = m.next_unit_id;
    assert_eq!(
        m.execute_command(
            0,
            &build_command(
                &[worker, second, distant],
                "barracks",
                Point { x: 43.0, z: 43.0 }
            ),
            false
        )
        .unwrap(),
        None
    );
    assert_eq!(m.world.game.units[&site].built, Some(0.0));
    close(m.world.game.units[&site].hp, 1500.0 * 0.25);
    assert_eq!(
        m.execute_command(
            0,
            &Command::Buy {
                kind: "mg".into(),
                from: Some(site)
            },
            false
        )
        .unwrap(),
        Some(Denial::Needs)
    );
    assert_eq!(
        m.execute_command(
            0,
            &build_command(&[worker], "motorpool", Point { x: 55.0, z: 43.0 }),
            false
        )
        .unwrap(),
        Some(Denial::Needs)
    );
    let dt = m.rules.movement.tick;
    m.advance_tick().unwrap();
    let two_rate = math::pow(2.0, 0.72) / 30.0 * dt;
    close(m.world.game.units[&site].built.unwrap(), two_rate);
    close(
        m.world.game.units[&site].hp,
        375.0 + 1500.0 * 0.75 * two_rate,
    );
    assert_eq!(
        m.execute_command(
            0,
            &Command::Stop {
                ids: vec![i64::from(second), i64::from(distant)]
            },
            false
        )
        .unwrap(),
        None
    );
    m.advance_tick().unwrap();
    close(
        m.world.game.units[&site].built.unwrap(),
        two_rate + dt / 30.0,
    );
    finish_site(&mut m, site, 650);
    close(m.world.game.units[&site].hp, 1500.0);
    let newborn = m.recorded_buy(0, "mg", Some(site)).unwrap().unit;
    assert_eq!(m.recruit_metadata[&newborn].building, site);
    ticks(&mut m, 1);

    let gunner = add_worker(&mut m, Point { x: 61.5, z: 64.0 });
    let target = add_actor(&mut m, 1, "rifle", Point { x: 86.0, z: 64.0 });
    let pillbox = build(&mut m, gunner, "pillbox", Point { x: 66.0, z: 64.0 });
    m.commands.units.get_mut(&pillbox).unwrap().hold_fire = false;
    assert_eq!(
        m.execute_command(
            0,
            &Command::Stop {
                ids: vec![i64::from(gunner)]
            },
            false
        )
        .unwrap(),
        None
    );
    let target_hp = m.world.game.units[&target].hp;
    for _ in 0..40 {
        assert!(!pillbox_fired(&m.advance_tick().unwrap(), pillbox));
    }
    assert_eq!(m.world.game.units[&pillbox].built, Some(0.0));
    assert_eq!(m.world.game.units[&target].hp, target_hp);
    assist(&mut m, &[gunner], pillbox);
    for _ in 0..750 {
        if m.world.game.units[&pillbox].built == Some(1.0) {
            break;
        }
        assert!(!pillbox_fired(&m.advance_tick().unwrap(), pillbox));
    }
    assert_eq!(m.world.game.units[&pillbox].built, Some(1.0));
    assert!(
        (0..80).any(|_| pillbox_fired(&m.advance_tick().unwrap(), pillbox)),
        "finished Pillbox must actually fire at its visible target: visible={:?}, pillbox={:?}, target={:?}, control={:?}, firing={:?}",
        m.world.game.players[0].visible,
        m.world.game.units[&pillbox],
        m.world.game.units[&target],
        m.commands.units[&pillbox],
        m.firing.get(pillbox)
    );
}

#[test]
fn base30_ordinary_assist_repairs_health_and_restores_a_supported_failed_footprint_section_for_free()
 {
    // Source sim.js:3710-3722,5587-5597,2629-2645. Logistics and Sudden Death remain held.
    let (mut m, worker) = scene();
    let site = build(&mut m, worker, "barracks", Point { x: 43.0, z: 43.0 });
    finish_site(&mut m, site, 650);
    let structure = m
        .world
        .structures
        .iter()
        .find(|s| s.entity_id == Some(site))
        .unwrap()
        .clone();
    let cell = structure.cells[0];
    let section = structure.section_ids[0].clone();
    let (meta, hp) = m.world.section_by_id(&section).unwrap();
    assert!(meta.anchor && !meta.supports.is_empty());
    let damage = hp.max_hp.unwrap();
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let skip = BTreeSet::new();
    damage_world(&mut m, &order)
        .section_hit(cell, damage, &DamageContext::plain(&skip))
        .unwrap();
    assert!(m.world.section_by_id(&section).unwrap().1.failed);
    assert_eq!(m.world.game.objects.as_ref().unwrap()[cell], b'.');
    assert!(m.world.game.units[&site].hp > 0.0);
    let before_hp = m.world.game.units[&site].hp;
    let before_mp = m.start.economy.players[0].mp;
    let before_spent = m.story[0].mp_spent;
    assist(&mut m, &[worker], site);
    m.advance_tick().unwrap();
    let repaired = 1500.0 * 0.005 * m.rules.movement.tick;
    close(m.world.game.units[&site].hp - before_hp, repaired);
    let (meta, hp) = m.world.section_by_id(&section).unwrap();
    assert!(!hp.failed);
    assert!(meta.rubble.is_empty());
    close(hp.hp, repaired);
    assert_eq!(m.world.game.objects.as_ref().unwrap()[cell], b'K');
    assert_eq!(m.story[0].mp_spent, before_spent);
    close(
        m.start.economy.players[0].mp,
        before_mp + m.start.economy.players[0].inc * m.rules.movement.tick,
    );
    let second = add_worker(&mut m, Point { x: 43.0, z: 38.0 });
    assist(&mut m, &[worker, second], site);
    let before_hp = m.world.game.units[&site].hp;
    m.advance_tick().unwrap();
    close(
        m.world.game.units[&site].hp - before_hp,
        repaired * math::pow(2.0, 0.72),
    );
}

#[test]
fn base33_ordinary_death_collapses_every_section_and_allows_rebuilding_on_its_rubble() {
    // Source sim.js:1100-1106,5678. Depot node release remains held.
    let (mut m, worker) = scene();
    let site = build(&mut m, worker, "barracks", Point { x: 43.0, z: 43.0 });
    finish_site(&mut m, site, 650);
    let structure = m
        .world
        .structures
        .iter()
        .find(|s| s.entity_id == Some(site))
        .unwrap()
        .clone();
    assert_eq!(structure.cells.len(), 9);
    knock_out(&mut m, site);
    assert!(
        m.world.game.units.contains_key(&site),
        "retirement belongs to the runner"
    );
    let report = m.advance_tick().unwrap();
    assert!(!m.world.game.units.contains_key(&site));
    assert!(m.production.building(site).is_none());
    assert_eq!(report.environment().filter(|effect| matches!(effect, crate::environment::Effect::BuildingCollapse { unit, .. } if *unit == site)).count(), 1);
    for (section, &cell) in structure.section_ids.iter().zip(&structure.cells) {
        assert!(m.world.section_by_id(section).unwrap().1.failed);
        assert_eq!(m.world.game.objects.as_ref().unwrap()[cell], b'R');
    }
    let replacement = build(&mut m, worker, "barracks", Point { x: 43.0, z: 43.0 });
    assert_ne!(replacement, site);
    assert_eq!(
        m.world.game.units[&replacement].cells.as_ref().unwrap(),
        &structure
            .cells
            .iter()
            .map(|c| *c as u32)
            .collect::<Vec<_>>()
    );
    for &cell in &structure.cells {
        assert_eq!(m.world.game.objects.as_ref().unwrap()[cell], b'K');
    }
    finish_site(&mut m, replacement, 650);
    let newborn = m.recorded_buy(0, "mg", Some(replacement)).unwrap().unit;
    assert_eq!(m.recruit_metadata[&newborn].building, replacement);
    ticks(&mut m, 1);
}

#[test]
fn base37_motor_pool_needs_a_live_finished_barracks_and_then_produces_a_real_tank() {
    // Source sim.js:353,3695. The ordinary constructor's finished Barracks is retired first.
    let (mut m, worker) = scene();
    remove_starting_barracks(&mut m);
    let command = build_command(&[worker], "motorpool", Point { x: 55.0, z: 43.0 });
    let mp = m.start.economy.players[0].mp;
    let next = m.next_unit_id;
    assert_eq!(
        m.execute_command(0, &command, false).unwrap(),
        Some(Denial::Needs)
    );
    assert_eq!(m.start.economy.players[0].mp, mp);
    assert_eq!(m.next_unit_id, next);
    let barracks = build(&mut m, worker, "barracks", Point { x: 43.0, z: 43.0 });
    assert_eq!(
        m.execute_command(0, &command, false).unwrap(),
        Some(Denial::Needs)
    );
    finish_site(&mut m, barracks, 650);
    let pool = build(&mut m, worker, "motorpool", Point { x: 55.0, z: 43.0 });
    assert_eq!(
        m.execute_command(
            0,
            &Command::Buy {
                kind: "tank".into(),
                from: Some(pool)
            },
            false
        )
        .unwrap(),
        Some(Denial::Needs)
    );
    finish_site(&mut m, pool, 1100);
    let tank = m.recorded_buy(0, "tank", Some(pool)).unwrap().unit;
    assert_eq!(m.rules.kinds[m.world.game.units[&tank].kind].name, "tank");
    assert_eq!(m.recruit_metadata[&tank].building, pool);
    ticks(&mut m, 1);
}

#[test]
fn base48_ordinary_buy_and_build_unlock_at_company_and_battalion_hq_tiers() {
    // Source tech.js:4-10 and sim.js:3698. Wall/Gate dig and entrench gates remain held.
    let (mut m, worker) = scene_with_tech(true);
    let pillbox = build_command(&[worker], "pillbox", Point { x: 44.0, z: 44.0 });
    assert_eq!(m.production.player(0).unwrap().tech().tier(), 1);
    assert_eq!(
        m.execute_command(
            0,
            &Command::Buy {
                kind: "mortar".into(),
                from: None
            },
            false
        )
        .unwrap(),
        Some(Denial::Tier)
    );
    assert_eq!(
        m.execute_command(0, &pillbox, false).unwrap(),
        Some(Denial::Tier)
    );
    assert_eq!(
        m.execute_command(
            0,
            &Command::Technology {
                kind: Technology::Tier
            },
            false
        )
        .unwrap(),
        None
    );
    for _ in 0..1250 {
        if m.production.player(0).unwrap().tech().tier() == 2 {
            break;
        }
        m.advance_tick().unwrap();
    }
    assert_eq!(m.production.player(0).unwrap().tech().tier(), 2);
    let mortar = m.recorded_buy(0, "mortar", None).unwrap().unit;
    assert_eq!(
        m.rules.kinds[m.world.game.units[&mortar].kind].name,
        "mortar"
    );
    assert_eq!(m.execute_command(0, &pillbox, false).unwrap(), None);
    assert_eq!(
        m.execute_command(
            0,
            &Command::Buy {
                kind: "ranger".into(),
                from: None
            },
            false
        )
        .unwrap(),
        Some(Denial::Tier)
    );
    let pool_worker = add_worker(&mut m, Point { x: 62.0, z: 43.0 });
    let pool = build(&mut m, pool_worker, "motorpool", Point { x: 67.0, z: 43.0 });
    finish_site(&mut m, pool, 950);
    let air_worker = add_worker(&mut m, Point { x: 74.0, z: 43.0 });
    let airfield = build_command(&[air_worker], "airfield", Point { x: 79.0, z: 43.0 });
    assert_eq!(
        m.execute_command(0, &airfield, false).unwrap(),
        Some(Denial::Tier)
    );
    assert_eq!(
        m.execute_command(
            0,
            &Command::Technology {
                kind: Technology::Tier
            },
            false
        )
        .unwrap(),
        None
    );
    for _ in 0..1850 {
        if m.production.player(0).unwrap().tech().tier() == 3 {
            break;
        }
        m.advance_tick().unwrap();
    }
    assert_eq!(m.production.player(0).unwrap().tech().tier(), 3);
    assert_eq!(m.execute_command(0, &airfield, false).unwrap(), None);
    let ranger = m.recorded_buy(0, "ranger", None).unwrap().unit;
    assert_eq!(
        m.rules.kinds[m.world.game.units[&ranger].kind].name,
        "ranger"
    );
    ticks(&mut m, 1);
}
