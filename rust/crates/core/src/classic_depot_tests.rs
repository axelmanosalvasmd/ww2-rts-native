//! Original Depot commands over independent canonical Classic constructors.
use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, UnitControl},
    match_runtime::{MatchOptions, PlayerConfig},
    production::{JobStatus, ProductionState},
    weather::WeatherChoice,
    world::{CellRow, Recipient},
};
use serde_json::{Value as Json, json};

#[path = "classic_depot_lifecycle_tests.rs"]
mod lifecycle;
#[path = "classic_depot_income_tests.rs"]
mod income;
#[path = "classic_construction_queue_tests.rs"]
mod queue;
#[path = "base_consumer_completion_tests.rs"]
mod consumers;

fn bits(n: f64) -> String { format!("{:016x}", n.to_bits()) }
fn point(p: Option<Point>) -> Json { p.map_or(Json::Null, |p| json!([bits(p.x), bits(p.z)])) }
fn input(shared: bool) -> Json {
    let mut input = json!({"map":{"name":"Classic Depot canonical command","w":128,"h":128,
        "rows":vec![".".repeat(128);128],"spawns":[{"x":8,"y":8},{"x":112,"y":112}],
        "points":[{"id":"neutral","x":64,"y":64,"mp":0,"vp":0}]},
        "settings":{"mode":"classic","army":"standard","tech":true,"weather":false,"logistics":false},
        "names":["A","B"],"teams":[0,1],"factions":[0,1]});
    if shared {
        input["map"]["spawns"] = json!([{"x":8,"y":8},{"x":15,"y":8},{"x":112,"y":112}]);
        input["names"] = json!(["A","B","C"]);
        input["teams"] = json!([0,0,1]);
        input["factions"] = json!([0,1,0]);
    }
    input
}
fn make(input: &Json) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
    Match::new_classic(&map, MatchOptions {
        seed: 701, mode: Mode::Classic,
        players: input["names"].as_array().unwrap().iter().enumerate().map(|(i, name)| PlayerConfig {
            name: name.as_str().unwrap().into(), team: input["teams"][i].as_i64().unwrap() as i32,
            faction: if input["factions"][i] == 0 { Faction::Usa } else { Faction::Germany },
        }).collect(),
        shuffle: false, army: ArmySize::Standard, weather: WeatherChoice::Map,
        weather_seed: Some(217), living_weather: false, map_key: None,
        tech: true, supply: true, logistics: false, sandbox: false,
    }).unwrap()
}
fn own(m: &Match, slot: usize, kind: &str) -> UnitId {
    m.world.game.units.values().find(|u| u.owner == slot && m.rules.kinds[u.kind].name == kind).unwrap().id
}
fn choose(m: &Match) -> (usize, Point) {
    let hq = &m.world.game.units[&own(m, 0, "hq")];
    let (index, node) = m.classic_runtime.as_ref().unwrap().nodes.iter().enumerate()
        .find(|(_, n)| n.depot.is_none() && math::hypot(n.at.x - hq.x, n.at.z - hq.z) <= 29.).unwrap();
    (index, Point { x: node.at.x + 1.25, z: node.at.z - 0.75 })
}
fn choose_tie(m: &mut Match) -> Json {
    let nodes = m.classic_runtime.as_ref().unwrap().nodes.clone();
    for (i, a) in nodes.iter().enumerate() {
        for b in nodes.iter().skip(i + 1) {
            if math::hypot(a.at.x - b.at.x, a.at.z - b.at.z) > 16. { continue; }
            let at = Point { x: (a.at.x + b.at.x) / 2., z: (a.at.z + b.at.z) / 2. };
            let mut candidates = nodes.iter().enumerate().map(|(index, n)|
                (index, math::hypot(n.at.x - at.x, n.at.z - at.z)))
                .filter(|(_, d)| *d <= 8.).collect::<Vec<_>>();
            candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
            if candidates.len() != 2 || candidates[0].1 != candidates[1].1 { continue; }
            if !m.recipient_sees(0, a.at).unwrap() || !m.recipient_sees(0, b.at).unwrap() { continue; }
            return json!({"x":at.x,"z":at.z,"first":candidates[0].0,"second":candidates[1].0});
        }
    }
    panic!("natural allied HQ nodes have no visible exact two-node midpoint tie");
}
fn project(m: &Match) -> Json {
    json!({"tick":m.world.game.tick,"next":m.next_unit_id,"nextJob":m.production.next_job_id(),
        "wallets":m.start.economy.players.iter().map(|p| json!([bits(p.mp),bits(p.mun),bits(p.fuel)])).collect::<Vec<_>>(),
        "spent":m.story.iter().map(|s| bits(s.mp_spent)).collect::<Vec<_>>(),
        "nodes":m.classic_runtime.as_ref().unwrap().nodes.iter().map(|n| json!([
            n.cell,point(Some(n.at)),n.depot.unwrap_or(0),bits(n.rate),n.fuel])).collect::<Vec<_>>(),
        "units":m.world.game.units.values().map(|u| {
            let b = m.production.building(u.id);
            let control = &m.commands.units[&u.id];
            json!({"id":u.id,"owner":u.owner,"type":m.rules.kinds[u.kind].name,"hp":bits(u.hp),
                "at":point(Some(u.at())),"built":u.built.map(bits),"cells":u.cells,
                "build":control.build_target,"path":u.path.iter().map(|p| point(Some(*p))).collect::<Vec<_>>(),
                "routeEnd":point(control.route_end),
                "queues":b.map(|b| b.queue().collect::<Vec<_>>()),
                "jobs":b.map_or_else(Vec::new, |b| b.rows().into_iter().map(|j| json!([
                    j.job.id,j.job.kind,bits(j.job.charge.mp),bits(j.job.charge.fuel),
                    if j.status == JobStatus::Active {"active"} else {"waiting"}])).collect::<Vec<_>>())})
        }).collect::<Vec<_>>(),
        "terrain":String::from_utf8(m.world.game.chars.clone()).unwrap(),
        "sections":m.world.game.units.values().filter(|u| m.rules.kinds[u.kind].name == "depot")
            .map(|u| json!([u.id,u.cells.as_ref().unwrap().iter().map(|c| json!([
                c,char::from(m.world.game.chars[*c as usize]).to_string(),bits(f64::from(m.world.game.cell_hp[*c as usize]))
            ])).collect::<Vec<_>>()])).collect::<Vec<_>>(),
    })
}

