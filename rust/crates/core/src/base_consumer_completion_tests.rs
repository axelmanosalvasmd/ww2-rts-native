//! Selected original components. No fixture value initializes native gameplay.
use super::*;
use crate::{
    environment::{Context as EnvironmentContext, DamageContext},
    recipient_snapshot,
};

struct Preserved {
    state: AtomicState,
    clock: Option<(u64, usize, bool)>,
    tick: i64,
    next: u32,
    random: crate::random::MatchRandom,
    environment_seed: u32,
}
impl Preserved {
    fn capture(m: &Match) -> Self {
        Self {
            state: AtomicState::capture(m),
            clock: m
                .classic_runtime
                .as_ref()
                .map(|s| (s.time_left.to_bits(), s.initial_teams, s.sudden_death)),
            tick: m.world.game.tick,
            next: m.next_unit_id,
            random: m.start.random.clone(),
            environment_seed: m.start.environment_seed,
        }
    }
    fn unchanged(self, m: &Match) {
        assert_eq!(
            m.classic_runtime.as_ref().map(|s| (
                s.time_left.to_bits(),
                s.initial_teams,
                s.sudden_death
            )),
            self.clock
        );
        assert_eq!(m.world.game.tick, self.tick);
        assert_eq!(m.next_unit_id, self.next);
        assert_eq!(m.start.random, self.random);
        assert_eq!(m.start.environment_seed, self.environment_seed);
        self.state.unchanged(m);
    }
}
fn construct(input: &Json) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
    let mode = Mode::from_kind(input["settings"]["mode"].as_str());
    let options = MatchOptions {
        seed: 701,
        mode,
        players: input["names"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, name)| PlayerConfig {
                name: name.as_str().unwrap().into(),
                team: input["teams"][i].as_i64().unwrap() as i32,
                faction: if input["factions"][i] == 0 {
                    Faction::Usa
                } else {
                    Faction::Germany
                },
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(217),
        living_weather: false,
        map_key: None,
        tech: true,
        supply: false,
        logistics: false,
        sandbox: false,
    };
    if mode == Mode::Classic {
        Match::new_classic(&map, options)
    } else {
        Match::new(&map, options)
    }
    .unwrap()
}
fn frames() -> Json {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/base-consumer-completion.json");
    let f: Json = serde_json::from_str(
        &std::fs::read_to_string(path)
            .expect("the seventh Source recipe needs its own exact-head generation/check grant"),
    )
    .unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["cases"].as_array().unwrap().len(), 5);
    f
}
fn equal(a: &Json, b: &Json) {
    match (a, b) {
        (Json::Number(a), Json::Number(b)) => {
            assert_eq!(a.as_f64().unwrap().to_bits(), b.as_f64().unwrap().to_bits())
        }
        (Json::Array(a), Json::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                equal(a, b);
            }
        }
        (Json::Object(a), Json::Object(b)) => {
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (key, a) in a {
                equal(a, &b[key]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
fn project(m: &Match) -> Json {
    json!({"tick":m.world.game.tick,"next":m.next_unit_id,
        "wallets":m.start.economy.players.iter().map(|p| json!([bits(p.mp),bits(p.mun),bits(p.fuel)])).collect::<Vec<_>>(),
        "spent":m.story.iter().map(|s| bits(s.mp_spent)).collect::<Vec<_>>(),
        "personal":m.player_rallies.iter().map(|p| point(*p)).collect::<Vec<_>>(),
        "actors":m.world.game.units.values().map(|u| json!([
            u.id,u.owner,m.rules.kinds[u.kind].name,bits(u.hp),u.built.map(bits)])).collect::<Vec<_>>(),
        "producers":m.world.game.units.values().filter_map(|u| m.production.building(u.id).map(|b| json!([
            u.id,bits(b.progress()),point(m.building_rallies[&u.id]),b.queue().collect::<Vec<_>>(),
            b.rows().into_iter().map(|j| json!([j.job.id,j.job.kind,bits(j.job.charge.mp),bits(j.job.charge.fuel),
                if j.status==JobStatus::Active {"active"} else {"waiting"}])).collect::<Vec<_>>()]))).collect::<Vec<_>>(),
        "sections":m.world.game.units.values().filter_map(|u| u.cells.as_ref().map(|cells| json!([
            u.id,cells.iter().map(|c| { let s=&m.world.game.sections[c];
                json!([c,bits(s.hp),bits(s.max_hp.unwrap()),s.failed,char::from(m.world.game.chars[*c as usize]).to_string()])
            }).collect::<Vec<_>>()]))).collect::<Vec<_>>(),
        "recipients":(0..m.world.game.players.len()).map(|slot| {
            let rows=recipient_snapshot::production_for(m,slot).unwrap();
            for name in ["queues","productionJobs"] {
                for row in rows[name].as_array().unwrap() {
                    assert_eq!(m.world.game.units[&(row[0].as_u64().unwrap() as u32)].owner,slot);
                }
            }
            json!({"cached":rows,"uncached":rows})
        }).collect::<Vec<_>>()})
}
fn step(m: &mut Match, count: u64, construction: bool) {
    for _ in 0..count {
        if construction {
            m.advance_classic_depot_tick().unwrap();
        } else {
            m.advance_classic_base_tick().unwrap();
        }
    }
}
fn contact(m: &mut Match, cell: usize, amount: f64, name: &str) -> f64 {
    // Same existing section component used by DamageWorld, without admitting
    // the unsupported Classic combat runner or changing actor-mode guards.
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let skip = BTreeSet::new();
    let mut hit = DamageContext::plain(&skip);
    hit.owner = Some(1);
    hit.contact_id = Some(name);
    let rain = m.world.game.rain;
    let wet = m.world.game.wet.unwrap_or(0.);
    let mut context = EnvironmentContext {
        tick: m.world.game.tick as u64,
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        grid: &m.grid,
        rain,
        wet,
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    };
    let (taken, effects) = m
        .environment
        .damage_section(&mut m.world, cell, amount, &mut context, &hit)
        .unwrap();
    for effect in effects {
        crate::match_combat::absorb_environment_effect(&mut m.combat, effect, &mut |_| {});
    }
    taken
}
fn replay(name: &str) -> Match {
    let f = frames();
    let case = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap();
    let mut m = construct(&case["input"]);
    equal(&project(&m), &case["initial"]);
    let construction = name == "classic-sequential-sites-queued-repair-and-paid-mg";
    let mut phases = BTreeSet::new();
    let mut paid_head = None;
    for (index, row) in case["records"].as_array().unwrap().iter().enumerate() {
        equal(&project(&m), &row["before"]);
        match row["op"].as_str().unwrap() {
            "command" => {
                let slot = row["slot"].as_u64().unwrap() as usize;
                let command = Command::from_json(&row["cmd"]).unwrap();
                let before = Preserved::capture(&m);
                let tick = m.world.game.tick;
                let next = m.next_unit_id;
                let wallet = m.start.economy.players[slot].mp;
                let result = m.execute_command(slot, &command, false).unwrap();
                assert_eq!(
                    json!(result.map(|d| d.name())),
                    row["result"],
                    "{name}: command {index}"
                );
                assert_eq!(m.world.game.tick, tick);
                if result.is_some() {
                    before.unchanged(&m);
                } else {
                    match command {
                        Command::Buy { ref kind, from } => {
                            if m.options.mode == Mode::Classic {
                                let jobs = m.production.building(from.unwrap()).unwrap().jobs();
                                assert_eq!(m.next_unit_id, next);
                                assert!(m.start.economy.players[slot].mp < wallet);
                                if kind == "mg" {
                                    paid_head = Some(jobs[0].id);
                                }
                            } else {
                                let u = &m.world.game.units[&next];
                                assert_eq!(u.owner, slot);
                                assert_eq!(m.rules.kinds[u.kind].name, "rifle");
                                assert_eq!(
                                    m.commands.units[&next].route_end,
                                    m.building_rallies[&from.unwrap()]
                                );
                                assert_eq!(m.next_unit_id, next + 1);
                                assert!(
                                    m.production
                                        .building(from.unwrap())
                                        .unwrap()
                                        .jobs()
                                        .is_empty()
                                );
                            }
                        }
                        Command::Move { ref orders, .. } if construction => {
                            assert_eq!(
                                m.commands.units[&(orders[0].unit as u32)].build_target,
                                None
                            );
                        }
                        Command::Assist {
                            ref ids,
                            target,
                            queue: true,
                            ..
                        } => {
                            let c = &m.commands.units[&(ids[0] as u32)];
                            assert_eq!(c.build_target, None);
                            assert!(
                                matches!(c.orders.front(),Some(Command::Assist { target:t,.. }) if *t==target)
                            );
                        }
                        _ => {}
                    }
                }
            }
            "advance" => {
                let count = row["count"].as_u64().unwrap();
                step(&mut m, count, construction);
                let phase = row["phase"].as_str().unwrap();
                assert!(phases.insert(phase.to_owned()));
                match phase {
                    "both-sites-complete" => {
                        let bars = m
                            .world
                            .game
                            .units
                            .values()
                            .filter(|u| m.rules.kinds[u.kind].name == "barracks")
                            .collect::<Vec<_>>();
                        assert_eq!(bars.len(), 2);
                        assert!(bars.iter().all(|u| u.built == Some(1.)));
                        assert!(m.commands.units[&own(&m, 0, "engineer")].orders.is_empty());
                    }
                    "supported-section-restored" => {
                        let id = own(&m, 0, "barracks");
                        let u = &m.world.game.units[&id];
                        let c = u.cells.as_ref().unwrap()[0];
                        assert!(!m.world.game.sections[&c].failed);
                        assert!(m.world.game.sections[&c].hp > 0.);
                        assert_eq!(m.world.game.chars[c as usize], b'K');
                        assert_eq!(
                            m.production.building(id).unwrap().jobs()[0].id,
                            paid_head.unwrap()
                        );
                    }
                    "queued-repair-and-real-mg-complete" => {
                        let id = own(&m, 0, "barracks");
                        let full = Catalog::compiled()
                            .unwrap()
                            .unit("barracks")
                            .unwrap()
                            .get("hpPer")
                            .and_then(Value::number)
                            .unwrap();
                        assert_eq!(m.world.game.units[&id].hp, full);
                        let engineer = own(&m, 0, "engineer");
                        assert_eq!(m.commands.units[&engineer].build_target, None);
                        assert!(m.commands.units[&engineer].orders.is_empty());
                        let mg = own(&m, 0, "mg");
                        assert_eq!(m.commands.units[&mg].route_end, m.building_rallies[&id]);
                        assert!(m.production.building(id).unwrap().jobs().is_empty());
                    }
                    "before-deadline" => assert!(!m.classic_sudden_death()),
                    "genuine-deadline" => {
                        assert!(m.classic_sudden_death());
                        assert!(
                            m.production
                                .buildings()
                                .all(|(_, b)| b.jobs().is_empty() && b.progress() == 0.)
                        );
                    }
                    "decay-ten-seconds" => {
                        let hq = own(&m, 0, "hq");
                        let hp = row["before"]["actors"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|u| u[0] == hq)
                            .unwrap()[3]
                            .as_str()
                            .unwrap();
                        let hp = f64::from_bits(u64::from_str_radix(hp, 16).unwrap());
                        assert!((hp - m.world.game.units[&hq].hp - 300.).abs() < 5.);
                    }
                    "earned"
                    | "head-progress"
                    | "head-born-waiter-promoted"
                    | "engineer-walks-to-repair-reach" => {}
                    _ => panic!("unexpected phase"),
                }
            }
            "breach" => {
                let id = row["building"].as_u64().unwrap() as u32;
                let jobs = m.production.building(id).unwrap().clone();
                let cell = row["cell"].as_u64().unwrap() as usize;
                assert_eq!(
                    m.world.game.units[&id].cells.as_ref().unwrap()[0] as usize,
                    cell
                );
                let taken = contact(
                    &mut m,
                    cell,
                    row["damage"].as_f64().unwrap(),
                    row["contact"].as_str().unwrap(),
                );
                assert_eq!(bits(taken), row["result"].as_str().unwrap());
                assert_eq!(m.production.building(id), Some(&jobs));
            }
            _ => panic!("unexpected component action"),
        }
        equal(&project(&m), &row["after"]);
    }
    m
}
#[test]
fn base22_23_ordinary_cancel_mode_gate_rally_ownership_and_actual_rifle() {
    for mode in ["default", "conquest"] {
        let m = replay(&format!("{mode}-cancel-rally-and-real-recruit"));
        assert!(m.classic_runtime.is_none());
    }
}
#[test]
fn base20_21_22_23_classic_paid_waiter_owner_rows_refund_and_promotion() {
    let m = replay("classic-owned-waiter-identity-and-promotion");
    assert_eq!(
        m.production
            .building(own(&m, 0, "hq"))
            .unwrap()
            .jobs()
            .len(),
        1
    );
    assert!(m.player_rallies.iter().all(Option::is_none));
}
#[test]
fn base29_30_classic_sequential_paid_sites_supported_breach_queued_assist_and_mg() {
    let m = replay("classic-sequential-sites-queued-repair-and-paid-mg");
    assert_eq!(m.story[0].built, 2.);
    assert_eq!(m.story[0].losses, 0.);
}
#[test]
fn base56_genuine_deadline_clears_without_refund_and_precedes_invalid_cancel_lookup() {
    let m = replay("classic-real-deadline-cancel-denial-precedence");
    assert!(m.classic_sudden_death());
    assert!(m.world.game.tick >= 30_000);
}
#[test]
fn base22_23_classic_paid_waiter_and_rally_preflight_errors_preserve_canonical_state() {
    let mut i = input(true);
    i["map"]["name"] = json!("BASE consumer completion");
    let mut m = construct(&i);
    step(&mut m, 3000, false);
    let hq = own(&m, 0, "hq");
    let ally = own(&m, 1, "hq");
    for _ in 0..3 {
        let buy = Command::from_json(&json!({"t":"buy","unit":"rifle","from":hq})).unwrap();
        assert_eq!(m.execute_command(0, &buy, false).unwrap(), None);
    }
    step(&mut m, 40, false);
    let job = m.production.building(hq).unwrap().jobs()[2].id;
    // Only bookkeeping is removed. The job, charge and progress were earned.
    let story = std::mem::take(&mut m.story);
    let before = Preserved::capture(&m);
    let cancel = Command::from_json(&json!({"t":"cancelProduction","id":hq,"job":job})).unwrap();
    let error = m.execute_command(0, &cancel, false).unwrap_err();
    assert!(error.contains("story"));
    before.unchanged(&m);
    m.story = story;
    let before = m.production.building(hq).unwrap().clone();
    let charge = before.jobs()[2].charge;
    let mp = m.start.economy.players[0].mp;
    assert_eq!(m.execute_command(0, &cancel, false).unwrap(), None);
    assert_eq!(m.start.economy.players[0].mp, mp + charge.mp);
    assert_eq!(
        m.production.building(hq).unwrap().progress(),
        before.progress()
    );
    assert_eq!(
        m.production.building(hq).unwrap().jobs(),
        &before.jobs()[..2]
    );
    // A later selected real owned maker lacks bookkeeping. The first maker
    // must keep its prior rally when validating that later maker fails.
    let buy = Command::from_json(&json!({"t":"buy","unit":"rifle","from":ally})).unwrap();
    assert_eq!(m.execute_command(1, &buy, false).unwrap(), None);
    let rally = Command::from_json(&json!({"t":"rally","id":hq,"x":41,"z":41})).unwrap();
    assert_eq!(m.execute_command(0, &rally, false).unwrap(), None);
    let engineer = own(&m, 0, "engineer");
    let at = m.world.game.units[&hq].at();
    let site = m.next_unit_id;
    let build = Command::from_json(
        &json!({"t":"build","kind":"barracks","ids":[engineer],"x":at.x+20.,"z":at.z}),
    )
    .unwrap();
    assert_eq!(m.execute_command(0, &build, false).unwrap(), None);
    m.building_rallies.remove(&site);
    let before = Preserved::capture(&m);
    let rally = Command::from_json(&json!({"t":"rally","ids":[hq,site],"x":45,"z":45})).unwrap();
    let error = m.execute_command(0, &rally, false).unwrap_err();
    assert!(error.contains("rally producer metadata"));
    before.unchanged(&m);
}
