//! Full intended consumers for protected, currently unavailable actuators.
//! Ignored until a separate exact-head grant. None is a passing row witness.
use super::Replay;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use ww2_core::{
    catalog::Catalog,
    combat::{Impact, WeaponDamage},
    economy::{Faction, Mode, Technology, price_of, tech_cost},
    economy_state::ArmySize,
    game::{PlaneState, Point},
    map::AuthoritativeMap,
    match_aviation::{MissionKind, PlaneMission},
    match_commands::Command,
    match_combat::{DamageWorld, ExplosionSource, FlightSource},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_support::{SupportEvent, SupportKind},
    match_tick::{TickEvent, TickReport},
    recipient_snapshot::SnapshotCache,
    weather::WeatherChoice,
};

struct Scene {
    m: Match,
    original: BTreeSet<u32>,
    replay: Option<Replay>,
}
impl Scene {
    fn new(seed: u64, logistics: bool, houses: &[(usize, usize)]) -> Self {
        Self::with_input(seed, logistics, houses, None)
    }
    fn with_input(seed: u64, logistics: bool, houses: &[(usize, usize)], replay: Option<Replay>) -> Self {
        let arena: Value = serde_json::from_str(include_str!(
            "../../simtest/tests/fixtures/base-pending-paratroopers.json"
        )).unwrap();
        let mut map = arena["map"].clone();
        for &(x, z) in houses {
            let mut row = map["rows"][z].as_str().unwrap().as_bytes().to_vec();
            row[x] = b'B';
            map["rows"][z] = json!(String::from_utf8(row).unwrap());
        }
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap();
        let options = MatchOptions {
            seed, mode: Mode::Conquest,
            players: vec![
                PlayerConfig { name: "A".into(), team: 0, faction: Faction::Usa },
                PlayerConfig { name: "B".into(), team: 1, faction: Faction::Germany },
            ],
            shuffle: false, army: ArmySize::Standard, weather: WeatherChoice::Map,
            weather_seed: Some(1), living_weather: false, map_key: None,
            tech: true, supply: false, logistics, sandbox: false,
        };
        let mut m = if let Some(replay) = &replay {
            Match::new_aligned(&map, options, replay.clone()).unwrap()
        } else { Match::new(&map, options).unwrap() };
        let original = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
        assert_eq!(original.len(), 10, "retain both complete constructor armies");
        // The original selected checks declare funding. No actor, stock, timer,
        // visibility or random state is installed from an expected observation.
        for p in &mut m.start.economy.players { p.mp = 10000.0; }
        let mut scene = Self { m, original, replay };
        for owner in 0..2 {
            let ids = scene.ids(owner);
            for (key, on) in [("holdFire", true), ("holdPos", true), ("autoRetreat", false)] {
                scene.command(owner, json!({"t":"stance","ids":ids,"key":key,"on":on}));
            }
            scene.command(owner, json!({"t":"autocast","ids":ids,"on":false}));
        }
        scene
    }
    fn ids(&self, owner: usize) -> Vec<u32> {
        self.m.world.game.units.values().filter(|u| u.owner == owner).map(|u| u.id).collect()
    }
    fn command(&mut self, owner: usize, input: Value) {
        let command = Command::from_json(&input).unwrap();
        assert!(!matches!(command, Command::Unsupported), "missing command parser: {input}");
        assert_eq!(self.m.execute_command(owner, &command, false).unwrap(), None,
            "actual command admission: {input}");
    }
    fn tick(&mut self) -> TickReport {
        let before = self.m.world.game.tick;
        // Intentionally use the real runner. In particular, enabled logistics
        // must fail here today. No private scheduler or guard bypass is used.
        let report = self.m.advance_tick().unwrap();
        assert_eq!(report.tick, before + 1);
        assert!(report.commands.iter().all(|c| c.denial.is_none()));
        assert!(self.original.iter().all(|id| self.m.world.game.units.contains_key(id)),
            "all constructor actors remain throughout the authored history");
        assert!(self.m.finish.is_none());
        report
    }
    fn ticks(&mut self, count: usize) {
        for _ in 0..count { self.tick(); }
    }
    fn until(&mut self, limit: usize, predicate: impl Fn(&Match) -> bool) {
        for _ in 0..limit {
            if predicate(&self.m) { return; }
            self.tick();
        }
        assert!(predicate(&self.m), "complete consumer exceeded its declared bound");
    }
    fn tier(&mut self, owner: usize, tier: u8) {
        while self.m.production.player(owner).unwrap().tech().tier() < tier {
            let before = self.m.production.player(owner).unwrap().tech().tier();
            let spent = self.m.story[owner].mp_spent;
            let cost = tech_cost(self.m.production.player(owner).unwrap().tech(),
                Technology::Tier, self.m.options.mode).unwrap().unwrap().mp;
            self.command(owner, json!({"t":"tech","kind":"tier"}));
            assert_eq!(self.m.story[owner].mp_spent - spent, cost);
            self.until(2400, |m| m.production.player(owner).unwrap().tech().tier() > before);
            assert!(self.m.production.player(owner).unwrap().jobs().is_empty());
        }
    }
    fn facility(&mut self, owner: usize, kind: &str) -> u32 {
        let builder = self.m.world.game.units.values().find(|u|
            u.owner == owner && self.m.rules.kinds[u.kind].name == "rifle").unwrap().id;
        let spawn = self.m.start.players[owner].spawn;
        let at = Point { x: spawn.x, z: spawn.z + 16.0 };
        assert!(self.m.recipient_sees(owner, at).unwrap());
        let before = self.m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
        let spent = self.m.story[owner].mp_spent;
        self.command(owner, json!({"t":"stance","ids":[builder],"key":"holdPos","on":false}));
        self.command(owner, json!({"t":"build","ids":[builder],"kind":kind,"x":at.x,"z":at.z}));
        let born = self.m.world.game.units.keys().filter(|id| !before.contains(*id)).copied().collect::<Vec<_>>();
        assert_eq!(born.len(), 1);
        let id = born[0];
        assert_eq!(self.m.rules.kinds[self.m.world.game.units[&id].kind].name, kind);
        assert!(self.m.world.game.units[&id].built.unwrap() < 1.0);
        assert_eq!(self.m.story[owner].mp_spent - spent, price_of(self.m.options.mode, kind).unwrap().mp);
        self.until(2400, |m| m.world.game.units[&id].built == Some(1.0));
        self.tick();
        assert!(self.m.production.building(id).is_some());
        id
    }
    fn buy(&mut self, owner: usize, kind: &str, from: Option<u32>) -> u32 {
        let before = self.m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
        let wallet = self.m.start.economy.players[owner].mp;
        let spent = self.m.story[owner].mp_spent;
        let mut input = json!({"t":"buy","unit":kind});
        if let Some(from) = from { input["from"] = json!(from); }
        self.command(owner, input);
        let born = self.m.world.game.units.keys().filter(|id| !before.contains(*id)).copied().collect::<Vec<_>>();
        assert_eq!(born.len(), 1, "one actual paid birth");
        let id = born[0];
        let u = &self.m.world.game.units[&id];
        assert_eq!((u.owner, self.m.rules.kinds[u.kind].name.as_str()), (owner, kind));
        let producer = self.m.recruit_metadata[&id].building;
        assert!(before.contains(&producer));
        assert_eq!(self.m.world.game.units[&producer].built, Some(1.0));
        if let Some(from) = from { assert_eq!(producer, from); }
        let cost = price_of(self.m.options.mode, kind).unwrap().mp;
        assert_eq!(self.m.start.economy.players[owner].mp, wallet - cost);
        assert_eq!(self.m.story[owner].mp_spent, spent + cost);
        for (key, on) in [("holdFire", true), ("holdPos", true), ("autoRetreat", false)] {
            self.command(owner, json!({"t":"stance","ids":[id],"key":key,"on":on}));
        }
        self.command(owner, json!({"t":"autocast","ids":[id],"on":false}));
        id
    }
    fn stage(&mut self, owner: usize, goals: &[(u32, f64, f64)]) {
        let orders = goals.iter().map(|(id,x,z)| json!([id,x,z])).collect::<Vec<_>>();
        self.command(owner, json!({"t":"move","orders":orders}));
        self.until(2400, |m| goals.iter().all(|(id,x,z)| {
            let u = &m.world.game.units[id];
            u.path.is_empty() && ww2_core::math::hypot(u.x-x, u.z-z) <= 0.25
                && m.commands.units[id].outcome == Some(ww2_core::match_commands::MoveOutcome::Arrived)
        }));
        self.ticks(40);
    }
    fn owner_row(&self, owner: usize, id: u32) -> Value {
        SnapshotCache::new(&self.m).unwrap().recipient(owner).unwrap().fields["units"]
            .as_array().unwrap().iter().find(|r| r[0].as_u64() == Some(u64::from(id))).unwrap().clone()
    }
}

