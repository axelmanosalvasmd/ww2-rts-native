//! Source-only full-step fixture loading and canonical Match projections.
//! Operative `react` uses the canonical control timer and must remain unchanged.
//! Ordinary `react` remains excluded until its tick phase is connected.
//! Source flight `duration` has no stored LiveFlight field; exact expiry,
//! trajectory and the linked ticket timer protect the represented duration facts.
//! Native objects eagerly allocate empty story/optional state; source absent
//! fields compare their canonical defaults. The source mutation tape has no
//! native property-write API and remains independent source-order evidence.
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    catalog::{Catalog, Value as RuleValue},
    companions::{Companions, EDGE_NAMES},
    conquest_end::{ConquestEndReason, ConquestFinish},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Point, Pose},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_abilities::AbilityState,
    match_combat::CombatState,
    match_commands::{MatchCommands, UnitControl},
    match_end::MatchSample,
    match_firing::{FiringState, LiveFlight},
    match_recovery::RecoveryState,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickEvent,
    random::{MatchRandom, StreamId},
    weather::WeatherChoice,
    world::CellRow,
};

pub fn fixture() -> Value {
    let mut text = String::new();
    GzDecoder::new(include_bytes!("../fixtures/fps-tick.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn n(v: &Value) -> f64 {
    v.as_f64().expect("finite original number")
}
fn id(v: &Value) -> u32 {
    n(v) as u32
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(_) => n(v) != 0.0,
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn point(v: &Value) -> Point {
    Point {
        x: n(&v["x"]),
        z: n(&v["z"]),
    }
}
fn point_json(p: Point) -> Value {
    json!({"x":p.x,"z":p.z})
}
fn pose_json(p: Pose) -> Value {
    json!({"x":p.x,"z":p.z,"rot":p.rot})
}
fn path(parent: &str, key: impl std::fmt::Display) -> String {
    if parent.is_empty() {
        key.to_string()
    } else {
        format!("{parent}.{key}")
    }
}
fn restore(v: &mut Value, bits: &Value, at: &str) {
    match v {
        Value::Number(_) => {
            let b = bits[at]
                .as_str()
                .unwrap_or_else(|| panic!("source bits absent at {at}"));
            let value = f64::from_bits(u64::from_str_radix(b, 16).unwrap());
            assert!(value.is_finite());
            *v = json!(value);
        }
        Value::Array(a) => {
            for (i, v) in a.iter_mut().enumerate() {
                restore(v, bits, &path(at, i));
            }
        }
        Value::Object(o) => {
            for (k, v) in o {
                restore(v, bits, &path(at, k));
            }
        }
        _ => {}
    }
}
pub fn decoded(encoded: &Value) -> Value {
    let mut v = encoded["raw"].clone();
    restore(&mut v, &encoded["floatBits"], "");
    v
}
pub fn arguments(action: &Value) -> Value {
    let mut v = Value::Object(
        action
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, _)| {
                !["delta", "result", "events", "rng", "argumentFloatBits"].contains(&k.as_str())
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    );
    restore(&mut v, &action["argumentFloatBits"], "");
    v
}
pub fn apply_delta(state: &mut Value, delta: &Value) {
    for change in delta.as_array().unwrap() {
        let c = change.as_array().unwrap();
        let p = c[0].as_array().unwrap();
        if p.is_empty() {
            *state = c[1].clone();
            continue;
        }
        let mut at = &mut *state;
        for k in &p[..p.len() - 1] {
            at = if let Some(k) = k.as_str() {
                at.get_mut(k).unwrap()
            } else {
                at.get_mut(k.as_u64().unwrap() as usize).unwrap()
            };
        }
        let k = p.last().unwrap();
        if c.len() == 1 {
            at.as_object_mut().unwrap().remove(k.as_str().unwrap());
        } else if let Some(k) = k.as_str() {
            at.as_object_mut().unwrap().insert(k.into(), c[1].clone());
        } else {
            at[k.as_u64().unwrap() as usize] = c[1].clone();
        }
    }
}
pub fn exact(actual: &Value, expected: &Value, context: &str) {
    match (actual, expected) {
        (Value::Number(_), Value::Number(_)) => assert_eq!(
            n(actual).to_bits(),
            n(expected).to_bits(),
            "{context}: numeric bits ({actual} vs {expected})"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{context}: array length");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                exact(a, b, &path(context, i));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{context}: field count");
            for (k, a) in a {
                exact(
                    a,
                    b.get(k).unwrap_or_else(|| panic!("{context}: missing {k}")),
                    &path(context, k),
                );
            }
        }
        _ => assert_eq!(actual, expected, "{context}"),
    }
}
fn fields(actual: &Value, expected: &Value, excluded: &[&str], context: &str) -> Value {
    Value::Object(
        expected
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| !excluded.contains(&k.as_str()))
            .map(|k| {
                let v = actual
                    .get(k)
                    .unwrap_or_else(|| panic!("{context}: represented field {k} absent"));
                (k.clone(), v.clone())
            })
            .collect(),
    )
}
fn compare_fields(actual: &Value, expected: &Value, excluded: &[&str], context: &str) {
    let a = fields(actual, expected, excluded, context);
    let b = Value::Object(
        expected
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, _)| !excluded.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    );
    exact(&a, &b, context);
}

