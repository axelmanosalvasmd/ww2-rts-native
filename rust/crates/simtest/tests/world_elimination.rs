use crate::shared::world_scene;
use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    game::{LOCAL_DEFENDER_OWNER, Unit},
    match_end::MatchSample,
    match_runtime::Match,
    world::CellChange,
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/world-elimination.json")).unwrap()
}
fn encode(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(format!("{:016x}", n.as_f64().unwrap().to_bits())),
        Value::Array(a) => Value::Array(a.into_iter().map(encode).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, encode(v))).collect()),
        v => v,
    }
}
fn owner(v: usize) -> i64 {
    if v == LOCAL_DEFENDER_OWNER {
        -1
    } else {
        v as i64
    }
}
fn unit(u: &Unit) -> Value {
    json!([
        u.id,
        owner(u.owner),
        u.hp,
        u.garrison.map_or(-1, |c| c as i64)
    ])
}
// Explicit reference adapter: native World keeps the named local actor sentinel internally.
fn effect(value: &ww2_core::environment::Effect) -> Value {
    let mut value = value.to_json();
    if value["to"].as_u64() == Some(LOCAL_DEFENDER_OWNER as u64) {
        value["to"] = json!(-1);
    }
    value
}
fn snapshot(m: &Match, order: &[u32]) -> Value {
    let base = world_scene::projection(m);
    let mut value = json!({"regions":base["regions"],"players":base["players"],
 "units":encode(json!(order.iter().filter_map(|id|m.world.game.units.get(id)).map(unit).collect::<Vec<_>>())),
 "story":m.story.iter().map(|s|Value::Object(ww2_core::match_story::StoryKey::ALL.into_iter().map(|k|(k.as_str().into(),encode(json!(s.value(k))))).collect())).collect::<Vec<_>>(),
 "finish":m.finish.map(|f|encode(json!({"winner":f.winner,"reason":f.reason.as_str(),"endAt":{"x":f.end_at.x,"z":f.end_at.z},"endTick":f.end_tick}))),
 "timeline":m.timeline.iter().map(|s|encode(match s {MatchSample::Conquest{t,vp}=>json!({"t":t,"vp":vp}),MatchSample::Territories{t,points,structures}=>json!({"t":t,"points":points,"structures":structures})})).collect::<Vec<_>>(),
 "reveal":m.world.game.reveal,"nodes":encode(json!(m.world_runtime.as_ref().unwrap().nodes.iter().map(|n|n.depot.unwrap_or(0)).collect::<Vec<_>>()))});
    // Base projection already encodes float fields but integer team/ID/tick fields remain raw.
    for r in value["regions"].as_array_mut().unwrap() {
        for i in [0, 2, 5] {
            if r[i].is_number() {
                r[i] = encode(r[i].take());
            }
        }
    }
    value
}
fn row(v: &ww2_core::world::CellRow) -> Value {
    let d = &v.data;
    let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),"material":d.material,"groundMaterial":d.ground_material,"groundWear":d.ground_wear});
    if let Some(f) = d.trench_front {
        data["trenchFront"] = json!(f);
    }
    if d.mine {
        data["mine"] = json!(true);
    }
    if let Some(s) = &d.section {
        data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
    }
    json!([
        v.cell,
        char::from(v.ch).to_string(),
        v.height,
        v.state,
        data
    ])
}