fn support_number(kind: &str, field: &str) -> f64 {
    Catalog::compiled().unwrap().support(kind).unwrap().get(field).unwrap().number().unwrap()
}

#[test]
#[ignore = "held CMB66: ordinary enabled-logistics tick admission is unavailable"]
fn cmb66_b09_funded_last_round_hit_miss_and_empty_after_real_reload_history() {
    // The original fixed controls start before construction. Use the unchanged
    // default adapter, never the ordinary CMB42 packet/observation mode.
    for (empty, fraction) in [(true, 0.0_f64), (false, 0.0), (false, 0.99)] {
        let bits = format!("{:016x}", fraction.to_bits());
        let replay = Replay::new(&json!({"layout":[0,1],"tape":vec![bits;16384]})).unwrap();
        let mut s = Scene::with_input(701, true, &[], Some(replay));
        s.tier(1, 2);
        let pool = s.facility(1, "motorpool");
        let target = s.buy(1, "tank", Some(pool));
        s.stage(1, &[(target, 81.0, 65.0)]);
        let gun = s.buy(0, "flak", None);
        let initial = s.m.logistics.get(gun).unwrap().clone();
        assert!(initial.ammo > 1.0 && initial.ammo == initial.ammo_max);
        s.stage(0, &[(gun, 61.0, 65.0)]);
        assert!(s.m.world.game.players[0].sees(target));
        assert!(s.m.can_shoot_target(gun, target).unwrap());
        let tank_hp = s.m.world.game.units[&target].hp;
        s.command(0, json!({"t":"attack","ids":[gun],"target":target}));
        let stop_at = if empty { 0.0 } else { 1.0 };
        let mut previous = initial.ammo;
        let mut debits = 0;
        let mut last_debit = None;
        for _ in 0..3000 {
            if s.m.logistics.get(gun).unwrap().ammo == stop_at { break; }
            s.tick();
            let ammo = s.m.logistics.get(gun).unwrap().ammo;
            if ammo != previous {
                assert_eq!(ammo, previous - 1.0, "only actual one-round launches deplete stock");
                if let Some(tick) = last_debit {
                    assert!((s.m.world.game.tick - tick) as f64 * s.m.rules.movement.tick >= 0.3 - 1e-9,
                        "actual flak reload interval, no cooldown reset");
                }
                last_debit = Some(s.m.world.game.tick);
                debits += 1;
            }
            assert_eq!(s.m.world.game.units[&target].hp, tank_hp, "flak has zero vehicle damage");
            previous = ammo;
        }
        assert_eq!(s.m.logistics.get(gun).unwrap().ammo, stop_at);
        assert_eq!(debits as f64, initial.ammo - stop_at);
        s.command(0, json!({"t":"stop","ids":[gun]}));
        assert_eq!(s.m.commands.units[&gun].attack_id, 0);
        assert!(s.m.commands.units[&gun].hold_fire);
        s.stage(1, &[(target, 100.0, 65.0)]);
        assert!(!s.m.can_shoot_target(gun, target).unwrap());
        assert_eq!(s.m.logistics.get(gun).unwrap().ammo, stop_at);
        s.command(0, json!({"t":"stance","ids":[gun],"key":"holdFire","on":false}));
        // Offset the dive impact, retaining the gun inside its 45 m AA radius.
        // A missed interception must not turn the reserve check into a death check.
        let at = Point { x: 61.0, z: 95.0 };
        let wallet = s.m.start.economy.players[1].mp;
        let replay = s.replay.as_ref().unwrap().clone();
        let chance_line = include_str!("match_support.rs").lines().position(|line|
            line.contains("if self.start.random.next_fraction(StreamId::OffMap) < chance")
        ).unwrap() + 1;
        let chance_draws = |receipt: &Value| receipt["draws"].as_array().unwrap().iter().filter(|r|
            r["consumer"][0] == "UNMAPPED.fraction" && r["consumer"][1] == "OffMap"
                && r["consumer"][2].as_str().unwrap().ends_with("match_support.rs")
                && r["consumer"][3].as_u64() == Some(chance_line as u64)
        ).cloned().collect::<Vec<_>>();
        assert!(chance_draws(&replay.receipt()).is_empty());
        let before_story = s.m.story[0].planes_downed;
        s.command(1, json!({"t":"support","kind":"dive","x":at.x,"z":at.z,"dir":0}));
        assert_eq!(s.m.start.economy.players[1].mp, wallet - support_number("dive", "cost"));
        let mut flak = 0;
        let mut down = 0;
        for _ in 0..((support_number("dive", "delay") / 0.05) as usize + 2) {
            let report = s.tick();
            for event in report.events {
                match event {
                    TickEvent::Support(SupportEvent::Flak { unit, .. }) if unit == gun => flak += 1,
                    TickEvent::Support(SupportEvent::ShotDown { kind: SupportKind::Dive, fighters: false, .. }) => down += 1,
                    _ => {}
                }
            }
        }
        assert_eq!(flak, if empty { 0 } else { 1 }, "literal B09 funded attempt count");
        assert_eq!(down, if !empty && fraction == 0.0 { 1 } else { 0 }, "literal B09 hit/miss/empty control");
        assert_eq!(s.m.logistics.get(gun).unwrap().ammo, 0.0, "all B09 final stocks are empty");
        assert_eq!(s.m.story[0].planes_downed - before_story, down as f64);
        let chance = chance_draws(&replay.receipt());
        assert_eq!(chance.len(), if empty {0} else {1},
            "a funded attempt consumes one actual chance draw, empty consumes none");
        if !empty { assert_eq!(chance[0]["bits"], format!("{:016x}",fraction.to_bits())); }

    }
}

