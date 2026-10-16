//! Native engineering behavior checks under Axel's approved intent rule.
//! Initial poses and damage inputs are authored scene inputs, never captured outputs.
#[path = "map_runtime_batch3_tests.rs"]
mod map_runtime_batch3;
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{Context, DamageContext, Effect, EnvironmentRuntime},
    game::{Point, Pose},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::SnapshotCache,
    terrain,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    vision,
    weather::WeatherChoice,
    world::{CellChange, World},
};
use serde_json::json;
use std::collections::BTreeSet;

fn map(rows: Vec<String>, buildings: serde_json::Value) -> AuthoritativeMap {
    AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Native engineering intents", "w":64, "h":64, "rows":rows,
            "spawns":[{"x":2,"y":2},{"x":61,"y":61}],
            "points":[{"x":32,"y":32}], "buildings":buildings
        }))
        .unwrap(),
    )
    .unwrap()
}
fn options() -> MatchOptions {
    MatchOptions {
        seed: 701,
        mode: Mode::Default,
        players: (0..2)
            .map(|slot| PlayerConfig {
                name: format!("p{slot}"),
                team: slot,
                faction: Faction::try_from(slot as u8).unwrap(),
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather: false,
        map_key: None,
        tech: true,
        supply: false,
        logistics: false,
        sandbox: false,
    }
}
fn scene() -> Match {
    Match::new(&map(vec![".".repeat(64); 64], json!([])), options()).unwrap()
}
fn publish(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        owner,
        m.rules.kind(name).unwrap(),
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
    id
}
fn env<T>(
    m: &mut Match,
    run: impl FnOnce(&mut EnvironmentRuntime, &mut World, &mut Context<'_>) -> T,
) -> T {
    let ids: Vec<_> = m.world.game.units.keys().copied().collect();
    let grid = SpatialGrid::new(&m.world.game.units, 16.0);
    let mut context = Context {
        tick: m.world.game.tick as u64,
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &ids,
        grid: &grid,
        rain: m.world.game.rain,
        wet: m.world.game.wet.unwrap_or(0.0),
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    };
    run(&mut m.environment, &mut m.world, &mut context)
}
fn center(c: usize) -> Point {
    Point {
        x: (c % 64) as f64 * 2.0 + 1.0,
        z: (c / 64) as f64 * 2.0 + 1.0,
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "actual {a}, expected {b}");
}
fn damage(m: &mut Match, c: usize, amount: f64) -> Vec<Effect> {
    env(m, |e, w, ctx| {
        e.damage_section(w, c, amount, ctx, &DamageContext::plain(&BTreeSet::new()))
            .unwrap()
            .1
    })
}
fn combat<T>(
    m: &mut Match,
    run: impl FnOnce(
        &mut crate::match_combat::DamageWorld<'_>,
        &mut crate::match_firing::FiringState,
    ) -> T,
) -> T {
    let order: Vec<_> = m.world.game.units.keys().copied().collect();
    let mut damage = crate::match_combat::DamageWorld {
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
    run(&mut damage, &mut m.firing)
}
fn blast_hit(m: &mut Match, id: u32, anti: Option<f64>) -> f64 {
    let before = m.world.game.units[&id].hp;
    let profile = crate::match_combat::ExplosionSource {
        infantry: 20.0,
        vehicle: 20.0,
        terrain: None,
        suppression: 0.0,
        scar: 0.0,
        anti_garrison: anti,
        weapon_kind: None,
        off_map_support: false,
    };
    combat(m, |d, f| d.hurt(id, profile, 1.0, 1, f).unwrap());
    before - m.world.game.units[&id].hp
}
fn direct(m: &mut Match, shooter: u32, target: u32) -> f64 {
    let u = &m.world.game.units[&shooter];
    let source = crate::match_combat::FlightSource {
        shooter,
        kind: u.kind,
        owner: u.owner as i32,
        airborne: false,
    };
    let impact = crate::combat::Impact {
        owner: source.owner,
        shooter,
        weapon: crate::combat::WeaponDamage::from_catalog(&m.rules.kinds[u.kind].name).unwrap(),
        damage_scale: None,
        energy: 1.0,
        armor_piercing: false,
        ap_naval_only: false,
        ap_multiplier: 1.0,
        launch_origin: u.at(),
        at: m.world.game.units[&target].at(),
        direction: Point { x: 1.0, z: 0.0 },
    };
    combat(m, |d, f| d.direct_hit(source, target, impact, f).unwrap())
}

#[test]
fn eng29_quiet_trench_hides_at_25m_until_firing_and_is_seen_up_close() {
    let mut m = scene();
    m.world.game.units.clear();
    // Vision consumes authored actors independently of command/publication metadata.
    let catalog = Catalog::compiled().unwrap();
    for (id, owner, x) in [(100, 0, 51.0), (101, 1, 76.0)] {
        let u = crate::unit_spawn::initial_unit(
            &m.rules,
            catalog,
            m.rules.kind("rifle").unwrap(),
            id,
            owner,
            Point { x, z: 51.0 },
        )
        .unwrap();
        m.world.game.units.insert(id, u);
    }
    let c = m.world.game.cell_of(76.0, 51.0).unwrap();
    m.world.set_cell(c, b'T').unwrap();
    m.world.game.tick = 100;
    let sees = |m: &mut Match| {
        let grid = SpatialGrid::new(&m.world.game.units, 16.0);
        vision::update_vision(&mut m.world.game, &m.rules, &grid);
        m.world.game.players[0].sees(101)
    };
    assert!(!sees(&mut m));
    m.world.game.units.get_mut(&101).unwrap().shot_at = Some(100.0);
    assert!(sees(&mut m));
    m.world.game.units.get_mut(&101).unwrap().shot_at = None;
    m.world.game.units.get_mut(&100).unwrap().x = 60.0;
    assert!(sees(&mut m));
    let wall = 25 * 64 + 33;
    m.world.set_cell(wall, b'B').unwrap();
    assert!(!sees(&mut m), "concealed close contact still requires LOS");
    m.world.set_cell(wall, b'.').unwrap();
    m.world.game.units.get_mut(&100).unwrap().x = 51.0;
    let mut plane = crate::unit_spawn::initial_aircraft(
        &m.rules,
        catalog,
        m.rules.kind("fighter").unwrap(),
        102,
        0,
        Point { x: 51.0, z: 51.0 },
    )
    .unwrap();
    plane.air = Some(crate::game::PlaneState::Station);
    m.world.game.units.insert(102, plane);
    assert!(sees(&mut m), "airborne observers bypass trench concealment");
    m.world.game.units.remove(&102);
    m.world.game.strikes.push(crate::game::Strike {
        live: true,
        kind: crate::game::StrikeKind::Recon,
        owner: 0,
        x: 76.0,
        z: 51.0,
        dir: 0.0,
        runtime: None,
    });
    assert!(sees(&mut m), "recon bypasses trench concealment");
}

#[test]
fn eng30_trench_speed_and_actual_tick_suppression_rally() {
    let mut m = scene();
    let at = Point { x: 51.0, z: 51.0 };
    let c = m.world.game.cell_of(at.x, at.z).unwrap();
    m.world.set_cell(c, b'T').unwrap();
    let id = publish(&mut m, "rifle", 0, at);
    let plain = publish(&mut m, "rifle", 0, Point { x: 71.0, z: 51.0 });
    for id in [id, plain] {
        m.world.game.units.get_mut(&id).unwrap().supp = 20.0;
        m.commands.units.get_mut(&id).unwrap().auto_retreat = false;
    }
    close(
        terrain::speed_mul(&m.world.game, &m.rules, &m.world.game.units[&id]),
        1.2,
    );
    m.advance_tick().unwrap();
    close(m.world.game.units[&id].supp, 19.3);
    close(m.world.game.units[&plain].supp, 19.6);
}

#[test]
fn eng31_trench_wear_caves_in_and_vehicle_contact_crushes_it() {
    let mut m = scene();
    let c = 25 * 64 + 25;
    m.world.set_cell(c, b'T').unwrap();
    for expected in *b"T+" {
        env(&mut m, |e, w, ctx| {
            let skip = BTreeSet::new();
            let mut hit = DamageContext::plain(&skip);
            e.damage_cells(
                w,
                crate::environment::Blast {
                    at: center(c),
                    radius: 1.0,
                    damage: 240.0,
                    scar: 0.0,
                },
                ctx,
                &mut hit,
            )
            .unwrap();
        });
        assert_eq!(m.world.game.chars[c], expected);
    }
    m.world.set_cell(c, b'T').unwrap();
    let at = center(c);
    let tank = publish(&mut m, "tank", 0, at);
    m.apply_moved_ground_contact(
        tank,
        Pose {
            x: at.x - 1.0,
            z: at.z,
            rot: 0.0,
        },
        None,
    )
    .unwrap();
    assert_eq!(m.world.game.chars[c], b'+');
    m.world.set_cell(c, b'T').unwrap();
    let rifle = publish(&mut m, "rifle", 0, at);
    close(blast_hit(&mut m, rifle, None), 10.0);
    let mut memory = crate::environment::DamageMemory::default();
    env(&mut m, |e, w, ctx| {
        let skip = BTreeSet::new();
        let mut hit = DamageContext::plain(&skip);
        hit.memory = Some(&mut memory);
        let blast = crate::environment::Blast {
            at,
            radius: 1.0,
            damage: 100.0,
            scar: 0.0,
        };
        e.damage_cells(w, blast, ctx, &mut hit).unwrap();
        let wear = w.game.wear.as_ref().unwrap()[c];
        e.damage_cells(w, blast, ctx, &mut hit).unwrap();
        assert_eq!(
            w.game.wear.as_ref().unwrap()[c],
            wear,
            "one breath hits each cell once"
        );
    });
}

#[test]
fn eng32_all_six_house_types_have_native_sections_and_keep_their_type() {
    let specs = [
        (1, 1, 0, 250.0, None),
        (10, 1, 0, 250.0, None),
        (11, 1, 1, 400.0, None),
        (23, 1, 1, 400.0, None),
        (6, 4, 2, 650.0, None),
        (1, 1, 3, 700.0, Some("church")),
        (1, 1, 4, 1000.0, Some("factory")),
        (1, 1, 5, 800.0, Some("mine head")),
    ];
    for (width, height, kind, hp, name) in specs {
        let mut rows = vec![vec![b'.'; 64]; 64];
        for row in rows.iter_mut().skip(20).take(height) {
            row[20..20 + width].fill(b'B');
        }
        let rows = rows
            .into_iter()
            .map(|r| String::from_utf8(r).unwrap())
            .collect();
        let buildings = name.map_or(json!([]), |name| json!([{"x":20,"y":20,"kind":name}]));
        let mut m = Match::new(&map(rows, buildings), options()).unwrap();
        for y in 20..20 + height {
            for x in 20..20 + width {
                let c = y * 64 + x;
                assert_eq!(usize::from(m.world.game.house[c]), kind);
                close(f64::from(m.world.game.cell_hp[c]), hp);
                close(m.world.game.sections[&(c as u32)].max_hp.unwrap(), hp);
            }
        }
        damage(&mut m, 20 * 64 + 20, hp);
        assert_eq!(usize::from(m.world.game.house[20 * 64 + 20]), kind);
        if width > 1 {
            assert_eq!(usize::from(m.world.game.house[20 * 64 + 21]), kind);
        }
    }
}

#[test]
fn eng34_house_cover_and_vision_follow_type_and_damage() {
    // Literal house data and consumers from pinned sim.js:139-146,2299,4548.
    for (width, kind, hp, whole, worn, sight, name) in [
        (1, 0, 250.0, 0.55, 0.85, 1.25, None),
        (11, 1, 400.0, 0.35, 0.70, 1.25, None),
        (24, 2, 650.0, 0.25, 0.55, 1.25, None),
        (1, 3, 700.0, 0.25, 0.55, 1.60, Some("church")),
        (1, 4, 1000.0, 0.20, 0.45, 1.40, Some("factory")),
        (1, 5, 800.0, 0.25, 0.50, 1.50, Some("mine head")),
    ] {
        let mut rows = vec![".".repeat(64); 64];
        rows[20].replace_range(20..20 + width, &"B".repeat(width));
        let buildings = name.map_or(json!([]), |name| json!([{"x":20,"y":20,"kind":name}]));
        let mut house = Match::new(&map(rows, buildings), options()).unwrap();
        let c = 20 * 64 + 20;
        assert_eq!(house.world.game.house[c], kind);
        let id = publish(&mut house, "rifle", 0, center(c));
        house.world.game.units.get_mut(&id).unwrap().garrison = Some(c);
        for remaining in [hp, hp * 0.5, 1.0] {
            let current = f64::from(house.world.game.cell_hp[c]);
            if current > remaining {
                damage(&mut house, c, current - remaining);
            }
            let u = &house.world.game.units[&id];
            close(
                house.cover_mul(u, Point { x: 1.0, z: 1.0 }).unwrap(),
                worn + (whole - worn) * remaining / hp,
            );
            close(
                vision::vision_range(&house.world.game, &house.rules, u),
                vision::vision_of(&house.world.game, &house.rules, u) * sight,
            );
        }
    }
    let mut rows = vec![".".repeat(64); 64];
    rows[20].replace_range(20..21, "B");
    let mut m = Match::new(&map(rows, json!([])), options()).unwrap();
    let c = 20 * 64 + 20;
    let id = publish(&mut m, "rifle", 0, center(c));
    m.world.game.units.get_mut(&id).unwrap().garrison = Some(c);
    close(
        m.cover_mul(&m.world.game.units[&id], Point { x: 1.0, z: 1.0 })
            .unwrap(),
        0.55,
    );
    close(
        vision::vision_range(&m.world.game, &m.rules, &m.world.game.units[&id]),
        vision::vision_of(&m.world.game, &m.rules, &m.world.game.units[&id]) * 1.25,
    );
    damage(&mut m, c, 125.0);
    close(
        m.cover_mul(&m.world.game.units[&id], Point { x: 1.0, z: 1.0 })
            .unwrap(),
        0.7,
    );
    close(blast_hit(&mut m, id, None), 10.0);
    close(blast_hit(&mut m, id, Some(0.8)), 16.0);
    let flamer = publish(&mut m, "flamer", 1, Point { x: 31.0, z: 41.0 });
    let garrison_damage = direct(&mut m, flamer, id);
    m.world.game.units.get_mut(&id).unwrap().garrison = None;
    let plain_damage = direct(&mut m, flamer, id);
    let bonus = crate::combat::WeaponDamage::from_catalog("flamer")
        .unwrap()
        .flame
        .unwrap();
    close(garrison_damage, plain_damage * bonus);
}

#[test]
fn eng35_house_failure_evicts_and_fire_hurts_the_garrison() {
    let mut rows = vec![".".repeat(64); 64];
    rows[20].replace_range(20..21, "B");
    let mut m = Match::new(&map(rows, json!([])), options()).unwrap();
    let c = 20 * 64 + 20;
    let id = publish(&mut m, "rifle", 0, center(c));
    m.world.game.units.get_mut(&id).unwrap().garrison = Some(c);
    let command = Command::Move {
        orders: vec![crate::match_commands::MoveOrder {
            unit: i64::from(id),
            at: Point { x: 51.0, z: 51.0 },
        }],
        control_ids: Some(vec![i64::from(id)]),
        attack_move: false,
        queue: false,
        together: false,
        face: None,
    };
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    assert_eq!(m.world.game.units[&id].garrison, None);
    let u = &m.world.game.units[&id];
    assert_eq!(m.world.game.flags_at(u.x, u.z) & crate::game::MOVE, 0);
    // A separately authored occupied-cell input isolates fire and eviction effects.
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = center(c).x;
    u.z = center(c).z;
    u.garrison = Some(c);
    u.path.clear();
    m.grid = SpatialGrid::new(&m.world.game.units, 16.0);
    let hp = m.world.game.units[&id].hp;
    env(&mut m, |e, w, ctx| {
        e.ignite(w, c, ctx).unwrap();
        e.burn(w, 0.5, ctx).unwrap();
    });
    close(m.world.game.units[&id].hp, hp - 4.0);
    close(m.world.game.units[&id].supp, 6.0);
    let before = m.world.game.units[&id].hp;
    let effects = damage(&mut m, c, 250.0);
    assert_eq!(m.world.game.units[&id].garrison, None);
    close(m.world.game.units[&id].hp, before - hp * 0.3);
    assert!(effects.iter().any(|e| matches!(e, Effect::Collapse(_))));
    assert!(!m.world.fires.contains_key(&c));
}

#[test]
fn eng54_ground_cover_tracks_depth_and_remaining_cell_health() {
    let mut m = scene();
    let c = 25 * 64 + 25;
    for (ch, expected) in [(b'O', 0.6), (b'+', 0.55), (b'#', 1.0), (b'H', 1.0)] {
        m.world.set_cell(c, ch).unwrap();
        m.world.game.wear.as_mut().unwrap()[c] = 0.0;
        close(terrain::cover_q(&m.world.game, &m.rules, c), expected);
        if ch == b'+' {
            m.world.game.wear.as_mut().unwrap()[c] = 1.0;
            close(terrain::cover_q(&m.world.game, &m.rules, c), 1.4);
        } else if ch != b'O' {
            m.world.game.cell_hp[c] *= 0.5;
            close(terrain::cover_q(&m.world.game, &m.rules, c), 0.75);
        }
    }
    for ch in *b"MF+" {
        m.world.set_cell(c, ch).unwrap();
        let wear = m.world.game.wear.as_ref().unwrap()[c];
        assert!(if ch == b'+' {
            (0.3..=0.6).contains(&wear)
        } else {
            (0.2..=0.8).contains(&wear)
        });
    }
}

#[test]
fn eng60_vehicle_contact_forms_mud_with_wetness_and_crusher_scaling() {
    for (name, crush) in [("tank", 1.0), ("truck", 0.5)] {
        for (ch, mud) in [(b'.', 1.0), (b'M', 0.5)] {
            for (wet, level) in [(0.0, 0), (0.5, 0), (0.5, -1)] {
                let mut m = scene();
                let c = 25 * 64 + 25;
                let at = center(c);
                m.world.set_cell(c, ch).unwrap();
                m.world.set_level(c, level).unwrap();
                m.world.game.wear.as_mut().unwrap()[c] = 0.0;
                m.world.game.wet = Some(wet);
                let id = publish(&mut m, name, 0, at);
                m.apply_moved_ground_contact(
                    id,
                    Pose {
                        x: at.x - 1.0,
                        z: at.z,
                        rot: 0.0,
                    },
                    None,
                )
                .unwrap();
                let expected =
                    (0.05 * crush * mud * (1.0 + 3.0 * wet * if level < 0 { 2.0 } else { 1.0 }))
                        as f32;
                assert_eq!(m.world.game.wear.as_ref().unwrap()[c], expected);
            }
        }
    }
    let mut m = scene();
    let c = 25 * 64 + 25;
    let at = center(c);
    m.world.game.wear.as_mut().unwrap()[c] = 0.99;
    let id = publish(&mut m, "tank", 0, at);
    m.apply_moved_ground_contact(
        id,
        Pose {
            x: at.x - 1.0,
            z: at.z,
            rot: 0.0,
        },
        None,
    )
    .unwrap();
    assert_eq!(m.world.game.chars[c], b'M');
    close(f64::from(m.world.game.wear.as_ref().unwrap()[c]), 0.0);
}

#[test]
fn eng61_dry_dust_lasts_one_second_and_extends_target_visibility() {
    for (wet, ch, dust) in [
        (0.0, b'.', true),
        (0.3, b'.', false),
        (0.0, b'M', false),
        (0.0, b'F', false),
    ] {
        let mut m = scene();
        let c = 25 * 64 + 25;
        let at = center(c);
        m.world.set_cell(c, ch).unwrap();
        m.world.game.wet = Some(wet);
        m.world.game.tick = 100;
        let id = publish(&mut m, "tank", 0, at);
        m.apply_moved_ground_contact(
            id,
            Pose {
                x: at.x - 1.0,
                z: at.z,
                rot: 0.0,
            },
            None,
        )
        .unwrap();
        assert_eq!(vision::dusty(&m.world.game, &m.world.game.units[&id]), dust);
        m.world.game.tick = 120;
        assert!(!vision::dusty(&m.world.game, &m.world.game.units[&id]));
    }
    let mut m = scene();
    m.world.game.units.clear();
    m.world.game.tick = 100;
    let catalog = Catalog::compiled().unwrap();
    for (id, owner, name, x) in [(100, 0, "rifle", 51.0), (101, 1, "tank", 91.0)] {
        let u = crate::unit_spawn::initial_unit(
            &m.rules,
            catalog,
            m.rules.kind(name).unwrap(),
            id,
            owner,
            Point { x, z: 51.0 },
        )
        .unwrap();
        m.world.game.units.insert(id, u);
    }
    let grid = SpatialGrid::new(&m.world.game.units, 16.0);
    vision::update_vision(&mut m.world.game, &m.rules, &grid);
    assert!(!m.world.game.players[0].sees(101));
    m.world.game.units.get_mut(&101).unwrap().dust = Some(100.0);
    vision::update_vision(&mut m.world.game, &m.rules, &grid);
    assert!(
        m.world.game.players[0].sees(101),
        "40 m is inside 36 m times 1.3"
    );
    m.world.game.tick = 120;
    vision::update_vision(&mut m.world.game, &m.rules, &grid);
    assert!(!m.world.game.players[0].sees(101));
}

#[test]
fn eng71_idle_infantry_escapes_fire_and_burnout_changes_woods() {
    // Inferred wooden shed sections at dry, partial and full rain intensities.
    for (rain, loss) in [(0.0, 10.0), (0.5, 6.5), (1.0, 3.0)] {
        let mut rows = vec![".".repeat(64); 64];
        rows[20].replace_range(20..21, "B");
        let mut house = Match::new(&map(rows, json!([])), options()).unwrap();
        let c = 20 * 64 + 20;
        assert_eq!(house.world.material_name(c, false), Some("wood"));
        house.world.game.rain = rain;
        let hp = house.world.game.sections[&(c as u32)].hp;
        env(&mut house, |e, w, ctx| {
            e.ignite(w, c, ctx).unwrap();
            assert!(w.fires.contains_key(&c));
            e.burn(w, 1.0, ctx).unwrap();
        });
        let section = &house.world.game.sections[&(c as u32)];
        close(section.hp, hp - loss);
        assert!(!section.failed);
    }
    let mut m = scene();
    let c = 25 * 64 + 25;
    m.world.set_cell(c, b'H').unwrap();
    m.world.set_cell(c + 1, b'H').unwrap();
    let id = publish(&mut m, "rifle", 0, center(c));
    let hp = m.world.game.units[&id].hp;
    env(&mut m, |e, w, ctx| {
        e.ignite(w, c, ctx).unwrap();
        e.burn(w, 0.5, ctx).unwrap();
    });
    let u = &m.world.game.units[&id];
    close(u.hp, hp - 4.0);
    close(u.supp, 6.0);
    assert_eq!(u.path.len(), 1);
    let to = m.world.game.cell_of(u.path[0].x, u.path[0].z).unwrap();
    assert_ne!(m.world.game.chars[to], b'H');
    assert!(!m.world.fires.contains_key(&to));
    let woods = c + 128;
    m.world.set_cell(woods, b'O').unwrap();
    env(&mut m, |e, w, ctx| {
        e.ignite(w, woods, ctx).unwrap();
        e.burn(w, 20.0, ctx).unwrap();
    });
    assert_eq!(m.world.game.chars[woods], b'.');
    assert_eq!(m.world.burnt[woods], 1);
    let near = 25 * 64 + 24;
    let hidden = 50 * 64 + 5;
    for c in [near, hidden] {
        m.world.set_cell(c, b'H').unwrap();
        env(&mut m, |e, w, ctx| e.ignite(w, c, ctx).unwrap());
    }
    let cache = SnapshotCache::new(&m).unwrap();
    let fires = cache.recipient(0).unwrap().fields["fires"]
        .as_array()
        .unwrap()
        .clone();
    assert!(fires.contains(&json!(near)));
    assert!(!fires.contains(&json!(hidden)));
}

#[test]
fn eng78_wet_ground_leaves_roads_unchanged_and_blocks_ignition() {
    let mut m = scene();
    let c = 25 * 64 + 25;
    for (ch, dry, wet) in [(b'.', 1.0, 0.8), (b'D', 1.35, 1.35), (b'M', 0.7, 0.56)] {
        m.world.set_cell(c, ch).unwrap();
        m.world.game.wear.as_mut().unwrap()[c] = 0.0;
        m.world.game.wet = Some(0.0);
        close(terrain::ground_mul(&m.world.game, &m.rules, c, true), dry);
        m.world.game.wet = Some(1.0);
        close(terrain::ground_mul(&m.world.game, &m.rules, c, true), wet);
    }
    m.world.set_cell(c, b'.').unwrap();
    m.world.set_level(c, -1).unwrap();
    m.world.game.wet = Some(0.5);
    close(terrain::ground_mul(&m.world.game, &m.rules, c, true), 0.8);
    m.world.set_cell(c, b'F').unwrap();
    m.world.game.wear.as_mut().unwrap()[c] = 0.0;
    close(
        terrain::ground_mul(&m.world.game, &m.rules, c, true),
        0.75 * 0.7,
    );
    m.world.set_cell(c, b'.').unwrap();
    m.world.game.wet = Some(0.36);
    env(&mut m, |e, w, ctx| e.ignite(w, c, ctx).unwrap());
    assert!(!m.world.fires.contains_key(&c));
    let mut seed = 7;
    let mut angle = 0.0;
    let mut strength = 0.0;
    let mut raining = true;
    let mut next = 1000.0;
    let mut rain = 1.0;
    let mut wet = 1.0;
    let mut x = 51.0;
    let mut z = 51.0;
    let mut remaining = 10.0;
    crate::weather_runtime::advance(
        1,
        1.0,
        Some(crate::weather::WeatherKind::Rain),
        crate::weather_runtime::Environment {
            seed: &mut seed,
            wind_angle: &mut angle,
            wind_strength: &mut strength,
            showers: Some(crate::weather_runtime::Showers {
                raining: &mut raining,
                next: &mut next,
                rain: &mut rain,
                wet: &mut wet,
            }),
        },
        &mut [crate::weather_runtime::Smoke {
            x: &mut x,
            z: &mut z,
            remaining: &mut remaining,
        }],
        None,
    )
    .unwrap();
    close(remaining, 8.5);
}

#[test]
fn eng79_mutation_rejects_invalid_layers_atomically_and_keeps_overlays_separate() {
    let mut m = scene();
    let c = 25 * 64 + 25;
    for change in [
        CellChange {
            ground: Some(b'?'),
            ..Default::default()
        },
        CellChange {
            object: Some(b'W'),
            ..Default::default()
        },
        CellChange {
            height: Some(5),
            ..Default::default()
        },
        CellChange {
            material: Some("unknown".into()),
            ..Default::default()
        },
    ] {
        let before = format!("{:?}", m.world);
        assert!(m.world.mutate_cell(c, change).is_err());
        assert_eq!(format!("{:?}", m.world), before);
    }
    m.world.set_cell(c, b'D').unwrap();
    m.world
        .mutate_cell(
            c,
            CellChange {
                object: Some(b'X'),
                mine: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(m.world.game.ground.as_ref().unwrap()[c], b'D');
    assert_eq!(m.world.game.objects.as_ref().unwrap()[c], b'X');
    assert_eq!(m.world.game.physical(c), b'X');
    assert_eq!(m.world.game.chars[c], b'N');
    assert_eq!(m.world.cell_log().last().unwrap().cell, c);
}

#[test]
fn eng74_ground_weather_scaling_reaches_canonical_vision_and_speed() {
    let mut m = scene();
    let rifle = publish(&mut m, "rifle", 0, Point { x: 51.0, z: 51.0 });
    let tank = publish(&mut m, "tank", 0, Point { x: 71.0, z: 51.0 });
    for (name, sight, infantry, vehicle, rain) in [
        ("clear", 1.0, 1.0, 1.0, 0.0),
        ("fog", 0.7, 1.0, 1.0, 0.0),
        ("rain", 1.0, 1.0, 1.0, 1.0),
        ("mud", 1.0, 0.9, 1.0, 0.0),
        ("snow", 0.9, 0.9, 0.85, 0.0),
    ] {
        m.world.game.weather = Some(m.rules.weather_kind(name).unwrap());
        m.world.game.rain = rain;
        for (id, speed) in [(rifle, infantry), (tank, vehicle)] {
            let u = &m.world.game.units[&id];
            close(
                crate::movement::weather_speed(&m.world.game, &m.rules, &m.rules.kinds[u.kind]),
                speed,
            );
            close(
                vision::vision_range(&m.world.game, &m.rules, u),
                m.rules.kinds[u.kind].vision * sight * (1.0 - 0.2 * rain),
            );
        }
    }
}

#[path = "engineering_runtime2_mine_tests.rs"]
mod engineering_runtime2_mine_tests;

#[path = "engineering_runtime2_visibility_tests.rs"]
mod engineering_runtime2_visibility_tests;
