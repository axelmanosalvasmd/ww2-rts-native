//! Original full-step construction replay through the sole Match runner.
//! Bare Source Map insertion order and removed historical actor objects are not native state.
//! Runtime proof requires the separately reviewed construction phase wiring.
use crate::shared::startup;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode, TechState, Technology},
    economy_state::ArmySize,
    game::Point,
    logistics::Reserves,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOutcome, UnitControl},
    match_end::MatchSample,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::{TickEvent, TickPathConfig, TickPhase},
    production::ProductionState,
    random::RandomSnapshot,
    unit_spawn,
    weather::WeatherChoice,
    world::Appearance,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn number(v: &Value) -> f64 {
    f64::from_bits(
        u64::from_str_radix(v.as_str().or_else(|| v["f64"].as_str()).unwrap(), 16).unwrap(),
    )
}
fn point(p: Point) -> Value {
    json!({"x":bits(p.x),"z":bits(p.z)})
}
fn tagged(v: Value) -> Value {
    match v {
        Value::Number(n) => json!({"f64":bits(n.as_f64().unwrap())}),
        Value::Array(a) => Value::Array(a.into_iter().map(tagged).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, tagged(v))).collect()),
        v => v,
    }
}
// The recorder encodes declared undefined properties as null before JSON serialization.
// Recover only the optional wire-presence differences found in the complete Source frame audit.
fn recorded_effects_json(recorded: &Value) -> Value {
    Value::Array(
        recorded
            .as_array()
            .expect("recorded Source effect frame")
            .iter()
            .map(|effect| {
                let mut row = effect.as_object().expect("recorded Source effect").clone();
                let kind = row
                    .get("k")
                    .and_then(Value::as_str)
                    .expect("recorded Source effect kind");
                let hurt = kind == "hurt";
                let section_collapse =
                    kind == "collapse" && row.get("section").is_some_and(Value::is_string);
                if hurt {
                    for key in ["fo", "local"] {
                        if row.get(key).is_some_and(Value::is_null) {
                            row.remove(key);
                        }
                    }
                } else if section_collapse && row.get("to").is_some_and(Value::is_null) {
                    row.remove("to");
                }
                Value::Object(row)
            })
            .collect(),
    )
}
fn refresh(m: &mut Match) {
    m.grid.rebuild(&m.world.game.units);
    ww2_core::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    for owner in 0..m.world.game.players.len() {
        m.refresh_recipient_routing(owner).unwrap();
    }
}
fn tier(m: &mut Match, slot: usize, value: u8) {
    // Trusted scene initialization only. Reconstruct no active production or research.
    let mut tech = Vec::new();
    let mut buildings = Vec::new();
    for p in 0..m.start.economy.players.len() {
        let row = m.production.player(p).unwrap();
        assert!(row.jobs().is_empty());
        let t = row.tech();
        tech.push(
            TechState::new(
                if p == slot { value } else { t.tier() },
                t.level(Technology::Infantry),
                t.level(Technology::Guns),
                t.level(Technology::Armor),
            )
            .unwrap(),
        );
    }
    for &id in m.world.game.units.keys() {
        if let Some(b) = m.production.building(id) {
            assert!(b.jobs().is_empty());
            assert_eq!(b.progress(), 0.);
            buildings.push(id);
        }
    }
    assert_eq!(m.production.next_job_id(), 0);
    let mut staged = ProductionState::new(tech).unwrap();
    for id in buildings {
        staged.initialize_building(id).unwrap();
    }
    m.production = staged;
}
fn make(c: &Value) -> Match {
    let s = &c["setup"];
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&s["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
            players: vec![
                PlayerConfig {
                    name: "Crew".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "Opposition".into(),
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
            tech: s["opts"]["tech"].as_bool().unwrap(),
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    let constructor_random = m.start.random.snapshot();
    assert_eq!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        s["initialActorOrder"]
    );
    for row in s["spawn"].as_array().unwrap() {
        let id = row["id"].as_u64().unwrap() as u32;
        assert_eq!(m.next_unit_id, id);
        assert!(!m.world.game.units.contains_key(&id));
        assert!(!m.commands.units.contains_key(&id));
        assert!(
            m.combat.get(id).is_none()
                && m.firing.get(id).is_none()
                && m.abilities.get(id).is_none()
                && m.recovery.get(id).is_none()
        );
        let kind = m.rules.kind(row["type"].as_str().unwrap()).unwrap();
        let u = unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            row["owner"].as_u64().unwrap() as usize,
            Point {
                x: row["x"].as_f64().unwrap(),
                z: row["z"].as_f64().unwrap(),
            },
        )
        .unwrap();
        m.commands.insert(
            id,
            UnitControl::new_for_mode(&u, &m.rules, Mode::Default).unwrap(),
        );
        m.combat.initialize(id);
        m.firing.initialize(id);
        m.abilities.initialize(id);
        m.recovery.initialize(id);
        m.world.game.units.insert(id, u);
        m.next_unit_id = id.checked_add(1).unwrap();
    }
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for ctl in m.commands.units.values_mut() {
        ctl.hold_fire = true;
        ctl.auto_cast = false;
        ctl.auto_retreat = false;
    }
    m.start.economy.players[0].mp = 5000.;
    if let Some(t) = s["prime"]["player0"]["tier"].as_u64() {
        tier(&mut m, 0, t as u8);
    }
    if let Some(p) = s["prime"]["observer"].as_object() {
        let u = m
            .world
            .game
            .units
            .get_mut(&(p["id"].as_u64().unwrap() as u32))
            .unwrap();
        u.x = p["x"].as_f64().unwrap();
        u.z = p["z"].as_f64().unwrap();
    }
    m.world.take_journal();
    refresh(&mut m);
    m.initialize_tick_scene(
        0,
        TickPathConfig {
            budget: 4096,
            retry_limit: u32::try_from(s["prime"]["pathRetryLimit"].as_u64().unwrap()).unwrap(),
        },
    )
    .unwrap();
    assert_eq!(
        m.start.random.snapshot(),
        constructor_random,
        "scene setup must preserve constructor streams"
    );
    m
}
fn raw_reserve(v: &Value) -> Reserves {
    Reserves {
        ammo: v["ammo"].as_f64().unwrap(),
        ammo_max: v["ammoMax"].as_f64().unwrap(),
        ammo_price: v["ammoPrice"].as_f64().unwrap(),
        ammo_credit: v["ammoCredit"].as_f64(),
        provisions: v["provisions"].as_f64().unwrap(),
        fuel: v["fuel"].as_f64(),
        emergency: v["emergency"].as_f64(),
        warning: v["warning"].as_f64(),
        forced: v["forced"].as_bool().unwrap(),
        stranded: v["stranded"].as_bool().unwrap(),
        exhausted: v["exhausted"].as_bool().unwrap(),
        supply_rate: None,
        cut_for: None,
    }
}
fn reserve(m: &Match, id: u32) -> Value {
    m.logistics.get(id).map(|r|tagged(json!({"ammo":r.ammo,"ammoMax":r.ammo_max,"ammoPrice":r.ammo_price,"ammoCredit":r.ammo_credit,"provisions":r.provisions,"fuel":r.fuel,"emergency":r.emergency,"warning":r.warning,"forced":r.forced,"stranded":r.stranded,"exhausted":r.exhausted}))).unwrap_or(Value::Null)
}
fn prime(m: &mut Match, aliases: &BTreeMap<String, u32>, a: &Value) {
    if let Some(rows) = a["units"].as_object() {
        for (name, fields) in rows {
            let id = aliases[name];
            for (k, v) in fields.as_object().unwrap() {
                match k.as_str() {
                    "hp" => m.world.game.units.get_mut(&id).unwrap().hp = v.as_f64().unwrap(),
                    "x" => m.world.game.units.get_mut(&id).unwrap().x = v.as_f64().unwrap(),
                    "z" => m.world.game.units.get_mut(&id).unwrap().z = v.as_f64().unwrap(),
                    "garrison" => {
                        let cell = v.as_u64().unwrap() as usize;
                        assert_eq!(
                            m.world.game.chars[cell], b'B',
                            "declared garrison must name a real map house"
                        );
                        m.world.game.units.get_mut(&id).unwrap().garrison = Some(cell);
                    }
                    "follow" => {
                        m.commands.units.get_mut(&id).unwrap().follow = v.as_u64().unwrap() as u32
                    }
                    "logistics" => {
                        if v.is_null() {
                            m.logistics.remove(id);
                        } else {
                            m.logistics.insert(id, raw_reserve(v));
                        }
                    }
                    k => panic!("unrepresented declared unit prime {k}"),
                }
            }
        }
    }
    if let Some(rows) = a["players"].as_object() {
        for (slot, fields) in rows {
            let slot = slot.parse::<usize>().unwrap();
            for (k, v) in fields.as_object().unwrap() {
                match k.as_str() {
                    "mp" => m.start.economy.players[slot].mp = v.as_f64().unwrap(),
                    "tier" => tier(m, slot, v.as_u64().unwrap() as u8),
                    k => panic!("unrepresented declared player prime {k}"),
                }
            }
        }
    }
    refresh(m);
}
fn patch(state: &mut Value, patch: &Value) {
    for change in patch.as_array().unwrap() {
        let path = change[0].as_array().unwrap();
        if path.is_empty() {
            *state = change[1].clone();
            continue;
        }
        let mut at = &mut *state;
        for key in &path[..path.len() - 1] {
            at = if let Some(n) = key.as_u64() {
                &mut at[n as usize]
            } else {
                &mut at[key.as_str().unwrap()]
            };
        }
        let key = path.last().unwrap();
        if let Some(n) = key.as_u64() {
            at[n as usize] = change[1].clone();
        } else if change.as_array().unwrap().len() == 1 {
            at.as_object_mut().unwrap().remove(key.as_str().unwrap());
        } else {
            at[key.as_str().unwrap()] = change[1].clone();
        }
    }
}
fn order(c: &Command) -> Value {
    match c {
        Command::Assist {
            ids,
            target,
            at: Some(p),
            queue: false,
        } => tagged(json!({"t":"assist","ids":ids,"id":target,"x":p.x,"z":p.z})),
        c => panic!("unrepresented queued Source order {c:?}"),
    }
}
fn paid(m: &Match, id: u32) -> Value {
    m.field_works.jobs().get(&id).map(|d|json!({"x":bits(d.at.x),"z":bits(d.at.z),"kind":d.kind.as_str(),"cells":d.cells.iter().map(|c|json!([c.cell,char::from(c.ch).to_string()])).collect::<Vec<_>>(),"t":bits(d.elapsed),"dir":bits(d.direction),"front":d.front.map(bits),"n":d.original_cells,"reach":d.reach.map(bits),"project":d.project})).unwrap_or(Value::Null)
}
#[derive(Default)]
struct Cursors {
    new_cells: usize,
    journal: usize,
}
fn compare(
    m: &Match,
    e: &Value,
    rng: &RandomSnapshot,
    cursors: &mut Cursors,
    effects: &Value,
    context: &str,
) {
    macro_rules! same {
        ($a:expr,$b:expr,$field:expr) => {
            assert_eq!($a, $b, "{context}: {}", $field)
        };
    }
    same!(json!(m.world.game.tick), e["tick"], "clock");
    same!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        e["actorOrder"],
        "actor order"
    );
    same!(json!(m.next_unit_id), e["nextId"], "ID allocation");
    same!(m.start.random.snapshot(), *rng, "quiet match streams");
    same!(
        json!(m.start.environment_seed),
        e["seed"],
        "living environment seed"
    );
    same!(bits(m.start.wind.0), e["wind"]["a"], "wind angle");
    same!(bits(m.start.wind.1), e["wind"]["v"], "wind strength");
    assert!(m.start.living.is_none() && e["wx"].is_null());
    same!(
        tagged(
            json!({"now":m.start.weather.now.name(),"next":m.start.weather.next.map(|w|w.name()),"at":m.start.weather.at})
        ),
        e["weather"],
        "weather plan"
    );
    assert!(m.world.game.mode.is_none() && e["mode"].is_null());
    same!(m.options.mode, Mode::Default, "actual default base mode");
    for s in e["units"].as_array().unwrap() {
        let id = s["id"].as_u64().unwrap() as u32;
        let u = &m.world.game.units[&id];
        let c = &m.commands.units[&id];
        same!(json!(m.rules.kinds[u.kind].name), s["type"], "type");
        same!(json!(u.owner), s["owner"], "owner");
        for (k, n) in [
            ("hp", u.hp),
            ("x", u.x),
            ("z", u.z),
            ("rot", u.rot),
            ("aim", c.aim),
            ("repath", u.repath),
            ("moveSpeed", u.move_speed),
        ] {
            same!(bits(n), s[k], format!("actor{id} {k}"));
        }
        same!(
            u.built.map(bits).unwrap_or(Value::Null),
            s["built"],
            "built"
        );
        same!(json!(u.cells), s["cells"], "footprint");
        let structure = m
            .world
            .structures
            .iter()
            .find(|t| t.entity_id == Some(id))
            .map(|t| t.id.as_str());
        same!(
            json!(structure),
            s["structureId"],
            "actor structure identity"
        );
        same!(paid(m, id), s["dig"], "paid work");
        same!(u.dig, m.field_works.is_active(id), "synchronous dig");
        same!(
            json!(m.field_works.membership(id)),
            s["entrench"],
            "entrench membership"
        );
        assert_eq!(u.entrench, m.field_works.membership(id).is_some());
        let build = s["build"].as_u64().unwrap_or(0) as u32;
        same!(c.build_target.unwrap_or(0), build, "canonical build target");
        same!(u.build, c.build_target.is_some(), "synchronous build flag");
        same!(
            json!(u.path.iter().copied().map(point).collect::<Vec<_>>()),
            s["path"],
            "path"
        );
        same!(
            json!(c.orders.iter().map(order).collect::<Vec<_>>()),
            s["orders"],
            "queued orders"
        );
        same!(c.follow, s["follow"].as_u64().unwrap_or(0) as u32, "follow");
        same!(
            json!(u.garrison.map_or(-1, |n| n as i64)),
            s["garrison"],
            "garrison"
        );
        same!(
            json!(m.combat.get(id).unwrap().carrier.unwrap_or(0)),
            s["riding"],
            "riding"
        );
        for (k, b) in [("holdFire", c.hold_fire), ("holdPos", u.hold_pos)] {
            let expected = s.get(k).expect("recorded Source stance");
            assert!(
                expected.is_boolean() || expected.is_null(),
                "{context}: {k} must be a boolean or unset Source stance"
            );
            same!(b, expected.as_bool().unwrap_or(false), k);
        }
        for (k, b) in [
            ("auto", c.auto_cast),
            ("autoRetreat", c.auto_retreat),
            ("retreating", u.retreating),
        ] {
            same!(json!(b), s[k], k);
        }
        same!(
            u.still.map(bits).unwrap_or(Value::Null),
            s["still"],
            "still"
        );
        let outcome = c.outcome.map(|o| match o {
            MoveOutcome::Interrupted => "interrupted",
            MoveOutcome::Moving => "moving",
            MoveOutcome::Unreachable => "unreachable",
            MoveOutcome::Arrived => "completed",
        });
        same!(json!(outcome), s["moveOutcome"], "route outcome");
        same!(
            json!(c.outcome_tick),
            s["moveOutcomeTick"],
            "route outcome tick"
        );
        if !s["moveResult"].is_null() {
            let kind = match c.outcome {
                Some(MoveOutcome::Unreachable) => "unreachable",
                Some(MoveOutcome::Arrived) => "completed",
                o => panic!("{context}: unrepresented route result {o:?}"),
            };
            same!(
                tagged(json!([kind, c.outcome_tick.unwrap()])),
                s["moveResult"],
                "route result"
            );
        }
        if let Some(prod) = m.production.building(id) {
            same!(
                json!(prod.queue().collect::<Vec<_>>()),
                s["queue"],
                "site production queue"
            );
            same!(bits(prod.progress()), s["prog"], "site production progress");
            assert!(prod.jobs().is_empty() && s["productionJobs"] == json!([]));
        } else {
            assert!(s["queue"].is_null() && s["prog"].is_null() && s["productionJobs"].is_null());
        }
        if s.get("logistics").is_some() {
            same!(reserve(m, id), s["logistics"], "reserve debit order");
            same!(
                bits(m.logistics.recovery_rate(id)),
                s["recoveryRate"],
                "recovery rate"
            );
        }
    }
    // Historical Source observer objects retain removed actors; membership and every live body are canonical.
    for s in e["sites"].as_array().unwrap() {
        let id = s["id"].as_u64().unwrap() as u32;
        same!(
            m.world.game.units.contains_key(&id),
            s["live"].as_bool().unwrap(),
            "site live membership"
        );
        if let Some(u) = m.world.game.units.get(&id) {
            same!(bits(u.hp), s["hp"], "live site HP");
            same!(u.built.map(bits).unwrap(), s["built"], "live site progress");
            same!(json!(u.cells), s["cells"], "live site footprint");
        }
    }
    same!(
        m.world.structures.len(),
        e["structures"].as_array().unwrap().len(),
        "structure count"
    );
    for (a, s) in m
        .world
        .structures
        .iter()
        .zip(e["structures"].as_array().unwrap())
    {
        same!(json!(a.id), s["id"], "structure order");
        same!(json!(a.kind), s["kind"], "structure kind");
        same!(json!(a.entity_id), s["entityId"], "structure entity");
        same!(
            a.section_ids.len(),
            s["sections"].as_array().unwrap().len(),
            "section count"
        );
        for ((id, cell), source) in a
            .section_ids
            .iter()
            .zip(&a.cells)
            .zip(s["sections"].as_array().unwrap())
        {
            let (meta, hp) = m.world.section_by_id(id).unwrap();
            let state = if hp.failed {
                "failed"
            } else {
                match meta.appearance {
                    Appearance::Intact => "intact",
                    Appearance::Damaged => "damaged",
                    Appearance::Burning => "burning",
                }
            };
            let row = tagged(
                json!({"id":meta.id,"structureId":meta.structure_id,"entityId":a.entity_id,"c":cell,"hp":hp.hp,"maxHp":hp.max_hp.unwrap(),"material":meta.material,"anchor":meta.anchor,"supports":meta.supports,"state":state,"rubble":meta.rubble}),
            );
            for (k, v) in row.as_object().unwrap() {
                same!(v, &source[k], format!("section{id} {k}"));
            }
            let fail = &meta.failure;
            if source.get("direction").is_some() {
                same!(
                    fail.direction
                        .map(|p| tagged(json!({"x":p.x,"z":p.z})))
                        .unwrap_or(Value::Null),
                    source["direction"],
                    "failure direction"
                );
            }
            for (k, n) in [
                ("impulse", fail.impulse),
                ("failedAt", fail.failed_at),
                ("settledAt", fail.settled_at),
            ] {
                if source.get(k).is_some() {
                    same!(
                        n.map(|n| tagged(json!(n))).unwrap_or(Value::Null),
                        source[k],
                        k
                    );
                }
            }
        }
    }
    let source_cells = e["structuralCellOrder"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_u64().unwrap() as usize)
        .collect::<BTreeSet<_>>();
    same!(
        m.world.sections.keys().copied().collect::<BTreeSet<_>>(),
        source_cells,
        "structural lookup membership (Map order unrepresented)"
    );
    let source_buildings = e["buildingCells"].as_array().unwrap();
    same!(
        m.building_cells.len(),
        source_buildings.len(),
        "building lookup count"
    );
    for row in source_buildings {
        let c = number(&row[0]) as usize;
        let b = &m.building_cells[&c];
        same!(
            tagged(json!({"id":b.unit,"owner":b.owner,"x":b.at.x,"z":b.at.z})),
            row[1],
            "building lookup body (Map order unrepresented)"
        );
    }
    for (slot, s) in e["players"].as_array().unwrap().iter().enumerate() {
        let p = &m.start.economy.players[slot];
        same!(json!(slot), s["slot"], "slot");
        same!(json!(p.team), s["team"], "team");
        same!(json!(p.out), s["out"], "out");
        same!(json!(p.away), s["away"], "away");
        for (k, n) in [("mp", p.mp), ("inc", p.inc), ("vp", p.vp)] {
            same!(bits(n), s[k], k);
        }
        for (k, n) in [("mun", p.mun), ("fuel", p.fuel)] {
            if s[k].is_null() {
                same!(n, 0., "source absent base resource");
            } else {
                same!(bits(n), s[k], k);
            }
        }
        if !s["tier"].is_null() {
            same!(
                json!(m.production.player(slot).unwrap().tech().tier()),
                s["tier"],
                "tier"
            );
        }
    }
    for (slot, s) in m.story.iter().enumerate() {
        for (k, n) in [
            ("kills", s.kills),
            ("losses", s.losses),
            ("built", s.built),
            ("captures", s.captures),
            ("mpSpent", s.mp_spent),
            ("supportCalls", s.support_calls),
            ("planesDowned", s.planes_downed),
        ] {
            if e["story"].is_null() {
                same!(n, 0., "absent story default");
            } else {
                same!(tagged(json!(n)), e["story"][slot][k], k);
            }
        }
    }
    if e["timeline"].is_null() {
        assert!(m.timeline.is_empty());
    } else {
        same!(
            tagged(json!(
                m.timeline
                    .iter()
                    .map(|s| match s {
                        MatchSample::Conquest { t, vp } => json!({"t":t,"vp":vp}),
                        MatchSample::Territories { .. } =>
                            panic!("construction fixture cannot represent a Territories timeline"),
                    })
                    .collect::<Vec<_>>()
            )),
            e["timeline"],
            "timeline"
        );
    }
    same!(
        tagged(effects.clone()),
        recorded_effects_json(&e["shots"]),
        "environment effects"
    );
    assert!(m.firing.events.is_empty() && m.support.events.is_empty());
    let falling=m.environment.falling_sections().iter().map(|s|{let b=&s.body;tagged(json!({"id":b.id,"kind":b.kind.as_str(),"x":b.x,"y":b.y,"z":b.z,"vx":b.vx,"vy":b.vy,"vz":b.vz,"dir":b.dir,"material":b.material,"mass":b.mass,"radius":b.radius,"origin":{"x":b.origin.x,"z":b.origin.z},"gravity":b.gravity,"drag":b.drag,"stamp":b.stamp,"born":b.born,"contacts":b.contacts,"settled":b.settled,"tilt":b.tilt,"section":s.section,"c":s.cell,"bridge":s.bridge,"at":s.at,"until":s.until,"impulse":s.impulse}))}).collect::<Vec<_>>();
    same!(json!(falling), e["falling"], "actual falling sections");
    let t = &e["terrain"];
    same!(
        tagged(json!(
            m.world.new_cells()[cursors.new_cells..]
                .iter()
                .map(startup::row)
                .collect::<Vec<_>>()
        )),
        t["newCells"],
        "ordered new terrain batch"
    );
    same!(
        json!(m.world.new_cells().len()),
        t["newCellsTotal"],
        "new terrain total"
    );
    same!(
        json!(&m.world.journal()[cursors.journal..]),
        t["journal"],
        "ordered journal batch"
    );
    same!(
        json!(m.world.journal().len()),
        t["journalTotal"],
        "journal total"
    );
    cursors.new_cells = m.world.new_cells().len();
    cursors.journal = m.world.journal().len();
    same!(
        tagged(json!(
            m.world
                .cell_log()
                .iter()
                .map(startup::row)
                .collect::<Vec<_>>()
        )),
        t["cellLog"],
        "cell log"
    );
    same!(
        json!([
            m.world.game.terrain_version,
            m.world.versions.obstruction,
            m.world.versions.infantry,
            m.world.versions.vehicle,
            m.world.versions.naval
        ]),
        t["versions"],
        "terrain versions"
    );
    for cell in t["cells"].as_array().unwrap() {
        let c = cell["c"].as_u64().unwrap() as usize;
        let g = &m.world.game;
        for (k, ch) in [
            ("ch", g.chars[c]),
            ("ground", g.ground.as_ref().unwrap()[c]),
            ("object", g.objects.as_ref().unwrap()[c]),
        ] {
            same!(json!(char::from(ch).to_string()), cell[k], k);
        }
        for (k, n) in [
            ("hpBits", g.cell_hp[c].to_bits()),
            ("wearBits", g.wear.as_ref().unwrap()[c].to_bits()),
            ("scarBits", m.world.scar[c].to_bits()),
        ] {
            same!(json!(n), cell[k], k);
        }
        same!(json!(g.flags[c]), cell["flags"], "flags");
        same!(
            json!(g.height.as_ref().map_or(0, |h| h[c])),
            cell["height"],
            "level"
        );
        same!(json!(m.world.mine_layer[c]), cell["mine"], "mine");
        same!(json!(m.world.cell_state[c]), cell["state"], "state");
        same!(json!(m.world.burnt[c]), cell["burnt"], "burnt");
        same!(
            json!(g.trench_front.as_ref().map(|v| v[c].to_bits())),
            cell["frontBits"],
            "front bits"
        );
    }
    same!(
        json!(
            m.world
                .mines
                .iter()
                .map(|(c, p)| json!([c, p]))
                .collect::<Vec<_>>()
        ),
        t["mines"],
        "mines"
    );
    same!(
        json!(
            m.world
                .mine_seen
                .iter()
                .map(|(c, p)| json!([c, p]))
                .collect::<Vec<_>>()
        ),
        t["mineSeen"],
        "mine seen"
    );
    if !e["pathStats"].is_null() {
        same!(
            m.commands.path_requests().stats().dropped as f64,
            number(&e["pathStats"]["dropped"]),
            "actual route drops"
        );
    }
    assert!(m.finish.is_none() && e["winner"].is_null());
    same!(
        e["randomDraws"],
        json!(1),
        "paired constructor-only source draw"
    );
}
#[test]
fn original_paid_construction_replays_full_match_ticks() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/construction-tick.json")).unwrap();
    let mut steps = 0;
    let mut snapshots = 0;
    let mut actions = 0;
    for c in fixture["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut m = make(c);
        let rng = m.start.random.snapshot();
        let mut aliases: BTreeMap<String, u32> =
            serde_json::from_value(c["setup"]["initialAliases"].clone()).unwrap();
        let mut expected = c["initial"].clone();
        let mut cursors = Cursors::default();
        let mut effects = json!([]);
        compare(&m, &expected, &rng, &mut cursors, &effects, name);
        snapshots += 1;
        let mut next = 0;
        for r in c["records"].as_array().unwrap() {
            actions += 1;
            let a = &r["action"];
            let op = a["op"].as_str().unwrap();
            let count = match op {
                "command" => {
                    let before = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
                    let cmd = Command::from_json(&a["cmd"]).unwrap();
                    let denial = m
                        .execute_command(a["slot"].as_u64().unwrap_or(0) as usize, &cmd, false)
                        .unwrap();
                    assert_eq!(
                        json!(denial.map(|d| d.name())),
                        r["result"],
                        "{name}: command refusal"
                    );
                    if let Some(alias) = a["as"].as_str() {
                        let fresh = m
                            .world
                            .game
                            .units
                            .keys()
                            .copied()
                            .filter(|id| !before.contains(id))
                            .collect::<Vec<_>>();
                        assert_eq!(fresh.len(), 1);
                        aliases.insert(alias.into(), fresh[0]);
                    }
                    1
                }
                "prime" => {
                    prime(&mut m, &aliases, a);
                    1
                }
                "steps" | "until" => r["steps"].as_u64().unwrap() as usize,
                op => panic!("unrepresented action {op}"),
            };
            for _ in 0..count {
                if op == "steps" || op == "until" {
                    let report = m
                        .advance_tick()
                        .unwrap_or_else(|err| panic!("{name} tick{}: {err}", m.world.game.tick));
                    assert!(report.commands.is_empty());
                    assert_eq!(
                        report
                            .phases
                            .iter()
                            .filter(|p| **p == TickPhase::Construction)
                            .count(),
                        1,
                        "{name}: single actual construction receipt"
                    );
                    assert!(
                        !report.unavailable.contains(&TickPhase::Construction),
                        "{name}: construction actuator unavailable"
                    );
                    let position = |phase| report.phases.iter().position(|p| *p == phase).unwrap();
                    assert!(position(TickPhase::Firing) < position(TickPhase::Construction));
                    assert!(position(TickPhase::Construction) < position(TickPhase::Separation));
                    effects
                        .as_array_mut()
                        .expect("owned construction caller effect batch")
                        .extend(report.events.iter().map(|event| match event {
                            TickEvent::Environment(effect) => effect.to_json(),
                            other => panic!(
                                "{name}: unexpected producer in quiet construction scene: {other:?}"
                            ),
                        }));
                    steps += 1;
                }
                patch(&mut expected, &c["patches"][next]);
                next += 1;
                snapshots += 1;
                assert_eq!(
                    json!(aliases),
                    expected["aliases"],
                    "{name}: canonical aliases"
                );
                compare(
                    &m,
                    &expected,
                    &rng,
                    &mut cursors,
                    &effects,
                    &format!("{name} snapshot{next}"),
                );
            }
            assert_eq!(next, r["lastSnapshot"].as_u64().unwrap() as usize);
        }
        assert_eq!(next + 1, c["snapshots"].as_u64().unwrap() as usize);
    }
    assert_eq!((actions, steps, snapshots), (113, 1919, 2013));
}

#[path = "construction_tick/base_selected.rs"]
mod base_selected;

#[path = "construction_tick/base28_corridor.rs"]
mod base28_corridor;