#[test]
#[ignore = "held CMB62: FireAt parser and protected terrain launcher are unavailable"]
fn cmb62_tank_shells_single_house_same_house_first_wall_and_rifle_refusal() {
    for deep in [false, true] {
        let houses = if deep {
            (9..=11).flat_map(|z| (8..=10).map(move |x| (x,z))).collect::<Vec<_>>()
        } else { vec![(9,10)] };
        let mut s = Scene::new(701, false, &houses);
        s.tier(0, 2);
        let pool = s.facility(0, "motorpool");
        let tank = s.buy(0, "tank", Some(pool));
        s.stage(0, &[(tank,20.0,if deep {38.0} else {40.0})]);
        let cell = 10 * s.m.world.game.w + 9;
        let wall = 11 * s.m.world.game.w + 9;
        assert!(s.m.world.game.cell_hp[cell] > 0.0);
        s.command(0, json!({"t":"fireat","ids":[tank],"x":19,"z":21}));
        assert_eq!(s.m.commands.units[&tank].fire_at, Some(cell));
        let mut wall_failure = None;
        let mut center_failure = None;
        for _ in 0..if deep {800} else {400} {
            s.tick();
            if deep && s.m.world.game.cell_hp[wall] <= 0.0 && wall_failure.is_none() {
                wall_failure = Some(s.m.world.game.tick);
            }
            if s.m.world.game.cell_hp[cell] <= 0.0 && center_failure.is_none() {
                center_failure = Some(s.m.world.game.tick);
            }
        }
        if deep {
            assert!(wall_failure.unwrap() < center_failure.unwrap(),
                "actual first-wall destruction precedes center destruction");
        }
        assert!(s.m.world.game.cell_hp[cell] <= 0.0);
        assert_eq!(s.m.commands.units[&tank].fire_at, None, "original order clears when destroyed");
        if !deep {
            s.ticks(1200);
            let meta = &s.m.world.sections[&cell];
            assert!(s.m.world.game.sections[&(cell as u32)].failed);
            assert!(meta.failure.settled_at.is_some_and(f64::is_finite));
            assert!(!meta.rubble.is_empty(), "original durable rubble footprint");
            for &rubble in &meta.rubble {
                assert_eq!(s.m.world.game.objects.as_ref().unwrap()[rubble], b'R');
                let dx = (rubble % s.m.world.game.w) as f64 - (cell % s.m.world.game.w) as f64;
                let dz = (rubble / s.m.world.game.w) as f64 - (cell / s.m.world.game.w) as f64;
                assert!(ww2_core::math::hypot(dx,dz) <= 3.0, "original rubble stays within 6 m");
            }
            let rifle = s.buy(0, "rifle", None);
            s.command(0, json!({"t":"fireat","ids":[rifle],"x":19,"z":21}));
            assert_eq!(s.m.commands.units[&rifle].fire_at, None, "original rifle refusal");
        }
    }
}

