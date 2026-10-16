use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    game::{CELL, Point},
    map::AuthoritativeMap,
    match_end::EndReason,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    unit_placement::place_unit,
    weather::WeatherChoice,
};

fn scene(data: Value) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Ordinary authored mission", "w":96, "h":96,
            "rows":vec![".".repeat(96);96],
            "spawns":[{"x":4,"y":4},{"x":90,"y":90},{"x":4,"y":90}],
            "points":[{"id":"bridgehead","x":32,"y":32}],
            "scenario":data
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 83085,
            mode: Mode::Conquest,
            players: [0, 1, 0]
                .into_iter()
                .enumerate()
                .map(|(slot, team)| PlayerConfig {
                    name: format!("seat{slot}"),
                    team,
                    faction: Faction::Germany,
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
    .unwrap()
}

fn definition(groups: Value, triggers: Vec<Value>) -> Value {
    json!({"version":1,"areas":[{"id":"entry","box":[39,19,45,21]}],
        "groups":groups,"objectives":[{"id":"local","text":"Local"},{"id":"global","text":"Global"}],
        "triggers":triggers})
}

fn trigger(id: &str, condition: Value) -> Value {
    json!({"id":id,"scope":"match","condition":condition,
        "actions":[{"kind":"say","text":id}]})
}

fn count(m: &Match, id: &str, scope: &str) -> u32 {
    m.authored_scenario
        .as_ref()
        .unwrap()
        .executions
        .get(&format!("{id}@{scope}"))
        .map_or(0, |record| record.count)
}

fn ids(m: &Match, group: &str) -> Vec<u32> {
    m.authored_scenario.as_ref().unwrap().facts.groups[group].clone()
}

fn tick(m: &mut Match, delivery: &mut DeliveryState) -> (Vec<Value>, Vec<TickEvent>) {
    let report = m.advance_tick().unwrap();
    assert_eq!(m.tick_state.fault(), None);
    let selected = (0..m.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(m, &selected, &report.events)
        .unwrap()
        .unwrap();
    let snapshots = selected
        .iter()
        .map(|seat| {
            let snapshot = frame.recipient(seat.slot).unwrap().into_json();
            assert_eq!(snapshot["t"], "s");
            snapshot
        })
        .collect();
    (snapshots, report.events)
}

fn checkpoint(m: &mut Match, delivery: &mut DeliveryState) {
    let scenario = m.authored_scenario.clone();
    *m = Match::from_save_json(m.save_json().unwrap()).unwrap();
    assert_eq!(m.authored_scenario, scenario);
    *delivery = DeliveryState::new(m).unwrap();
}

fn finish(m: &mut Match, delivery: &mut DeliveryState) {
    // The ordinary VP owner decides the result while the authored state remains installed.
    m.start.points[0].owner = Some(0);
    m.start.points[0].vp = 20.0;
    m.start.win_vp = m
        .start
        .economy
        .players
        .iter()
        .filter(|p| p.team == 0)
        .map(|p| p.vp)
        .sum::<f64>()
        + 1.0;
    let mut ended = None;
    for _ in 0..200 {
        tick(m, delivery);
        if let Some(result) = m.finish {
            assert_eq!(result.reason, EndReason::VictoryPoints);
            assert_eq!(result.winner, 0);
            let at = *ended.get_or_insert(m.world.game.tick);
            if m.world.game.tick >= at + 8 {
                return;
            }
        }
    }
    panic!("ordinary mission did not finish and deliver eight ending ticks");
}

fn relocate(m: &mut Match, id: u32, at: [usize; 2]) {
    place_unit(
        &mut m.grid,
        m.world.game.units.get_mut(&id).unwrap(),
        |unit| {
            unit.x = (at[0] as f64 + 0.5) * CELL;
            unit.z = (at[1] as f64 + 0.5) * CELL;
            unit.path.clear();
            unit.world_goal = None;
            unit.amove = None;
        },
    );
}

#[test]
fn mode83_conditions_follow_capture_losses_groups_areas_and_scoped_dependencies_to_match_end() {
    let mut triggers = vec![
        trigger(
            "owned",
            json!({"kind":"ownership","target":"bridgehead","side":0}),
        ),
        trigger(
            "wrongOwner",
            json!({"kind":"ownership","target":"bridgehead","side":2}),
        ),
        trigger(
            "captured",
            json!({"kind":"capture","target":"bridgehead","side":0}),
        ),
        trigger("lostUnit", json!({"kind":"unitLost","unit":"first"})),
        trigger("lostGroup", json!({"kind":"groupLost","group":"patrol"})),
        trigger(
            "atLeast",
            json!({"kind":"groupCount","group":"patrol","op":"atLeast","count":2}),
        ),
        trigger(
            "atMost",
            json!({"kind":"groupCount","group":"patrol","op":"atMost","count":1}),
        ),
        trigger(
            "equal",
            json!({"kind":"groupCount","group":"patrol","op":"equal","count":1}),
        ),
        trigger(
            "sideEntry",
            json!({"kind":"areaEntry","area":"entry","side":0}),
        ),
        trigger(
            "wrongEntry",
            json!({"kind":"areaEntry","area":"entry","side":1}),
        ),
        trigger(
            "groupEntry",
            json!({"kind":"areaEntry","area":"entry","group":"patrol","side":1}),
        ),
        trigger(
            "outsideGroup",
            json!({"kind":"areaEntry","area":"entry","group":"capturer"}),
        ),
        trigger(
            "all",
            json!({"kind":"all","conditions":[{"kind":"capture","target":"bridgehead","side":0},{"kind":"unitLost","unit":"first"}]}),
        ),
        trigger(
            "any",
            json!({"kind":"any","conditions":[{"kind":"time","seconds":7200},{"kind":"unitLost","unit":"first"}]}),
        ),
        trigger(
            "falseAll",
            json!({"kind":"all","conditions":[{"kind":"unitLost","unit":"first"},{"kind":"ownership","target":"bridgehead","side":2}]}),
        ),
        trigger(
            "historical",
            json!({"kind":"all","conditions":[{"kind":"capture","target":"bridgehead","side":0},{"kind":"ownership","target":"bridgehead","side":1}]}),
        ),
        trigger("localComplete", json!({"kind":"time","seconds":0})),
        trigger("globalComplete", json!({"kind":"time","seconds":0.15})),
        trigger(
            "localObjective",
            json!({"kind":"objectiveComplete","objective":"local"}),
        ),
        trigger(
            "globalObjective",
            json!({"kind":"objectiveComplete","objective":"global"}),
        ),
        trigger(
            "localTrigger",
            json!({"kind":"triggerComplete","trigger":"localComplete"}),
        ),
        trigger(
            "globalTrigger",
            json!({"kind":"triggerComplete","trigger":"globalComplete"}),
        ),
        trigger(
            "teamOwner",
            json!({"kind":"ownership","target":"bridgehead"}),
        ),
        trigger(
            "teamCapture",
            json!({"kind":"capture","target":"bridgehead"}),
        ),
    ];
    for t in &mut triggers {
        let id = t["id"].as_str().unwrap().to_owned();
        if id.starts_with("local") || id.starts_with("global") && id != "globalComplete" {
            t["scope"] = json!("player");
        }
        if id == "localComplete" {
            t["side"] = json!(0);
            t["actions"] = json!([{"kind":"objectiveComplete","objective":"local"}]);
        }
        if id == "globalComplete" {
            t["actions"] = json!([{"kind":"objectiveComplete","objective":"global"}]);
        }
        if id.starts_with("team") {
            t["scope"] = json!("team");
        }
    }
    let mut m = scene(definition(
        json!([
            {"id":"patrol","units":[{"id":"first","type":"rifle","side":0,"x":40,"y":20},{"id":"second","type":"rifle","side":0,"x":44,"y":20}]},
            {"id":"capturer","units":[{"id":"flag","type":"rifle","side":0,"x":32,"y":32}]}
        ]),
        triggers,
    ));
    let mut delivery = DeliveryState::new(&m).unwrap();
    let (initial, _) = tick(&mut m, &mut delivery);
    for snapshot in initial {
        assert_eq!(
            snapshot["scenario"],
            json!({"version":1,"objectives":[{
                "id":"local","scope":"player:0","text":"Local","state":"complete",
                "recipients":"public","side":0,"updated":0.05,"execution":"scenario:4"
            }]})
        );
    }
    for id in ["atLeast", "sideEntry", "groupEntry"] {
        assert_eq!(count(&m, id, "match"), 1, "{id}");
    }
    for id in [
        "owned",
        "captured",
        "lostUnit",
        "lostGroup",
        "atMost",
        "equal",
        "all",
        "any",
        "wrongEntry",
        "outsideGroup",
    ] {
        assert_eq!(count(&m, id, "match"), 0, "{id}");
    }
    for slot in 0..3 {
        assert_eq!(
            count(&m, "localObjective", &format!("player:{slot}")),
            u32::from(slot == 0)
        );
        assert_eq!(
            count(&m, "localTrigger", &format!("player:{slot}")),
            u32::from(slot == 0)
        );
        assert_eq!(count(&m, "globalObjective", &format!("player:{slot}")), 0);
    }
    for _ in 0..200 {
        tick(&mut m, &mut delivery);
        if count(&m, "captured", "match") == 1 {
            break;
        }
    }
    assert_eq!(m.start.points[0].owner, Some(0));
    assert_eq!(count(&m, "owned", "match"), 1);
    assert_eq!(count(&m, "captured", "match"), 1);
    assert_eq!(count(&m, "teamOwner", "team:0"), 1);
    assert_eq!(count(&m, "teamOwner", "team:1"), 0);
    assert_eq!(count(&m, "teamCapture", "team:0"), 1);
    for slot in 0..3 {
        assert_eq!(count(&m, "globalObjective", &format!("player:{slot}")), 1);
        assert_eq!(count(&m, "globalTrigger", &format!("player:{slot}")), 1);
    }
    checkpoint(&mut m, &mut delivery);
    let first = m.authored_scenario.as_ref().unwrap().facts.authored_units["first"];
    m.world.game.units.get_mut(&first).unwrap().hp = 0.0;
    tick(&mut m, &mut delivery);
    for id in ["lostUnit", "atMost", "equal", "all", "any"] {
        assert_eq!(count(&m, id, "match"), 1, "{id}");
    }
    assert_eq!(count(&m, "lostGroup", "match"), 0);
    let second = ids(&m, "patrol")[0];
    m.world.game.units.get_mut(&second).unwrap().hp = 0.0;
    tick(&mut m, &mut delivery);
    assert_eq!(count(&m, "lostGroup", "match"), 1);
    let flag = ids(&m, "capturer")[0];
    relocate(&mut m, flag, [12, 32]);
    let enemy = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    relocate(&mut m, enemy, [32, 32]);
    for _ in 0..360 {
        tick(&mut m, &mut delivery);
        if count(&m, "historical", "match") == 1 {
            break;
        }
    }
    assert_eq!(m.start.points[0].owner, Some(1));
    assert_eq!(
        count(&m, "historical", "match"),
        1,
        "capture remains true after ownership changes"
    );
    assert_eq!(count(&m, "wrongOwner", "match"), 0);
    assert_eq!(count(&m, "falseAll", "match"), 0);
    assert_eq!(count(&m, "wrongEntry", "match"), 0);
    relocate(&mut m, enemy, [90, 90]);
    finish(&mut m, &mut delivery);
}

#[test]
fn mode84_actions_use_canonical_damage_and_publish_ordered_arrivals_through_ending_ticks() {
    let mut actions = trigger("effects", json!({"kind":"time","seconds":0.05}));
    actions["actions"] = json!([
        {"kind":"say","text":{"en":"Advance","es":"Avanzar"}},
        {"kind":"objectiveActivate","objective":"global"},
        {"kind":"damage","box":[40,40,40,40],"damage":25},
        {"kind":"reinforce","group":"walkers","side":0,"at":[20,20],"roster":[{"type":"rifle","count":1}],"expires":10,"order":{"kind":"move","at":[25,20]}},
        {"kind":"reinforce","group":"attackers","side":0,"at":[20,30],"roster":[{"type":"rifle","count":1}],"expires":10,"order":{"kind":"attackMove","at":[25,30]}},
        {"kind":"reinforce","group":"holders","side":0,"at":[20,40],"roster":[{"type":"rifle","count":1}],"expires":10,"order":{"kind":"hold"}}
    ]);
    let mut destroy = trigger("collapse", json!({"kind":"time","seconds":0.15}));
    destroy["actions"] = json!([{"kind":"destroy","box":[40,40,40,40]},
        {"kind":"objectiveComplete","objective":"global"}]);
    let mut m = scene(definition(
        json!([
            {"id":"walkers","units":[]},{"id":"attackers","units":[]},{"id":"holders","units":[]}
        ]),
        vec![actions, destroy],
    ));
    let damaged = 40 * 96 + 40;
    let untouched = 40 * 96 + 41;
    m.world.set_cell(damaged, b'#').unwrap();
    m.world.set_cell(untouched, b'#').unwrap();
    let hp = m.world.game.cell_hp[damaged];
    let neighbor_hp = m.world.game.cell_hp[untouched];
    let mut delivery = DeliveryState::new(&m).unwrap();
    let (_, events) = tick(&mut m, &mut delivery);
    assert!(events.iter().any(|e| matches!(e,TickEvent::AuthoredScenario(s) if s.text=="Advance" && s.localized["es"]=="Avanzar")));
    assert_eq!(m.world.game.cell_hp[damaged], hp - 25.0);
    assert_eq!(m.world.game.cell_hp[untouched], neighbor_hp);
    assert_eq!(
        m.authored_scenario.as_ref().unwrap().objectives[0].state,
        "active"
    );
    let walker = ids(&m, "walkers")[0];
    let attacker = ids(&m, "attackers")[0];
    let holder = ids(&m, "holders")[0];
    assert_eq!(
        m.world.game.units[&walker].world_goal,
        Some(Point { x: 51.0, z: 41.0 })
    );
    assert_eq!(
        m.world.game.units[&attacker].amove,
        Some(Point { x: 51.0, z: 61.0 })
    );
    assert!(!m.world.game.units[&holder].hold_pos);
    assert_eq!(m.world.game.units[&holder].world_goal, None);
    assert_eq!(m.world.game.units[&holder].amove, None);
    assert!(m.world.game.units[&holder].path.is_empty());
    checkpoint(&mut m, &mut delivery);
    let before = m.world.game.units[&walker].x;
    let mut collapsed = false;
    for _ in 0..12 {
        let (_, events) = tick(&mut m, &mut delivery);
        collapsed |= events.iter().any(|e| {
            matches!(
                e,
                TickEvent::Environment(crate::environment::Effect::Collapse(_))
            )
        });
    }
    assert!(collapsed);
    assert_eq!(m.world.game.chars[damaged], b'+');
    assert_eq!(m.world.game.chars[untouched], b'#');
    assert_eq!(m.world.game.cell_hp[untouched], neighbor_hp);
    assert!(m.world.game.units[&walker].x > before);
    assert_eq!(
        m.authored_scenario.as_ref().unwrap().objectives[0].state,
        "complete"
    );
    assert!(m.authored_scenario.as_ref().unwrap().deferred.is_empty());
    finish(&mut m, &mut delivery);
}

fn reinforce(group: &str, at: [usize; 2], kind: &str, expires: f64) -> Value {
    json!({"kind":"reinforce","group":group,"side":0,"at":at,
        "roster":[{"type":kind,"count":1}],"expires":expires,"order":{"kind":"hold"}})
}

#[test]
fn mode85_setup_first_free_bounded_placement_retry_and_expiry_keep_pending_groups_alive() {
    let mut arrivals = trigger("arrivals", json!({"kind":"time","seconds":0.05}));
    arrivals["actions"] = json!([
        reinforce("entry", [20, 20], "rifle", 10.0),
        reinforce("expiring", [50, 20], "rifle", 0.2)
    ]);
    let mut m = scene(definition(
        json!([
            {"id":"entry","units":[{"id":"blocker","type":"rifle","side":0,"x":20,"y":20}]},
            {"id":"expiring","units":[{"id":"doomed","type":"rifle","side":0,"x":50,"y":20}]},
            {"id":"sameCell","units":[{"id":"a","type":"rifle","side":0,"x":60,"y":40},{"id":"b","type":"rifle","side":0,"x":60,"y":40}]}
        ]),
        vec![
            arrivals,
            trigger(
                "finallyLost",
                json!({"kind":"groupLost","group":"expiring"}),
            ),
        ],
    ));
    let same = ids(&m, "sameCell");
    assert_eq!(
        (
            m.world.game.units[&same[0]].x,
            m.world.game.units[&same[0]].z
        ),
        (121.0, 81.0)
    );
    assert_eq!(
        (
            m.world.game.units[&same[1]].x,
            m.world.game.units[&same[1]].z
        ),
        (119.0, 79.0)
    );
    for y in 17..=23 {
        for x in 17..=23 {
            if [x, y] != [20, 20] {
                m.world.set_cell(y * 96 + x, b'W').unwrap();
            }
        }
    }
    m.world.set_cell(20 * 96 + 50, b'W').unwrap();
    let doomed = ids(&m, "expiring")[0];
    let mut delivery = DeliveryState::new(&m).unwrap();
    tick(&mut m, &mut delivery);
    assert_eq!(
        ids(&m, "entry").len(),
        1,
        "free cells beyond radius three are not used"
    );
    assert_eq!(m.authored_scenario.as_ref().unwrap().deferred.len(), 2);
    checkpoint(&mut m, &mut delivery);
    m.world.game.units.get_mut(&doomed).unwrap().hp = 0.0;
    tick(&mut m, &mut delivery);
    assert_eq!(
        count(&m, "finallyLost", "match"),
        0,
        "pending replacement prevents groupLost"
    );
    m.world.set_cell(17 * 96 + 17, b'.').unwrap();
    tick(&mut m, &mut delivery);
    assert_eq!(ids(&m, "entry").len(), 2);
    let arrived = ids(&m, "entry")[1];
    assert_eq!(
        (
            m.world.game.units[&arrived].x,
            m.world.game.units[&arrived].z
        ),
        (35.0, 35.0)
    );
    assert_eq!(count(&m, "finallyLost", "match"), 0);
    for _ in 0..4 {
        tick(&mut m, &mut delivery);
    }
    assert_eq!(count(&m, "finallyLost", "match"), 1);
    assert!(m.authored_scenario.as_ref().unwrap().deferred.is_empty());
    m.world.set_cell(20 * 96 + 50, b'.').unwrap();
    for _ in 0..3 {
        tick(&mut m, &mut delivery);
    }
    assert!(
        ids(&m, "expiring").is_empty(),
        "an expired arrival never revives when its entry reopens"
    );
    finish(&mut m, &mut delivery);
}

#[test]
fn mode85_live_population_and_type_caps_retry_after_canonical_retirement() {
    for (kind, initial) in [("rifle", 9), ("tiger", 1)] {
        let units = (0..initial)
            .map(|i| json!({"id":format!("initial{i}"),"type":kind,"side":0,"x":20+i*3,"y":20}))
            .collect::<Vec<_>>();
        let mut request = trigger("request", json!({"kind":"time","seconds":0.05}));
        request["actions"] = json!([reinforce("reserve", [50, 30], kind, 10.0)]);
        let mut m = scene(definition(
            json!([{"id":"existing","units":units},{"id":"reserve","units":[]}]),
            vec![request],
        ));
        let mut delivery = DeliveryState::new(&m).unwrap();
        for _ in 0..3 {
            tick(&mut m, &mut delivery);
        }
        assert!(
            ids(&m, "reserve").is_empty(),
            "{kind} capacity must keep the arrival pending"
        );
        assert_eq!(m.authored_scenario.as_ref().unwrap().deferred.len(), 1);
        checkpoint(&mut m, &mut delivery);
        let released = ids(&m, "existing")[0];
        m.world.game.units.get_mut(&released).unwrap().hp = 0.0;
        for _ in 0..3 {
            tick(&mut m, &mut delivery);
        }
        assert!(!m.world.game.units.contains_key(&released));
        assert_eq!(ids(&m, "reserve").len(), 1);
        assert!(m.authored_scenario.as_ref().unwrap().deferred.is_empty());
        finish(&mut m, &mut delivery);
    }
}

#[test]
fn mode85_queue_reservations_block_arrivals_before_ordinary_lifecycle_resumes() {
    // Nonempty skirmish production queues remain guarded. These two accounting
    // probes clear their detached queues before any ordinary tick or snapshot.
    for (kind, queue) in [("rifle", vec!["rifle"; 9]), ("tiger", vec!["tiger"])] {
        let mut m = scene(definition(json!([]), vec![]));
        let id = m.next_unit_id;
        m.place_starting_building(0, "motorpool", 50 * 96 + 20)
            .unwrap();
        m.production.reconcile_queue(id, &queue).unwrap();
        let before = m.next_unit_id;
        assert_eq!(
            m.scenario_arrive(0, kind, [30, 20], &json!({"kind":"hold"}))
                .unwrap(),
            None
        );
        assert_eq!(m.next_unit_id, before);
        m.production.reconcile_queue(id, &[]).unwrap();
        assert!(
            m.scenario_arrive(0, kind, [30, 20], &json!({"kind":"hold"}))
                .unwrap()
                .is_some()
        );
        let mut delivery = DeliveryState::new(&m).unwrap();
        for _ in 0..4 {
            tick(&mut m, &mut delivery);
        }
        finish(&mut m, &mut delivery);
    }
}

#[test]
fn mov67_scenario_setup_and_successive_arrivals_index_resolved_poses_before_ticks() {
    use crate::unit_placement::tests::assert_current;
    let mut m = scene(definition(
        json!([
            {"id":"same","units":[
                {"id":"a","type":"rifle","side":0,"x":60,"y":40},
                {"id":"b","type":"rifle","side":0,"x":60,"y":40}]}
        ]),
        vec![],
    ));
    let setup = ids(&m, "same");
    assert_ne!(
        m.world.game.units[&setup[0]].at(),
        m.world.game.units[&setup[1]].at()
    );
    assert_current(&m.world.game, &m.grid);
    let mut poses = Vec::new();
    for order in [
        json!({"kind":"hold"}),
        json!({"kind":"move","at":[40,30]}),
        json!({"kind":"attackMove","at":[40,30]}),
    ] {
        let id = m
            .scenario_arrive(0, "rifle", [40, 20], &order)
            .unwrap()
            .unwrap();
        let u = &m.world.game.units[&id];
        assert_eq!(u.owner, 0);
        assert!(!poses.contains(&u.at()));
        poses.push(u.at());
        assert_eq!(
            m.world.game.flags[m.world.game.cell_of(u.x, u.z).unwrap()]
                & m.rules.kinds[u.kind].block(),
            0
        );
        assert_current(&m.world.game, &m.grid);
    }
    assert_eq!(poses[0], Point { x: 81.0, z: 41.0 });
    assert_eq!(poses[1], Point { x: 79.0, z: 39.0 });
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..6 {
        tick(&mut m, &mut delivery);
        assert_current(&m.world.game, &m.grid);
    }
    finish(&mut m, &mut delivery);
}