pub fn make(f: &Value, initial: &Value, mode: Mode) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode,
            players: initial["players"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| PlayerConfig {
                    name: p["name"].as_str().unwrap().into(),
                    team: n(&p["team"]) as i32,
                    faction: Faction::try_from(n(&p["faction"]) as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    // Match::new stamps real starting bases. Keep its World terrain, sections,
    // logs, soak set and immutable initial terrain while removing the actors.
    let old: Vec<_> = m.world.game.units.keys().copied().collect();
    for id in old {
        m.production.remove_building(id);
    }
    m.world.game.units.clear();
    m.commands = MatchCommands::new(4096);
    m.firing = FiringState::default();
    m.combat = CombatState::default();
    m.abilities = AbilityState::default();
    m.recovery = RecoveryState::default();
    m.companions = Companions::default();
    m.recruit_metadata.clear();
    m.building_rallies.clear();
    m.next_unit_id = id(&initial["game"]["nextId"]);
    m.firing.next_flight = n(&initial["game"]["nextFlight"]) as u64;
    m.world.game.tick = n(&initial["game"]["tick"]) as i64;
    m.world.game.vision_tick = initial["game"]["visionTick"].as_f64().map(|n| n as i64);
    m.start.environment_seed = n(&initial["game"]["seed"]) as i64 as u32;
    m.start.wind = (
        n(&initial["game"]["wind"]["a"]),
        n(&initial["game"]["wind"]["v"]),
    );
    // The real seed701 MatchRandom stays untouched. Environment setup has its
    // separate source seed and wind, as in the existing firing fixtures.
    assert_eq!(m.start.random.snapshot().seed, 701);
    assert!(
        !m.start
            .random
            .snapshot()
            .streams
            .iter()
            .any(|(s, _)| *s == StreamId::Combat)
    );
    for p in initial["players"].as_array().unwrap() {
        let slot = id(&p["slot"]) as usize;
        m.start.players[slot].spawn = point(&p["spawn"]);
        m.start.players[slot].support = p["sup"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), n(v)))
            .collect();
        let e = &mut m.start.economy.players[slot];
        e.mp = n(&p["mp"]);
        e.vp = n(&p["vp"]);
        e.inc = n(&p["inc"]);
        e.away = truthy(&p["away"]);
        e.out = truthy(&p["out"]);
        m.world.game.players[slot].visible =
            p["visible"].as_array().unwrap().iter().map(id).collect();
        m.world.game.players[slot].visible.sort_unstable();
    }
    for source in initial["units"].as_array().unwrap() {
        assert!(
            !truthy(&source["operative"]),
            "initial state must precede actual entry"
        );
        let u = simtest::load::unit(
            source,
            &m.rules,
            m.world.game.players.len(),
            m.world.game.w * m.world.game.h,
        )
        .unwrap();
        let mut c = UnitControl::new(&u, &m.rules).unwrap();
        c.aim = n(&source["aim"]);
        c.auto_cast = truthy(&source["auto"]);
        c.auto_retreat = truthy(&source["autoRetreat"]);
        c.hold_fire = truthy(&source["holdFire"]);
        c.attack_id = id(&source["attackId"]);
        c.react = n(&source["react"]);
        assert!(source["orders"].as_array().unwrap().is_empty());
        m.commands.insert(u.id, c);
        let w = m.firing.initialize(u.id);
        w.cooldown = n(&source["cooldown"]);
        w.retarget = n(&source["retarget"]);
        w.target_id = id(&source["targetId"]);
        let combat = m.combat.initialize(u.id);
        combat.xp = n(&source["xp"]);
        combat.hit_at = n(&source["hitAt"]);
        combat.hit_by = (id(&source["hitBy"]) != 0).then(|| id(&source["hitBy"]));
        combat.last_hit = source["lastHit"].as_f64().map(|n| n as i32);
        combat.carrier = (id(&source["riding"]) != 0).then(|| id(&source["riding"]));
        combat.cargo = source["cargo"].as_array().unwrap().iter().map(id).collect();
        let a = m.abilities.initialize(u.id);
        a.cooldown = n(&source["cd"]);
        a.buff = n(&source["buff"]);
        a.ap = truthy(&source["ap"]);
        m.recovery.initialize(u.id).reinforcement = n(&source["reinf"]);
        m.world.game.units.insert(u.id, u);
    }
    assert!(initial["operatives"].as_array().unwrap().is_empty());
    m.grid = SpatialGrid::new(&m.world.game.units, 16.0);
    m
}