fn setup(m: &mut Match, f: &Value, case: &Value) -> Vec<u32> {
    world_scene::reset(m, f, &case["input"]);
    // Same explicit restored terrain as the source fixture, before registering actual sections.
    m.world.structures.clear();
    m.world.sections.clear();
    m.world.game.sections.clear();
    m.world.detached_sections.clear();
    for cell in 0..m.world.game.chars.len() {
        if m.world.game.chars[cell] != b'.' {
            m.world.set_cell(cell, b'.').unwrap();
        }
    }

    m.environment = Default::default();
    m.start.environment_seed = 123456789;
    m.next_unit_id = 1000;
    m.start.wind = (0.7, 0.6);
    m.world.game.rain = 0.;
    m.world.game.wet = Some(0.);
    let mut order = Vec::new();
    for raw in case["input"]["units"].as_array().unwrap() {
        let id = raw["id"].as_u64().unwrap() as u32;
        order.push(id);
        m.abilities.initialize(id);
        m.recovery.initialize(id);
        let cells = raw["cells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_u64().unwrap() as usize)
            .collect::<Vec<_>>();
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.garrison = raw["garrison"].as_u64().map(|c| c as usize);
        u.cells = Some(cells.iter().map(|c| *c as u32).collect());
        if !cells.is_empty() {
            m.world.structures.retain(|s| s.entity_id != Some(id));
            let hp = m.world.game.units[&id].hp;
            let hp_per = Catalog::compiled()
                .unwrap()
                .unit(raw["type"].as_str().unwrap())
                .unwrap()
                .get("hpPer")
                .unwrap()
                .number()
                .unwrap();
            for cell in &cells {
                m.world
                    .mutate_cell(
                        *cell,
                        CellChange {
                            ground: Some(b'.'),
                            object: Some(b'K'),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
            m.world
                .register_building_structure(id, &cells, hp, hp_per)
                .unwrap();
        }
        if let Some(hp) = raw["postRegisterHp"].as_f64() {
            m.world.game.units.get_mut(&id).unwrap().hp = hp;
        }
    }
    m.world.take_new_cells();
    m.world.reset_journal();
    order
}
#[test]
fn actual_world_elimination_matches_original_order_collateral_and_overflow() {
    let f = fixture();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    let teams = f["cases"][0]["input"]["teams"].as_array().unwrap();
    for case in f["cases"].as_array().unwrap() {
        let mut m = world_scene::make_match(&f, teams);
        let order = setup(&mut m, &f, case);
        for raw in case["input"]["units"].as_array().unwrap() {
            let kind = m.rules.kind(raw["type"].as_str().unwrap()).unwrap();
            if m.rules.kinds[kind].air {
                assert!(raw.get("air").is_none());
                let id = raw["id"].as_u64().unwrap() as u32;
                assert_eq!(
                    m.world.game.units[&id].air,
                    Some(ww2_core::game::PlaneState::Base)
                );
                assert_eq!(m.world.game.units[&id].hp, raw["hp"].as_f64().unwrap());
            }
        }
        for step in case["steps"].as_array().unwrap() {
            m.world.game.tick = step["tick"].as_i64().unwrap();
            assert_eq!(
                snapshot(&m, &order),
                step["before"],
                "{} before",
                case["name"]
            );
            let identities = (
                m.world.game.flags.as_ptr(),
                m.world.game.chars.as_ptr(),
                m.world.initial().chars.as_ptr(),
            );
            let result = m
                .apply_world_capture_income_and_elimination(
                    &order,
                    &order,
                    step["dt"].as_f64().unwrap(),
                )
                .unwrap();
            assert_eq!(
                snapshot(&m, &order),
                step["after"],
                "{} after",
                case["name"]
            );
            assert_eq!(
                encode(json!(result.removed.iter().map(unit).collect::<Vec<_>>())),
                encode(step["removedUnits"].clone())
            );
            assert_eq!(
                encode(json!(result.effects.iter().map(effect).collect::<Vec<_>>())),
                step["shots"],
                "{} effects",
                case["name"]
            );
            assert_eq!(
                encode(json!(
                    m.world.new_cells().iter().map(row).collect::<Vec<_>>()
                )),
                step["newCells"],
                "{} rows",
                case["name"]
            );
            assert_eq!(json!(m.world.journal()), step["journal"]);
            assert_eq!(
                m.environment.falling_sections().len(),
                step["falling"].as_u64().unwrap() as usize
            );
            assert_eq!(
                m.start.environment_seed,
                step["seed"].as_u64().unwrap() as u32
            );
            assert_eq!(m.next_unit_id, step["nextId"].as_u64().unwrap() as u32);
            assert_eq!(
                (
                    m.world.game.flags.as_ptr(),
                    m.world.game.chars.as_ptr(),
                    m.world.initial().chars.as_ptr()
                ),
                identities
            );
            for unit in result.removed {
                assert!(!m.commands.units.contains_key(&unit.id));
                assert!(m.firing.get(unit.id).is_none());
                assert!(m.abilities.get(unit.id).is_none());
                assert!(m.recovery.get(unit.id).is_none());
                assert!(m.combat.get(unit.id).is_some());
            }
        }
    }
}
fn remembered(m: &Match) -> String {
    let memory = &m.world_runtime.as_ref().unwrap().memory;
    let team = memory.team(5).unwrap();
    let query = team.query();
    let terrain = query.navigation();
    let live = memory.live_for(0).unwrap();
    let live_view = live.live();
    format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
        team.rows(),
        team.version(),
        terrain.flags.as_ptr(),
        terrain.flags,
        terrain.heights.as_ref().map(|h| h.as_ptr()),
        terrain.heights,
        terrain.versions,
        live_view.fires.as_ptr(),
        live_view.fires,
        live_view.bunkers,
        live.wrecks()
    )
}
fn protected(m: &Match) -> String {
    format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
        m.world,
        m.start.economy,
        m.world_runtime.as_ref().unwrap().regions,
        m.environment,
        (-1..6)
            .map(|owner| m.grid.owned_by(&m.world.game.units, owner))
            .collect::<Vec<_>>(),
        m.combat,
        m.firing,
        m.commands.units,
        m.abilities,
        m.recovery,
        m.story
    )
}
#[test]
fn unsupported_and_late_world_guards_preserve_every_canonical_owner() {
    let f = fixture();
    let case = &f["cases"][3];
    let mutations: [fn(&mut Match); 7] = [
        |m| m.world.game.height = Some(Vec::new()),
        |m| m.story[0].captures = f64::NAN,
        |m| m.world.game.terrain_version = u32::MAX - 1,
        |m| m.world.game.units.get_mut(&12).unwrap().riding = true,
        |m| m.options.logistics = true,
        |m| m.world.game.units.get_mut(&12).unwrap().entrench = true,
        |m| m.world.game.units.get_mut(&12).unwrap().build = true,
    ];
    for (index, change) in mutations.into_iter().enumerate() {
        let mut m = world_scene::make_match(&f, case["input"]["teams"].as_array().unwrap());
        let order = setup(&mut m, &f, case);
        m.refresh_world_routing(0).unwrap();
        let memory_before = remembered(&m);
        change(&mut m);
        let before = protected(&m);
        let error = m
            .apply_world_capture_income_and_elimination(&order, &order, 0.05)
            .unwrap_err();
        if index == 1 {
            assert_eq!(error, "nonfinite match story counter");
        }
        assert_eq!(protected(&m), before);
        assert_eq!(remembered(&m), memory_before);
    }
}

#[allow(dead_code)]
use crate::shared::firing as firing_scene;
#[test]
fn administrative_removal_retains_captured_flight_xp_and_paid_metadata_has_no_refund() {
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/match-firing.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    let firing: Value = serde_json::from_str(&text).unwrap();
    let case = firing["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["variation"] == "flight-dead-shooter")
        .unwrap();
    let mut ordinary = firing_scene::make(case);
    ordinary.capture_firing_prior().unwrap();
    ordinary.world.game.tick = 1;
    ordinary.begin_firing_tick(&[1, 2, 3]).unwrap();
    ordinary.advance_firing_timers(1, 0.05).unwrap();
    ordinary.apply_unit_firing(1, false, 0.05).unwrap();
    ordinary.step_firing_flights().unwrap();
    firing_scene::compare(
        &firing_scene::snapshot(&mut ordinary),
        &case["steps"][0]["expected"],
        "captured ordinary flight",
    );
    let captured = ordinary.firing.flights.values().next().unwrap().clone();
    let f = fixture();
    let scene = &f["cases"][3];
    let mut m = world_scene::make_match(&f, scene["input"]["teams"].as_array().unwrap());
    let mut order = setup(&mut m, &f, scene);
    world_scene::update_unit(
        &mut m,
        &json!({"id":15,"owner":1,"type":"hq","x":900.0,"z":900.0,"hp":3000.0,"built":1.0}),
        true,
    );
    order.push(15);
    world_scene::update_unit(
        &mut m,
        &json!({"id":16,"owner":0,"type":"engineer","x":801.0,"z":421.0,"built":1.0}),
        true,
    );
    order.push(16);
    // A persistence witness imports a proven ordinary flight into World without launching or stepping it.
    let mut flight = captured.clone();
    flight.shooter = 12;
    flight.owner = 2;
    flight.source_kind = m.world.game.units[&12].kind;
    m.firing.flights.insert(flight.id, flight.clone());
    m.combat.get_mut(12).unwrap().xp = 4.5;
    let region_facts = m.world_economy_regions().unwrap();
    let units = m
        .world
        .game
        .units
        .values()
        .map(|u| ww2_core::production::UnitView {
            id: u.id,
            owner: (u.owner != LOCAL_DEFENDER_OWNER).then_some(u.owner),
            kind: &m.rules.kinds[u.kind].name,
            hp: u.hp,
            built: u.built.unwrap_or(0.0),
        })
        .collect::<Vec<_>>();
    let context = ww2_core::production::Context {
        mode: ww2_core::economy_state::ModeContext {
            kind: ww2_core::economy::Mode::World,
            defender_team: None,
            income_excluded_slot: None,
        },
        army: m.options.army,
        tech_enabled: false,
        naval: false,
        sudden_death: false,
        units: &units,
        drops: &[],
        world: Some(ww2_core::economy_state::WorldContext {
            regions: &region_facts,
        }),
    };
    m.start.economy.players[0].mp = 1000.0;
    m.start.economy.players[1].mp = 1000.0;
    m.production
        .queue_unit(
            &context,
            &mut m.start.economy,
            0,
            ww2_core::economy::Faction::Usa,
            "rifle",
            Some(40),
        )
        .unwrap();
    m.production
        .queue_unit(
            &context,
            &mut m.start.economy,
            1,
            ww2_core::economy::Faction::Usa,
            "rifle",
            Some(15),
        )
        .unwrap();
    assert!(m.production.building(40).unwrap().rows()[0].job.charge.mp > 0.0);
    let work = m
        .field_works
        .prepare_start(
            &m.world,
            &mut m.start.economy.players[0],
            ww2_core::field_works::StartRequest {
                control_eligible: true,
                owner: 0,
                unit: 16,
                kind: ww2_core::field_works::FortKind::Trench,
                at: ww2_core::game::Point { x: 801.0, z: 421.0 },
                direction: Some(0.0),
            },
            |_| true,
        )
        .unwrap();
    assert!(work.cost() > 0.0);
    work.commit(&mut m.field_works);
    m.world.game.units.get_mut(&16).unwrap().dig = true;

    let survivor = m.production.building(15).unwrap().clone();
    m.logistics.insert(
        12,
        ww2_core::logistics::Reserves {
            ammo: 3.,
            ammo_max: 4.,
            ammo_price: 2.,
            ammo_credit: Some(1.),
            provisions: 0.5,
            fuel: None,
            emergency: None,
            warning: None,
            forced: false,
            stranded: false,
            exhausted: false,
            supply_rate: None,
            cut_for: None,
        },
    );
    let survivor_control = m.commands.units[&14].clone();
    let wallets = m.start.economy.clone();
    let story = m.story.clone();
    m.apply_world_capture_income_and_elimination(&order, &order, 0.)
        .unwrap();
    assert_eq!(m.firing.flights.get(&flight.id), Some(&flight));
    assert_eq!(m.combat.get(12).unwrap().xp, 4.5);
    assert!(m.production.building(40).is_none());
    assert!(!m.field_works.is_active(16));
    assert!(m.logistics.get(12).is_none());
    assert_eq!(m.production.building(15), Some(&survivor));
    assert_eq!(m.commands.units[&14], survivor_control);
    for (before, after) in wallets.players.iter().zip(&m.start.economy.players) {
        assert_eq!(
            (before.mp, before.mun, before.fuel),
            (after.mp, after.mun, after.fuel)
        );
    }
    assert_eq!(m.story, story);

    // Source leaves this collapsed garrison in the current list with negative HP.
    // Represented World firing skips it before targeting, timers or output.
    let source_survivor = scene["steps"][0]["postWreckUnits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|unit| unit["id"] == 14)
        .unwrap();
    assert_eq!(
        encode(json!(m.world.game.units[&14].hp)),
        encode(source_survivor["hp"].clone())
    );
    assert!(m.world.game.units[&14].hp <= 0.0);
    let before = protected(&m);
    let end_state = snapshot(&m, &order);
    let firing = m.firing.clone();
    let paid_metadata = (
        m.production.clone(),
        m.field_works.clone(),
        m.logistics.clone(),
    );
    m.apply_unit_firing(14, false, 0.05).unwrap();
    assert_eq!(protected(&m), before);
    assert_eq!(snapshot(&m, &order), end_state);
    assert_eq!(m.firing, firing);
    assert_eq!(
        (&m.production, &m.field_works, &m.logistics),
        (&paid_metadata.0, &paid_metadata.1, &paid_metadata.2)
    );
}
