//! Native intent checks for the owned ground-order rows. No Source process or capture is used.
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::PlaneState,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickReport,
    recipient_snapshot::SnapshotCache,
    weather::WeatherChoice,
};
use serde_json::{Value, json};

struct Scene(Match);
impl Scene {
    fn new() -> Self {
        let authored = json!({
            "name": "Order intent open lane",
            "w": 64,
            "h": 40,
            "rows": vec![".".repeat(64); 40],
            "heights": vec!["0".repeat(64); 40],
            "spawns": [{"x": 8, "y": 10}, {"x": 40, "y": 32}],
            "points": [{"x": 4, "y": 36, "vp": 0, "mp": 0}]
        });
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&authored).unwrap()).unwrap();
        let mut s = Self(
            Match::new(
                &map,
                MatchOptions {
                    seed: 701,
                    mode: Mode::Conquest,
                    players: vec![
                        PlayerConfig {
                            name: "A".into(),
                            team: 0,
                            faction: Faction::Usa,
                        },
                        PlayerConfig {
                            name: "B".into(),
                            team: 1,
                            faction: Faction::Germany,
                        },
                    ],
                    shuffle: false,
                    army: ArmySize::Endless,
                    weather: WeatherChoice::Map,
                    weather_seed: Some(1),
                    living_weather: false,
                    map_key: None,
                    tech: false,
                    supply: true,
                    logistics: false,
                    sandbox: false,
                },
            )
            .unwrap(),
        );
        for owner in 0..2 {
            let ids =
                s.0.world
                    .game
                    .units
                    .values()
                    .filter(|u| u.owner == owner && !s.0.rules.kinds[u.kind].structure)
                    .map(|u| u.id)
                    .collect::<Vec<_>>();
            assert_eq!(
                ids,
                if owner == 0 {
                    vec![1, 2, 3]
                } else {
                    vec![4, 5, 6]
                }
            );
            for (key, on) in [
                ("holdFire", true),
                ("holdPos", true),
                ("autoRetreat", false),
            ] {
                s.command(owner, json!({"t":"stance","ids":ids,"key":key,"on":on}));
            }
            s.command(owner, json!({"t":"autocast","ids":ids,"on":false}));
        }
        s
    }
    fn command(&mut self, owner: usize, raw: Value) {
        let cmd = Command::from_json(&raw).expect("valid authored order");
        assert_eq!(
            self.0.execute_command(owner, &cmd, false),
            Ok(None),
            "{raw}"
        );
    }
    fn denied(&mut self, owner: usize, raw: Value, denial: Denial) {
        let cmd = Command::from_json(&raw).unwrap();
        assert_eq!(
            self.0.execute_command(owner, &cmd, false),
            Ok(Some(denial)),
            "{raw}"
        );
    }
    fn step(&mut self) -> TickReport {
        self.0
            .advance_tick()
            .expect("ordinary native tick must run")
    }
    fn until(&mut self, limit: usize, label: &str, reached: impl Fn(&Match) -> bool) {
        for _ in 0..limit {
            if reached(&self.0) {
                return;
            }
            self.step();
        }
        panic!("native scene did not reach {label} within {limit} ticks");
    }
    fn move_to(&mut self, owner: usize, id: u32, x: f64, z: f64) {
        self.command(owner, json!({"t":"move","orders":[[id,x,z]]}));
        self.until(1200, "ordered arrival", |m| {
            let u = &m.world.game.units[&id];
            u.path.is_empty() && crate::math::hypot(u.x - x, u.z - z) < 1.0
        });
    }
}