pub fn patch_unit(m: &mut Match, unit: u32, values: &Value) {
    for (k, v) in values.as_object().unwrap() {
        match k.as_str() {
            "x" => m.world.game.units.get_mut(&unit).unwrap().x = n(v),
            "z" => m.world.game.units.get_mut(&unit).unwrap().z = n(v),
            "hp" => m.world.game.units.get_mut(&unit).unwrap().hp = n(v),
            "supp" => m.world.game.units.get_mut(&unit).unwrap().supp = n(v),
            "repath" => m.world.game.units.get_mut(&unit).unwrap().repath = n(v),
            "sprint" => m.world.game.units.get_mut(&unit).unwrap().sprint = n(v),
            "cooldown" => m.firing.get_mut(unit).unwrap().cooldown = n(v),
            "retarget" => m.firing.get_mut(unit).unwrap().retarget = n(v),
            "cd" => m.abilities.get_mut(unit).unwrap().cooldown = n(v),
            "buff" => m.abilities.get_mut(unit).unwrap().buff = n(v),
            "reloadT" => m.companions.soldiers.get_mut(&unit).unwrap().reload = n(v),
            "nadeT" => m.companions.soldiers.get_mut(&unit).unwrap().nade_time = n(v),
            "nades" => m.companions.soldiers.get_mut(&unit).unwrap().nades = id(v),
            "mags" => m.companions.soldiers.get_mut(&unit).unwrap().mags = [id(&v[0]), id(&v[1])],
            "react" => m.commands.units.get_mut(&unit).unwrap().react = n(v),
            other => panic!("unrepresented trusted patch: {other}"),
        }
    }
    // Same publication seam as original placeUnit, with no whole-grid rebuild.
    m.grid.update(unit, &m.world.game.units[&unit]);
}
pub fn patch_player(m: &mut Match, owner: usize, values: &Value) {
    for (k, v) in values.as_object().unwrap() {
        match k.as_str() {
            "mp" => m.start.economy.players[owner].mp = n(v),
            "away" => m.start.economy.players[owner].away = truthy(v),
            "out" => m.start.economy.players[owner].out = truthy(v),
            other => panic!("unrepresented player patch {other}"),
        }
    }
}
pub fn patch_game(m: &mut Match, values: &Value) {
    for (k, v) in values.as_object().unwrap() {
        assert_eq!(k, "winner");
        m.finish = if v.is_null() {
            None
        } else {
            Some(ConquestFinish {
                winner: n(v) as i32,
                reason: ConquestEndReason::VictoryPoints,
                end_at: Point {
                    x: m.world.game.w as f64,
                    z: m.world.game.h as f64,
                },
                end_tick: m.world.game.tick,
            })
        };
    }
}