#[test]
#[ignore = "held CMB67: explicit held-aircraft ground attack and ammo consumer unavailable"]
fn cmb67_original_hold_fire_patrol_then_explicit_ground_attack_uses_ammo() {
    let mut s = Scene::new(701, false, &[]);
    s.tier(0, 3);
    let field = s.facility(0, "airfield");
    let plane = s.buy(0, "attacker", Some(field));
    let target = s.buy(1, "rifle", None);
    // These are the original first-half test's declared pre-step component
    // pose/station/patrol inputs, not a way around mission admission.
    for (id,x,z) in [(plane,40.0,50.0),(target,50.0,50.0)] {
        let u = s.m.world.game.units.get_mut(&id).unwrap();
        u.x = x; u.z = z;
    }
    s.m.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Station);
    s.m.aviation.planes.get_mut(&plane).unwrap().mission = Some(PlaneMission {
        kind: MissionKind::Patrol, at: Point {x:50.0,z:50.0}, target: None,
    });
    s.m.firing.get_mut(&plane).unwrap().retarget = 99.0;
    s.m.grid.rebuild(&s.m.world.game.units);
    ww2_core::vision::update_vision(&mut s.m.world.game, &s.m.rules, &s.m.grid);
    assert!(s.m.world.game.players[0].sees(target));
    let ammo = s.m.aviation.planes[&plane].ammo;
    s.ticks(6);
    assert_eq!(s.m.aviation.planes[&plane].ammo, ammo, "original held patrol .3 seconds");
    s.command(0, json!({"t":"attack","ids":[plane],"target":target}));
    assert!(s.m.commands.units[&plane].hold_fire, "explicit attack preserves Hold Fire");
    s.ticks(20);
    assert!(s.m.aviation.planes[&plane].ammo < ammo,
        "entire original first-half control requires the actual weapon consumer");
}