fn flags(m: &Match, slot: usize, id: u32) -> u64 {
    let projected = SnapshotCache::new(m).unwrap().recipient(slot).unwrap();
    projected.fields["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u[0] == id)
        .expect("actor visible in recipient snapshot")[12]
        .as_u64()
        .unwrap()
}

#[test]
fn cmb83_stance_validation_owner_flags_and_explicit_attack() {
    let mut s = Scene::new();
    for raw in [
        json!({"t":"stance","ids":[1],"key":"toString","on":true}),
        json!({"t":"stance","ids":[1],"key":"holdFire","on":1}),
    ] {
        assert_eq!(Command::from_json(&raw), Err(Denial::Blocked));
    }
    s.denied(
        0,
        json!({"t":"stance","ids":[4],"key":"holdFire","on":true}),
        Denial::Needs,
    );
    s.move_to(0, 1, 45., 29.);
    s.move_to(1, 4, 57., 29.);
    s.step();
    let foreign = (
        s.0.world.game.units[&4].clone(),
        s.0.commands.units[&4].clone(),
    );
    for (key, bit) in [("holdFire", 2048), ("holdPos", 4096), ("autoRetreat", 8192)] {
        for on in [false, true] {
            s.command(0, json!({"t":"stance","ids":[1,4],"key":key,"on":on}));
            assert_eq!(flags(&s.0, 0, 1) & bit, if on { bit } else { 0 });
            assert_eq!(flags(&s.0, 1, 1) & bit, 0);
        }
    }
    assert_eq!(
        (&s.0.world.game.units[&4], &s.0.commands.units[&4]),
        (&foreign.0, &foreign.1)
    );
    s.command(0, json!({"t":"autocast","ids":[1],"on":true}));
    assert_ne!(flags(&s.0, 0, 1) & 16384, 0);
    assert_eq!(flags(&s.0, 1, 1) & 16384, 0);
    s.command(0, json!({"t":"autocast","ids":[1],"on":false}));
    let hp = s.0.world.game.units[&4].hp;
    for _ in 0..80 {
        s.step();
    }
    assert_eq!(
        s.0.world.game.units[&4].hp, hp,
        "hold fire suppresses automatic shots"
    );
    s.command(0, json!({"t":"attack","ids":[1],"target":4}));
    s.until(200, "explicit attack under hold fire", |m| {
        m.world.game.units[&4].hp < hp
    });
    s.command(
        0,
        json!({"t":"stance","ids":[1],"key":"holdFire","on":false}),
    );
    assert!(!s.0.commands.units[&1].hold_fire);
}

#[test]
fn cmb91_mov49_attack_move_halts_kills_resumes_and_finishes() {
    let mut s = Scene::new();
    s.move_to(0, 1, 45., 29.);
    s.move_to(1, 4, 63., 29.);
    s.command(
        0,
        json!({"t":"stance","ids":[1],"key":"holdFire","on":false}),
    );
    s.command(0, json!({"t":"amove","orders":[[1,85.,29.]]}));
    let mut halted = false;
    let mut resumed = false;
    let mut dead = false;
    for _ in 0..1200 {
        s.step();
        let u = &s.0.world.game.units[&1];
        let target = s.0.firing.get(1).unwrap().target_id;
        if target == 4
            && s.0.world.game.units.contains_key(&4)
            && s.0.can_shoot_target(1, 4).unwrap()
        {
            halted |= u.path.is_empty() && u.amove.is_some();
        }
        if !s.0.world.game.units.contains_key(&4) {
            dead = true;
            resumed |= !u.path.is_empty() && u.x > 45.;
            if u.amove.is_none() {
                assert!(crate::math::hypot(u.x - 85., u.z - 29.) < 2.5);
                break;
            }
        }
    }
    assert!(
        halted && dead && resumed,
        "must halt for a shootable target, kill it, and resume"
    );
    assert!(s.0.world.game.units[&1].amove.is_none());
    let mut plain = Scene::new();
    plain.move_to(0, 1, 45., 29.);
    plain.move_to(1, 4, 63., 29.);
    plain.command(
        0,
        json!({"t":"stance","ids":[1],"key":"holdFire","on":false}),
    );
    plain.move_to(0, 1, 85., 29.);
    assert!(
        plain.0.world.game.units[&1].x > 80.,
        "plain Move continues past the enemy"
    );
}

#[test]
fn cmb140_stop_clears_live_orders_and_paid_build_work() {
    let mut s = Scene::new();
    s.move_to(0, 1, 45., 29.);
    s.move_to(0, 2, 43., 33.);
    s.move_to(1, 4, 57., 29.);
    s.step();
    s.command(0, json!({"t":"amove","orders":[[1,85.,29.]]}));
    s.command(0, json!({"t":"move","orders":[[1,81.,33.]],"queue":true}));
    s.command(0, json!({"t":"attack","ids":[2],"target":4}));
    assert!(s.0.world.game.units[&1].amove.is_some());
    assert!(!s.0.commands.units[&1].orders.is_empty());
    assert_eq!(s.0.commands.units[&2].attack_id, 4);
    let foreign = s.0.world.game.units[&4].clone();
    s.denied(0, json!({"t":"stop","ids":[]}), Denial::Needs);
    s.denied(0, json!({"t":"stop","ids":[4]}), Denial::Needs);
    s.command(0, json!({"t":"stop","ids":[1,2,4]}));
    assert_eq!(s.0.world.game.units[&4], foreign);
    for id in [1, 2] {
        let u = &s.0.world.game.units[&id];
        let c = &s.0.commands.units[&id];
        assert!(u.path.is_empty() && u.amove.is_none() && u.world_goal.is_none());
        assert!(c.orders.is_empty() && c.attack_id == 0 && c.build_target.is_none());
    }
    let site = s.0.next_unit_id;
    s.command(
        0,
        json!({"t":"build","ids":[1,2],"kind":"motorpool","x":29,"z":9}),
    );
    assert!(s.0.world.game.units[&1].build && s.0.world.game.units[&2].build);
    let mp = s.0.start.economy.players[0].mp;
    let built = s.0.world.game.units[&site].built;
    s.command(0, json!({"t":"stop","ids":[1,2]}));
    assert_eq!(s.0.start.economy.players[0].mp, mp);
    for id in [1, 2] {
        assert!(!s.0.world.game.units[&id].build);
        assert_eq!(s.0.commands.units[&id].build_target, None);
    }
    for _ in 0..20 {
        s.step();
        assert_eq!(s.0.world.game.units[&site].built, built);
    }
    let mut project = Scene::new();
    project.command(0, json!({"t":"entrench","ids":[1,2],"pattern":"line","fort":"trench","x":29,"z":9,"x2":41,"z2":9}));
    project.until(2400, "paid project work before Stop", |m| {
        [1, 2]
            .iter()
            .all(|id| m.field_works.jobs().contains_key(id))
    });
    let mp = project.0.start.economy.players[0].mp;
    project.command(0, json!({"t":"stop","ids":[1,2]}));
    assert_eq!(project.0.start.economy.players[0].mp, mp);
    for id in [1, 2] {
        assert!(!project.0.world.game.units[&id].dig && !project.0.world.game.units[&id].entrench);
        assert!(!project.0.field_works.is_active(id));
        assert_eq!(project.0.field_works.membership(id), None);
    }
    project.step();
}

#[test]
fn cmb140_aircraft_stop_patrols_here() {
    let mut s = Scene::new();
    let plane = s.0.spawn_aircraft(0, "attacker").unwrap();
    s.command(
        0,
        json!({"t":"stance","ids":[plane],"key":"holdFire","on":true}),
    );
    s.command(0, json!({"t":"move","orders":[[plane,45.,29.]]}));
    assert_eq!(s.0.world.game.units[&plane].air, Some(PlaneState::Out));
    let at = s.0.world.game.units[&plane].at();
    s.command(0, json!({"t":"stop","ids":[plane]}));
    let mission = s.0.aviation.planes[&plane].mission.unwrap();
    assert_eq!(mission.kind, crate::match_aviation::MissionKind::Patrol);
    assert_eq!(mission.at, at);
}