fn rule_json(v: &RuleValue) -> Value {
    match v {
        RuleValue::Null => Value::Null,
        RuleValue::Bool(b) => json!(b),
        RuleValue::Number(n) => json!(n),
        RuleValue::String(s) => json!(s),
        RuleValue::Array(a) => Value::Array(a.iter().map(rule_json).collect()),
        RuleValue::Object(o) => {
            Value::Object(o.iter().map(|(k, v)| (k.clone(), rule_json(v))).collect())
        }
    }
}
fn ability_profile(m: &Match, flight: &LiveFlight) -> Value {
    let mut profile = rule_json(
        Catalog::compiled()
            .unwrap()
            .unit("rifle")
            .unwrap()
            .get("ab")
            .unwrap(),
    );
    let blast = flight.blast_source.expect("linked grenade blast");
    profile["inf"] = json!(blast.infantry);
    profile["veh"] = json!(blast.vehicle);
    profile["terrain"] = json!(blast.terrain);
    profile["supp"] = json!(blast.suppression);
    profile["radius"] = json!(flight.blast_radius);
    assert!(blast.weapon_kind.is_none() && !blast.off_map_support);
    assert_eq!(blast.scar.to_bits(), 0f64.to_bits());
    assert!(blast.anti_garrison.is_none());
    assert!(m.rules.kinds.get(flight.source_kind).is_some());
    profile
}
fn unit_row(m: &Match, unit: u32) -> Value {
    let u = &m.world.game.units[&unit];
    let c = &m.commands.units[&unit];
    let w = m.firing.get(unit).unwrap();
    let combat = m.combat.get(unit).unwrap();
    let a = m.abilities.get(unit).unwrap();
    let r = m.recovery.get(unit).unwrap();
    assert!(
        c.orders.is_empty(),
        "these source scenes have no queued ordinary orders"
    );
    assert!(
        !u.dig && !u.build && !u.entrench,
        "inactive work descriptors only"
    );
    let follow = if c.follow == 0 {
        json!(0)
    } else {
        json!(m.companions.records[&c.follow].key)
    };
    let mut row = json!({"id":u.id,"type":m.rules.kinds[u.kind].name,"owner":u.owner,
        "x":u.x,"z":u.z,"rot":u.rot,"aim":c.aim,"moveSpeed":u.move_speed,"travelDir":u.travel_dir,
        "vx":u.vx,"vz":u.vz,"hp":u.hp,"supp":u.supp,"path":u.path.iter().copied().map(point_json).collect::<Vec<_>>(),
        "orders":[],"attackId":c.attack_id,"targetId":w.target_id,"cooldown":w.cooldown,"still":u.still,
        "retarget":w.retarget,"repath":u.repath,"react":c.react,"stuck":u.stuck,"cd":a.cooldown,"buff":a.buff,"ap":a.ap,
        "nade":c.nade.map(point_json),"retreating":u.retreating,"reinf":r.reinforcement,"dig":null,
        "garrison":u.garrison.map_or(-1,|n|n as i64),"enter":c.enter.map_or(-1,|n|n as i64),"board":u.board,
        "amove":u.amove.map(point_json),"face":c.face.map(|f|json!({"angle":f.angle,"x":f.at.x,"z":f.at.z})),
        "xp":combat.xp,"fireAt":c.fire_at.map_or(-1,|n|n as i64),"sprint":u.sprint,
        "riding":combat.carrier.unwrap_or(0),"cargo":combat.cargo,"hitBy":combat.hit_by.unwrap_or(0),
        "hitAt":combat.hit_at,"hitFrom":u.hit_from.map(point_json),"lastHit":combat.last_hit,
        "deathImpulse":combat.death_impulse.map(|d|json!({"dir":d.direction,"impulse":d.magnitude})),
        "drift":u.drift,"reverse":u.reverse.map(point_json),"auto":c.auto_cast,"autoRetreat":c.auto_retreat,
        "holdFire":c.hold_fire,"holdPos":u.hold_pos,"follow":follow,"shotAt":u.shot_at,
        "motionBefore":u.motion_before.map(pose_json),"traffic":u.traffic.as_ref().map(|t|json!({"goal":point_json(t.goal),"legs":t.legs.iter().copied().map(point_json).collect::<Vec<_>>(),"passing":t.passing,"by":t.by,"until":t.until})),
        "trafficWait":u.traffic_wait,"queuedBehind":u.queued_behind,"worldGoal":u.world_goal.map(point_json),
        "hpWas":r.hp_was,"woundAt":r.wound_at,"healAt":r.heal_at,"built":u.built,"cells":u.cells});
    assert_eq!(u.riding, combat.carrier.is_some());
    if let Some(body) = m.companions.soldiers.get(&unit) {
        let op = &m.companions.records[&body.companion];
        assert!(c.operative);
        assert_eq!(op.unit_id, unit);
        for (k,v) in json!({"operative":op.key,"vy":body.vy,"jy":body.jy,"crouch":body.crouch,
            "weapon":body.weapon,"mags":body.mags,"reloadT":body.reload,"nades":body.nades,"nadeT":body.nade_time}).as_object().unwrap() {
            row.as_object_mut().unwrap().insert(k.clone(),v.clone());
        }
    } else {
        assert!(!c.operative);
    }
    row
}
fn input_json(i: &ww2_core::companions::Input) -> Value {
    json!({"forward":i.forward,"strafe":i.strafe,"yaw":i.yaw,"pitch":i.pitch,"fire":i.fire,
        "ads":i.ads,"crouch":i.crouch,"sprint":i.sprint,"weapon":i.weapon})
}
fn companion_rows(m: &Match) -> Value {
    Value::Array(m.companions.records.values().map(|op|{
        let seen:serde_json::Map<_,_>=EDGE_NAMES.iter().enumerate().filter_map(|(i,k)|op.seen[i].map(|n|(k.to_string(),json!(n)))).collect();
        let want:serde_json::Map<_,_>=EDGE_NAMES.iter().enumerate().filter(|(i,_)|op.want[*i]).map(|(_,k)|(k.to_string(),json!(true))).collect();
        json!({"key":op.key,"owner":op.owner,"unitId":op.unit_id,"squadId":op.squad_id,
            "respawnAt":op.respawn_at,"online":op.online,"input":op.input.as_ref().map(input_json),
            "lastInput":op.last_input,"lastSeq":op.last_seq,"shots":op.shots,"hits":op.hits,"kills":op.kills,
            "seen":seen,"want":want})
    }).collect())
}
fn cell_row(row: &CellRow) -> Value {
    let d = &row.data;
    let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),
        "material":d.material,"groundMaterial":d.ground_material,"groundWear":f64::from(d.ground_wear)});
    if let Some(s) = &d.section {
        data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,
        "hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
    }
    assert!(d.trench_front.is_none() && !d.mine && d.rail.is_none() && d.station.is_none());
    json!([
        row.cell,
        char::from(row.ch).to_string(),
        row.height,
        row.state,
        data
    ])
}
fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut rows: Vec<Value> = Vec::new();
    for v in values {
        if rows.last().is_some_and(|r| r[0] == v) {
            let r = rows.last_mut().unwrap();
            r[1] = json!(r[1].as_u64().unwrap() + 1);
        } else {
            rows.push(json!([v, 1]));
        }
    }
    json!(rows)
}
fn terrain(m: &Match) -> Value {
    let g = &m.world.game;
    json!({"flags":runs(g.flags.iter().map(|n|json!(n))),
        "height":g.height.as_ref().map(|v|runs(v.iter().map(|n|json!(n)))),
        "chars":runs(g.chars.iter().map(|n|json!(char::from(*n).to_string()))),
        "ground":runs(g.ground.as_ref().unwrap().iter().map(|n|json!(char::from(*n).to_string()))),
        "objects":runs(g.objects.as_ref().unwrap().iter().map(|n|json!(char::from(*n).to_string()))),
        "cellHp":runs(g.cell_hp.iter().map(|n|json!(f64::from(*n)))),
        "wear":runs(g.wear.as_ref().unwrap().iter().map(|n|json!(f64::from(*n)))),
        "scar":runs(m.world.scar.iter().map(|n|json!(f64::from(*n)))),
        "burnt":runs(m.world.burnt.iter().map(|n|json!(n))),"cellState":runs(m.world.cell_state.iter().map(|n|json!(n))),
        "versions":[g.terrain_version,m.world.versions.obstruction,m.world.versions.infantry,m.world.versions.vehicle]})
}
fn flight_row(m: &Match, p: &LiveFlight) -> Value {
    assert!(p.intended.is_none() && !p.armor_piercing && !p.ap_naval);
    assert_eq!(p.damage_scale.to_bits(), 1f64.to_bits());
    assert_eq!(p.suppression.to_bits(), 0f64.to_bits());
    let mut row = json!({"id":p.id,"owner":p.owner,"shooter":p.shooter,"sourceId":p.shooter,
        "sourceMatchesShooter":m.world.game.units.contains_key(&p.shooter),"kind":m.rules.kinds[p.source_kind].name,
        "weapon":rule_json(Catalog::compiled().unwrap().unit(&m.rules.kinds[p.source_kind].name).unwrap().get("w").unwrap()),
        "x":p.solution.origin.x,"y":p.solution.origin.y,"z":p.solution.origin.z,
        "vx":p.solution.velocity.x,"vy":p.solution.velocity.y,"vz":p.solution.velocity.z,
        "gravity":p.solution.gravity,"stamp":p.solution.stamp,"launched":p.launched,"last":p.last,"expires":p.expires,
        "energy":p.energy,"sequence":p.sequence,"maxContacts":p.max_contacts,"near":p.near,
        "launchOrigin":{"x":p.launch_origin.x,"y":p.launch_origin.y,"z":p.launch_origin.z},
        "startCell":p.start_cell,"explosive":p.explosive,"blastRadius":p.blast_radius,
        "effect":p.effect,"sent":p.sent,"suppressed":m.firing.suppressed.get(&p.volley_group).unwrap()});
    row["blastSource"] = ability_profile(m, p);
    row
}