#[test]
#[ignore = "held CMB68: ordinary dead-carrier ejection and 30 percent loss unavailable"]
fn cmb68_halftrack_capacity_riding_views_unload_reboard_and_actual_wreck_exit() {
    let mut s = Scene::new(701, false, &[]);
    s.tier(0, 2);
    let pool = s.facility(0, "motorpool");
    let carrier = s.buy(0, "halftrack", Some(pool));
    let rifle = s.buy(0, "rifle", None);
    let mg = s.buy(0, "mg", None);
    s.stage(0, &[(carrier,40.0,40.0),(rifle,43.0,40.0),(mg,43.0,44.0)]);
    let hp = s.m.world.game.units[&rifle].hp;
    assert_eq!(hp,100.0);
    s.command(0, json!({"t":"board","ids":[rifle,mg],"target":carrier}));
    s.until(200, |m| m.combat.get(rifle).unwrap().carrier == Some(carrier));
    assert_eq!(s.m.combat.get(carrier).unwrap().cargo, vec![rifle]);
    assert!(!s.m.world.game.units[&mg].riding);
    let full = Command::from_json(&json!({"t":"board","ids":[mg],"target":carrier})).unwrap();
    assert_eq!(s.m.execute_command(0,&full,false).unwrap(), Some(ww2_core::match_commands::Denial::Maximum));
    s.command(0, json!({"t":"move","orders":[[carrier,61,40],[rifle,0,0]]}));
    s.until(1000, |m| m.world.game.units[&carrier].x > 51.0);
    let parent = &s.m.world.game.units[&carrier];
    let rider = &s.m.world.game.units[&rifle];
    assert!(ww2_core::math::hypot(parent.x-rider.x,parent.z-rider.z)<0.01);
    assert_eq!(rider.hp,hp);
    assert!(s.owner_row(0,rifle)[12].as_u64().unwrap() & 262144 != 0);
    let foe = s.buy(1,"rifle",None);
    s.stage(1,&[(foe,65.0,40.0)]);
    assert!(s.m.world.game.players[1].sees(carrier));
    assert!(!s.m.world.game.players[1].sees(rifle));
    let view = SnapshotCache::new(&s.m).unwrap().recipient(1).unwrap();
    assert!(view.fields["units"].as_array().unwrap().iter().all(|r|r[0] != json!(rifle)));
    s.stage(1,&[(foe,100.0,100.0)]);
    s.command(0,json!({"t":"unload","ids":[carrier]}));
    assert!(!s.m.world.game.units[&rifle].riding);
    assert!(s.m.combat.get(carrier).unwrap().cargo.is_empty());
    assert_eq!(s.m.world.game.units[&rifle].hp,hp,"manual unload has no thrown damage");
    s.command(0,json!({"t":"board","ids":[rifle],"target":carrier}));
    s.until(200,|m|m.combat.get(rifle).unwrap().carrier==Some(carrier));
    // Exact original selected input at test.js:7804, after real reboarding.
    // The ordinary runner still owns ejection, placement, damage and retirement.
    s.m.world.game.units.get_mut(&carrier).unwrap().hp=0.0;
    s.ticks(3);
    assert!(!s.m.world.game.units[&rifle].riding,"actual wreck exit");
    assert_eq!(s.m.combat.get(rifle).unwrap().carrier,None);
    assert!((s.m.world.game.units[&rifle].hp-70.0).abs()<1e-6,"exact 30 percent full-squad loss");
}