struct AtomicState {
    falling: Vec<crate::environment::FallingSection>,
    firing: crate::match_firing::FiringState,
    combat: crate::match_combat::CombatState,
    abilities: crate::match_abilities::AbilityState,
    recovery: crate::match_recovery::RecoveryState,
    logistics: crate::logistics::LogisticsState,
    field_works: crate::field_works::FieldWorksRuntime,
    projected: Json,
    production: ProductionState,
    wallet: crate::economy_state::EconomyState,
    story: Vec<crate::match_story::PlayerStory>,
    units: BTreeMap<UnitId, Unit>,
    controls: BTreeMap<UnitId, UnitControl>,
    nodes: Option<Vec<crate::match_world::ResourceNode>>,
    footprints: BTreeMap<usize, crate::match_runtime::BuildingCell>,
    log: Vec<CellRow>,
    new_cells: Vec<CellRow>,
    journal: Vec<usize>,
    base: u64,
    end: u64,
    pending: Vec<Option<BTreeSet<usize>>>,
    versions: crate::world::RegionVersions,
    structures: Vec<crate::world::WorldStructure>,
    flags: Vec<u16>,
    cell_hp_bits: Vec<u32>,
    heights: Option<Vec<i8>>,
    physical_sections: BTreeMap<u32, crate::game::Section>,
    rallies: BTreeMap<u32, Option<Point>>,
    grid_nodes: Vec<Vec<UnitId>>,
    sections: BTreeMap<usize, crate::world::SectionMetadata>,
}
fn pending(m: &Match) -> Vec<Option<BTreeSet<usize>>> {
    (0..m.world.game.players.len()).map(Recipient::Player).chain(std::iter::once(Recipient::Spectators))
        .map(|r| m.world.pending(r).cloned()).collect()
}
impl AtomicState {
    fn capture(m: &Match) -> Self {
        Self { falling: m.environment.falling_sections().to_vec(), firing: m.firing.clone(),
            combat: m.combat.clone(), abilities: m.abilities.clone(), recovery: m.recovery.clone(),
            logistics: m.logistics.clone(), field_works: m.field_works.clone(),
            projected: m.classic_runtime.as_ref().map_or(Json::Null, |_| project(m)),
            production: m.production.clone(), wallet: m.start.economy.clone(),
            story: m.story.clone(), units: m.world.game.units.clone(), controls: m.commands.units.clone(),
            nodes: m.classic_runtime.as_ref().map(|s| s.nodes.clone()), footprints: m.building_cells.clone(),
            log: m.world.cell_log().to_vec(), new_cells: m.world.new_cells().to_vec(),
            journal: m.world.journal().to_vec(), base: m.world.journal_base(), end: m.world.journal_end().unwrap(),
            pending: pending(m), versions: m.world.versions, structures: m.world.structures.clone(),
            flags: m.world.game.flags.clone(),
            cell_hp_bits: m.world.game.cell_hp.iter().map(|n| n.to_bits()).collect(),
            heights: m.world.game.height.clone(), physical_sections: m.world.game.sections.clone(),
            rallies: m.building_rallies.clone(),
            grid_nodes: m.classic_runtime.iter().flat_map(|s| &s.nodes).map(|n|
                m.grid.in_order(&m.world.game.units, (n.at.x, n.at.z), 16., |_, _| true)).collect(),
            sections: m.world.sections.clone() }
    }
    fn unchanged(self, m: &Match) {
        assert_eq!(m.environment.falling_sections(), self.falling);
        assert_eq!(m.firing, self.firing);
        assert_eq!(m.combat, self.combat);
        assert_eq!(m.abilities, self.abilities);
        assert_eq!(m.recovery, self.recovery);
        assert_eq!(m.logistics, self.logistics);
        assert_eq!(m.field_works, self.field_works);
        if self.projected.is_null() { assert!(m.classic_runtime.is_none()); }
        else { assert_eq!(project(m), self.projected); }
        assert_eq!(m.production, self.production);
        assert_eq!(m.start.economy, self.wallet);
        assert_eq!(m.story, self.story);
        assert_eq!(m.commands.units, self.controls);
        assert_eq!(m.world.game.units.len(), self.units.len());
        for (id, before) in self.units {
            let mut actual = m.world.game.units[&id].clone();
            let mut before = before;
            // Unit's pace may be NaN. Verify its exact bits before comparing all
            // remaining fields on copies; no canonical state is normalized.
            assert_eq!(actual.pace.to_bits(), before.pace.to_bits());
            actual.pace = 0.; before.pace = 0.;
            assert_eq!(actual, before);
        }
        assert_eq!(m.classic_runtime.as_ref().map(|s| s.nodes.clone()), self.nodes);
        assert_eq!(m.building_cells, self.footprints);
        assert_eq!(m.world.cell_log(), self.log);
        assert_eq!(m.world.new_cells(), self.new_cells);
        assert_eq!(m.world.journal(), self.journal);
        assert_eq!(m.world.journal_base(), self.base);
        assert_eq!(m.world.journal_end().unwrap(), self.end);
        assert_eq!(pending(m), self.pending);
        assert_eq!(m.world.versions, self.versions);
        assert_eq!(m.world.structures, self.structures);
        assert_eq!(m.world.sections, self.sections);
        assert_eq!(m.world.game.flags, self.flags);
        assert_eq!(m.world.game.cell_hp.iter().map(|n| n.to_bits()).collect::<Vec<_>>(), self.cell_hp_bits);
        assert_eq!(m.world.game.height, self.heights);
        assert_eq!(m.world.game.sections, self.physical_sections);
        assert_eq!(m.building_rallies, self.rallies);
        assert_eq!(m.classic_runtime.iter().flat_map(|s| &s.nodes).map(|n|
            m.grid.in_order(&m.world.game.units, (n.at.x, n.at.z), 16., |_, _| true)).collect::<Vec<_>>(), self.grid_nodes);
    }
}
fn replay(name: &str) -> Match {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/classic-depot.json");
    let fixture: Json = serde_json::from_str(&std::fs::read_to_string(path)
        .expect("Root must grant the exact new Source recipe before paired Depot replay")).unwrap();
    assert_eq!(fixture["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(fixture["node"], "v24.21.0"); assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 3);
    let case = fixture["cases"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap();
    let tied = name == "classic-depot-equal-distance-node-order-and-next-free";
    assert_eq!(case["input"], input(tied));
    let mut m = make(&case["input"]);
    assert_eq!(project(&m), case["initial"]);
    let mut commands = 0;
    for row in case["records"].as_array().unwrap() {
        match row["op"].as_str().unwrap() {
            "warmup" => {
                assert_eq!(row["count"], 1);
                assert_eq!(m.world.game.tick, 0);
                m.advance_classic_base_tick().unwrap();
                let selection = if tied { choose_tie(&mut m) } else {
                    let (index, at) = choose(&m);
                    json!({"index":index,"x":at.x,"z":at.z})
                };
                assert_eq!(selection, case["selection"]);
            }
            "command" => {
                let before = AtomicState::capture(&m);
                let command = Command::from_json(&row["cmd"]).unwrap();
                let result = m.execute_command(row["slot"].as_u64().unwrap() as usize, &command, false).unwrap();
                assert_eq!(json!(result.map(|d| d.name())), row["result"]);
                if result.is_some() { before.unchanged(&m); }
                commands += 1;
            }
            _ => panic!("unexpected Source Depot action"),
        }
        assert_eq!(project(&m), row["expected"], "{name}, command {commands}");
    }
    assert_eq!(commands, if tied {3} else if name == "classic-depot-paid-node-and-duplicate-refusal" {2} else {6});
    m
}
#[test]
fn base26_classic_actual_depot_node_payment_engineer_route_and_duplicate_atomic_refusal() {
    let m = replay("classic-depot-paid-node-and-duplicate-refusal");
    let depot = &m.world.game.units[&own(&m, 0, "depot")];
    assert_eq!(depot.hp, 150.); assert_eq!(depot.built, Some(0.));
    assert_eq!(depot.cells.as_ref().unwrap().len(), 4);
    assert_eq!(m.commands.units[&own(&m, 0, "engineer")].build_target, Some(depot.id));
    assert_eq!(m.story[0].mp_spent, 60.);
    assert_eq!(m.classic_runtime.as_ref().unwrap().nodes.iter().filter(|n| n.depot == Some(depot.id)).count(), 1);
    assert!(m.validate_construction_tick().is_err(), "ordinary full construction admission must stay unchanged");
}
#[test]
fn base26_classic_actual_depot_foreign_nonbuilder_site_and_earned_funds_refusals() {
    let m = replay("classic-depot-real-selection-and-funds-refusals");
    assert!(m.classic_runtime.as_ref().unwrap().nodes.iter().all(|n| n.depot.is_none()));
    assert!(m.start.economy.players[0].mp >= 0. && m.start.economy.players[0].mp < 60.);
    assert_eq!(m.production.building(own(&m, 0, "hq")).unwrap().jobs().len(), 2);
}
#[test]
fn base26_classic_actual_depot_story_failure_discards_stamp_route_node_and_charge() {
    let mut m = make(&input(false));
    m.advance_classic_base_tick().unwrap();
    let (index, at) = choose(&m);
    assert!(m.classic_runtime.as_ref().unwrap().nodes[index].depot.is_none());
    let engineer = own(&m, 0, "engineer");
    // Bookkeeping-only fault at the final fallible boundary. Actors, nodes,
    // wallet, pose, clock, paid jobs and the random stream remain genuine.
    m.story.clear();
    let before = AtomicState::capture(&m);
    let command = Command::from_json(&json!({"t":"build","ids":[engineer],"kind":"depot","x":at.x,"z":at.z})).unwrap();
    let error = m.execute_command(0, &command, false).expect_err("missing story must refuse before publication");
    assert!(error.contains("story"), "must reach the staged story guard: {error}");
    before.unchanged(&m);
}

#[test]
fn base26_classic_actual_depot_tied_nodes_preserve_order_then_select_next_free() {
    let m = replay("classic-depot-equal-distance-node-order-and-next-free");
    let depots = m.world.game.units.values().filter(|u| m.rules.kinds[u.kind].name == "depot").collect::<Vec<_>>();
    assert_eq!(depots.len(), 2);
    assert!(depots.iter().all(|u| u.owner == 0 && u.hp == 150. && u.built == Some(0.)));
    assert_eq!(m.story[0].mp_spent, 120.);
    assert_eq!(m.story[1].mp_spent, 0.);
    assert_eq!(m.commands.units[&own(&m, 0, "engineer")].build_target, Some(depots[1].id));
}