#[derive(Default)]
pub struct Projection {
    shots: Vec<Value>,
}
impl Projection {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn record_events(&mut self, m: &Match, events: Vec<TickEvent>) {
        for event in events {
            let shot = match event {
                TickEvent::Firing(event) => event.to_json(&m.rules),
                TickEvent::Environment(ww2_core::environment::Effect::LastHit { .. }) => {
                    panic!("operative fixture cannot represent LastHit as a shot")
                }
                TickEvent::Environment(event) => event.to_json(),
                TickEvent::Scenario(event) => event.shot_json(),
                TickEvent::AuthoredScenario(event) => event.shot_json(),
                TickEvent::Horde(event) => event.shot_json(),

                TickEvent::AuthoredMission(row) => row.clone(),
                TickEvent::Support(_) | TickEvent::Aviation(_) => {
                    panic!("operative fixture cannot represent Support or Aviation shot events")
                }
            };
            self.shots.push(shot);
        }
    }
    pub fn compare(&mut self, m: &mut Match, expected: &Value, context: &str) {
        assert!(
            m.firing.events.is_empty()
                && m.support.events.is_empty()
                && m.aviation.events.is_empty(),
            "{context}: report must detach all emitted producer events"
        );
        exact(
            &json!(self.shots),
            &expected["shots"],
            &path(context, "shots"),
        );
        let source = &expected["game"];
        let g = &m.world.game;
        let game = json!({"w":g.w,"h":g.h,"tick":g.tick,"nextId":m.next_unit_id,"nextFlight":m.firing.next_flight,
            "winner":m.finish.map(|f|f.winner),"mode":if g.mode.is_none(){Value::Null}else{panic!("internal non-Conquest mode")},
            "seed":f64::from(m.start.environment_seed as i32),"wind":{"a":m.start.wind.0,"v":m.start.wind.1},
            "visionTick":g.vision_tick,"weather":{"now":m.start.weather.now.name(),"next":m.start.weather.next.map(|n|n.name()),"at":m.start.weather.at},
            "wx":null,"winVp":m.start.win_vp,"tech":m.options.tech,"supply":m.options.supply,"logisticsEnabled":m.options.logistics});
        assert!(m.start.living.is_none() && !m.options.living_weather);
        exact(&game, source, &path(context, "game"));
        exact(
            &json!(g.units.keys().copied().collect::<Vec<_>>()),
            &expected["unitOrder"],
            &path(context, "unitOrder"),
        );
        assert_eq!(g.units.len(), expected["units"].as_array().unwrap().len());
        for row in expected["units"].as_array().unwrap() {
            let excluded: &[&str] = if truthy(&row["operative"]) {
                &[]
            } else {
                &["react"]
            };
            compare_fields(
                &unit_row(m, id(&row["id"])),
                row,
                excluded,
                &path(context, format!("unit:{}", row["id"])),
            );
        }
        exact(
            &companion_rows(m),
            &expected["operatives"],
            &path(context, "operatives"),
        );
        for p in expected["players"].as_array().unwrap() {
            let slot = id(&p["slot"]) as usize;
            let e = &m.start.economy.players[slot];
            let s = &m.start.players[slot];
            let actual = json!({"slot":slot,"name":s.config.name,"team":g.players[slot].team,"faction":s.config.faction as u8,
                "vp":e.vp,"mp":e.mp,"inc":e.inc,"sup":s.support,"spawn":point_json(s.spawn),"visible":g.players[slot].visible,
                "away":e.away,"out":e.out});
            assert_eq!(e.away, truthy(&p["away"]));
            assert_eq!(e.out, truthy(&p["out"]));
            compare_fields(&actual, p, &[], &path(context, format!("player:{slot}")));
        }
        assert_eq!(
            m.start.points.len(),
            expected["points"].as_array().unwrap().len()
        );
        for (i, source) in expected["points"].as_array().unwrap().iter().enumerate() {
            let p = &m.start.points[i];
            let actual = json!({"scenarioId":p.id,"x":p.at.x,"z":p.at.z,"vp":p.vp,"mp":p.mp,
                "owner":p.owner.map_or(-1,|o|o as i64),"capper":p.capper.map_or(-1,|o|o as i64),"progress":p.progress,
                "cut":p.cut,"needs":p.needs.map_or(-1,|n|n as i64),"kind":p.kind,"onPoint":p.on_point,
                "contested":p.contested,"capTick":m.capture_ticks[i]});
            compare_fields(&actual, source, &[], &path(context, format!("point:{i}")));
        }
        let story=json!(m.story.iter().map(|s|json!({"kills":s.kills,"losses":s.losses,"built":s.built,
            "captures":s.captures,"mpSpent":s.mp_spent,"supportCalls":s.support_calls,"planesDowned":s.planes_downed})).collect::<Vec<_>>());
        if expected["story"].is_null() {
            for row in story.as_array().unwrap() {
                for n in row.as_object().unwrap().values() {
                    assert_eq!(
                        n.as_f64().unwrap().to_bits(),
                        0f64.to_bits(),
                        "{context}: absent story must be zero"
                    );
                }
            }
        } else {
            exact(&story, &expected["story"], &path(context, "story"));
        }
        let timeline = json!(
            m.timeline
                .iter()
                .map(|s| match s {
                    MatchSample::Conquest { t, vp } => json!({"t":t,"vp":vp}),
                    MatchSample::Territories { .. } =>
                        panic!("companion-tick fixture cannot represent a Territories timeline"),
                })
                .collect::<Vec<_>>()
        );
        let empty_timeline = json!([]);
        let expected_timeline = if expected["timeline"].is_null() {
            &empty_timeline
        } else {
            &expected["timeline"]
        };
        exact(&timeline, expected_timeline, &path(context, "timeline"));
        let mut fallen = serde_json::Map::new();
        for (k, p) in [
            ("structure", m.fallen.structure),
            ("bunker", m.fallen.bunker),
            ("hq", m.fallen.hq),
        ] {
            if let Some(p) = p {
                fallen.insert(k.into(), point_json(p));
            }
        }
        if expected["fallen"].is_null() {
            assert!(fallen.is_empty());
        } else {
            exact(
                &Value::Object(fallen),
                &expected["fallen"],
                &path(context, "fallen"),
            );
        }
        exact(&terrain(m), &expected["terrain"], &path(context, "terrain"));
        let environment = json!({"wrecks":[],"fires":m.world.fires.iter().map(|(c,t)|json!([c,t])).collect::<Vec<_>>(),
            "soak":m.world.soak(),"newCells":m.world.new_cells().iter().map(cell_row).collect::<Vec<_>>(),
            "cellLog":m.world.cell_log().iter().map(cell_row).collect::<Vec<_>>(),
            "claims":m.firing.target_claims().map(|((target,team),v)|json!([u64::from(target)*8+team as u64,v])).collect::<Vec<_>>(),
            "aid":m.world.aid.iter().map(|(c,o)|json!([c,o])).collect::<Vec<_>>(),
            "mines":m.world.mines.iter().map(|(c,o)|json!([c,o])).collect::<Vec<_>>()});
        assert!(g.wrecks.is_empty());
        exact(
            &environment,
            &expected["environment"],
            &path(context, "environment"),
        );
        let probes:Vec<_>=expected["grid"]["probes"].as_array().unwrap().iter().map(|p|{
            let at=(n(&p["x"]),n(&p["z"]));let r=n(&p["radius"]);
            json!({"x":at.0,"z":at.1,"radius":r,"inOrder":m.grid.in_order(&g.units,at,r,|_,_|true),
                "unordered":m.grid.unordered(&g.units,at,r,|_,_|true),"heavy":m.grid.heavy(&g.units,at,r,|_,_|true)})
        }).collect();
        let owned: Vec<_> = (0..g.players.len())
            .map(|o| m.grid.owned_by(&g.units, o as i32))
            .collect();
        exact(
            &json!({"probes":probes,"ownedBy":owned}),
            &expected["grid"],
            &path(context, "grid"),
        );
        exact(
            &json!(m.firing.flights.keys().copied().collect::<Vec<_>>()),
            &expected["flightOrder"],
            &path(context, "flightOrder"),
        );
        for p in expected["flights"].as_array().unwrap() {
            compare_fields(
                &flight_row(m, &m.firing.flights[&(n(&p["id"]) as u64)]),
                p,
                &["duration"],
                &path(context, format!("flight:{}", p["id"])),
            );
        }
        let tickets=json!(m.firing.grenades.iter().map(|n|json!({"x":n.at.x,"z":n.at.z,"t":n.remaining,
            "owner":n.owner,"flight":n.flight,"ab":ability_profile(m,&m.firing.flights[&n.flight])})).collect::<Vec<_>>());
        exact(&tickets, &expected["nades"], &path(context, "nades"));
        assert!(
            m.firing.salvos.is_empty() && m.world.game.strikes.is_empty() && g.smokes.is_empty()
        );
        for k in ["salvos", "strikes", "smokes"] {
            exact(&json!([]), &expected[k], &path(context, k));
        }
        let snapshot = m.start.random.snapshot();
        let state = snapshot
            .streams
            .iter()
            .find_map(|(s, v)| (*s == StreamId::Combat).then_some(*v));
        let count = n(&expected["combatRandom"]["count"]) as usize;
        assert_eq!(snapshot.seed, n(&expected["combatRandom"]["seed"]) as u64);
        if count == 0 {
            assert!(state.is_none(), "{context}: unexpected Combat draws");
        } else {
            assert_eq!(
                format!("{:016x}", state.expect("recorded Combat stream")),
                expected["combatRandom"]["state"].as_str().unwrap(),
                "{context}: exact Combat cursor"
            );
            let mut predictor = MatchRandom::new(snapshot.seed);
            for _ in 0..count {
                predictor.next_fraction(StreamId::Combat);
            }
            assert_eq!(
                state,
                predictor
                    .snapshot()
                    .streams
                    .iter()
                    .find_map(|(s, v)| (*s == StreamId::Combat).then_some(*v)),
                "{context}: exactly recorded draw count"
            );
        }
    }
}