#[test]
#[ignore = "authored CMB68 actual component, separate exact-head execution grant required"]
fn cmb68_tank_rider_enemy_hit_rider_blast_allied_controls_and_enclosed_blast() {
    // Source sim.js:2423-2439,4209,4727-4733. These component cases are
    // separate from the halftrack wreck and missing offshore throw actuator.
    // Path 0 is a tank contact, 1 is the rider's actual hurt consumer,
    // 2 is full radial membership. A rider-specific hit cannot be substituted
    // by a blast that reaches the tank first and happens to eject its riders.
    for (enclosed, path, owner) in [
        (false,0,1_usize), (false,0,0),
        (false,1,1), (false,1,0), (false,2,1), (true,2,1),
    ] {
        let mut s = Scene::new(701,false,&[]);
        s.tier(0,2);
        let pool = s.facility(0,"motorpool");
        let carrier = s.buy(0,if enclosed {"halftrack"} else {"tank"},Some(pool));
        let first = s.buy(0,"rifle",None);
        let second = if enclosed { None } else { Some(s.buy(0,"rifle",None)) };
        let mut goals = vec![(carrier,61.0,45.0),(first,64.0,45.0)];
        if let Some(second) = second { goals.push((second,64.0,49.0)); }
        s.stage(0,&goals);
        let riders = if let Some(second) = second {vec![first,second]} else {vec![first]};
        s.command(0,json!({"t":"board","ids":riders,"target":carrier}));
        s.until(400,|m|riders.iter().all(|id|m.combat.get(*id).unwrap().carrier==Some(carrier)));
        assert_eq!(s.m.combat.get(carrier).unwrap().cargo.len(),riders.len());
        let foe = s.buy(1,"rifle",None);
        s.stage(1,&[(foe,51.0,45.0)]);
        assert!(s.m.world.game.players[1].sees(carrier));
        assert!(!s.m.world.game.players[1].sees(first),"hidden carried squad");
        assert!(s.m.can_shoot_target(foe,carrier).unwrap());
        assert!(!s.m.can_shoot_target(foe,first).unwrap(),"visible carrier, hidden carried squad");
        let shooter = if owner==1 {foe} else {s.buy(0,"rifle",None)};
        let target = if path==0 {carrier} else {first};
        let before = riders.iter().map(|id|s.m.world.game.units[id].hp).collect::<Vec<_>>();
        let order = s.m.world.game.units.keys().copied().collect::<Vec<_>>();
        let at = s.m.world.game.units[&target].at();
        let from = s.m.world.game.units[&shooter].at();
        let kind = s.m.world.game.units[&shooter].kind;
        let impact = Impact { owner:owner as i32, shooter,
            weapon:WeaponDamage::from_catalog("rifle").unwrap(), damage_scale:None,
            energy:1.0, armor_piercing:false, ap_naval_only:false, ap_multiplier:1.5,
            launch_origin:from, at, direction:Point{x:1.0,z:0.0},
        };
        let mut damage = DamageWorld {
            world:&mut s.m.world, combat:&mut s.m.combat, grid:&mut s.m.grid, rules:&s.m.rules,
            logistics:&s.m.logistics, production:&s.m.production, mode:Mode::Conquest, tech_enabled:true,
            environment:&mut s.m.environment, environment_seed:&mut s.m.start.environment_seed,
            next_id:&mut s.m.next_unit_id, unit_order:&order, wind_angle:0.0, wind_strength:0.5,
        };
        if path==2 {
            // Actual blast membership decides deck versus enclosed exposure.
            damage.explosion(at,4.5,ExplosionSource::support("strafe").unwrap(),owner as i32,
                None,None,None,&mut s.m.firing).unwrap();
        } else if path==1 {
            damage.hurt(first,ExplosionSource::support("strafe").unwrap(),1.0,
                owner as i32,&mut s.m.firing).unwrap();
        } else {
            damage.direct_hit(FlightSource{shooter,kind,owner:owner as i32,airborne:false},
                carrier,impact,&mut s.m.firing).unwrap();
        }
        for (index,id) in riders.iter().enumerate() {
            let u = &s.m.world.game.units[id];
            let thrown = !enclosed && owner == 1;
            assert_eq!(u.riding,!thrown);
            assert_eq!(s.m.combat.get(*id).unwrap().carrier,if thrown {None} else {Some(carrier)});
            if thrown { assert!(u.supp>=60.0,"all riders receive original minimum shock"); }
            if !enclosed && (path==2 || path==1 && index==0) {
                assert!(u.hp<before[index],"the actual blast/hurt consumer damages exposed riders");
            } else { assert_eq!(u.hp,before[index],"no thrown damage on a nonwreck jump"); }
        }
        assert_eq!(s.m.combat.get(carrier).unwrap().cargo.len(),
            if !enclosed && owner==1 {0} else {riders.len()});
    }
}


#[test]
#[ignore = "held CMB67: automatic ground weapon and bomber area salvo unavailable"]
fn cmb67_automatic_patrol_weapon_and_bomber_area_salvos_use_actual_consumers() {
    for area in [false,true] {
        let mut s = Scene::new(701,false,&[]);
        s.tier(0,3);
        let field = s.facility(0,"airfield");
        let plane = s.buy(0,if area {"bomber"} else {"attacker"},Some(field));
        let target = s.buy(1,"rifle",None);
        // The same declared station/pose setup as the original first-half
        // Hold Fire component. All subsequent missions and shots are actual.
        for (id,x) in [(plane,40.0),(target,50.0)] {
            let u = s.m.world.game.units.get_mut(&id).unwrap(); u.x=x; u.z=50.0;
        }
        s.m.world.game.units.get_mut(&plane).unwrap().air=Some(PlaneState::Station);
        s.m.aviation.planes.get_mut(&plane).unwrap().mission=Some(PlaneMission {
            kind:MissionKind::Patrol,target:None,at:Point{x:50.0,z:50.0},
        });
        s.m.grid.rebuild(&s.m.world.game.units);
        ww2_core::vision::update_vision(&mut s.m.world.game,&s.m.rules,&s.m.grid);
        assert!(s.m.world.game.players[0].sees(target));
        assert!(ww2_core::math::hypot(
            s.m.world.game.units[&plane].x-s.m.world.game.units[&target].x,
            s.m.world.game.units[&plane].z-s.m.world.game.units[&target].z,
        )<20.0);
        // Ground can_shoot_target itself rejects airborne shooters. Reach the
        // actual aircraft mission/stance consumer without calling that helper.
        let ammo=s.m.aviation.planes[&plane].ammo;
        if area {
            // Preserve the native Area refusal until its owner implements it.
            // Do not write the desired Area mission directly into runtime state.
            assert!(s.m.send_aircraft_mission(0,plane,PlaneMission {
                kind:MissionKind::Area,target:None,at:Point{x:50.0,z:50.0},
            }).unwrap());
        } else {
            s.command(0,json!({"t":"stance","ids":[plane],"key":"holdFire","on":false}));
        }
        let mut launches=0;
        for _ in 0..20 {
            for event in s.tick().events {
                match event {
                    TickEvent::Firing(ww2_core::match_firing::LaunchEvent::Salvo{shooter,at,..})
                        if area && shooter==plane => {
                            assert_eq!(at,Point{x:50.0,z:50.0},"every bomber stick uses its ordered spot");
                            launches+=1;
                        }
                    _=>{}
                }
            }
        }
        assert!(s.m.aviation.planes[&plane].ammo<ammo);
        if area { assert!(launches>0,"actual bomber salvo event, not a manufactured ammo debit"); }
    }
}
