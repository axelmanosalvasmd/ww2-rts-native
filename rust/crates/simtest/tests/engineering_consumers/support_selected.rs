//! Complete original support acceptance, pending authentic capture and execution.
use super::*;
use ww2_core::{
    navigation::{TerrainView, Versions},
    orders::OrderKey,
    pathfinding::{PathRules, PathTerrain, Pathfinding},
    random::StreamId,
};

fn selected_fixture() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/engineering-support-selected.json.gz");
    let bytes = std::fs::read(path).expect("authentic Source capture pending Root grant");
    let mut text = String::new();
    GzDecoder::new(bytes.as_slice())
        .read_to_string(&mut text)
        .unwrap();
    let f: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    let input: Value = serde_json::from_str(include_str!(
        "../fixtures/engineering-support-selected-input.json"
    ))
    .unwrap();
    assert_eq!(f["input"], input);
    assert_eq!(f["checks"].as_array().unwrap().len(), 6);
    assert_eq!(f["scenes"].as_array().unwrap().len(), 7);
    let body_hashes = [
        "96ec6b5523b003026a73d92d1f84ea79f88401066eb98bfd779d0f28102eb18e",
        "9735a564a0ca42665d18935812b30b427f3898a5e8ec67f4956916517c91ae96",
        "590bb8794b26620f132f866eb2f66194fb9ac11fc13b2cdf32e5aeee3bb5d655",
        "eab56416cd32edf17d37aedec2d33b861c8f02608608de016871962827d38ed5",
        "72edae656cfd626381f31a3af348d67984a65f992e23f871c0e9211bda59fb38",
        "c04e301c79ccfdca96e52cc5a621706cad76264a167adbdd1c50a9e9505c24c0",
    ];
    for ((name, check), hash) in input["checks"]
        .as_array()
        .unwrap()
        .iter()
        .zip(f["checks"].as_array().unwrap())
        .zip(body_hashes)
    {
        assert_eq!(*name, check["name"]);
        assert_eq!(check["passed"], true);
        assert_eq!(check["bodySha256"], hash);
    }
    f
}

fn selected_scene<'a>(f: &'a Value, name: &str) -> &'a Value {
    let scenes: Vec<_> = f["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|scene| scene["name"] == name)
        .collect();
    assert_eq!(scenes.len(), 1);
    assert!(
        f["input"]["nativeScenes"]
            .as_array()
            .unwrap()
            .contains(&json!(name))
    );
    let trace = scenes[0];
    let mut map = f["input"]["map"].clone();
    map["w"] = json!(20);
    map["h"] = json!(20);
    let mut rows = vec![".".repeat(20); 20];
    if name == "Fortifications wire:0" {
        rows.fill(format!("{}XX{}", ".".repeat(9), ".".repeat(9)));
    }
    if name == "test.js:701:0" {
        rows[10] = format!("{}BB{}HH{}", ".".repeat(8), ".".repeat(4), ".".repeat(4));
    }
    if name == "Bombing:0" {
        rows[10] = format!("..{}..", "BB..".repeat(4));
    }
    map["rows"] = json!(rows);
    if name.starts_with("A blast that lowers a cliff") {
        map["heights"] = json!(vec![format!("{}2{}", "0".repeat(10), "0".repeat(9)); 20]);
    }
    assert_eq!(trace["map"], map, "entire original authored map");
    trace
}

fn make_scene(input: &Value, trace: &Value) -> Match {
    let setup = &trace["setup"];
    let mut m = make(input, &trace["map"], setup, &json!({}));
    clear(&mut m);
    for (slot, player) in m.start.players.iter_mut().enumerate() {
        player.spawn = point(&setup["players"][slot]["spawn"]);
        m.start.economy.players[slot].mp = number(&setup["players"][slot]["mp"]);
    }
    restore_environment(&mut m, setup);
    publish(&mut m, input, setup);
    m
}

fn support_projection(m: &Match) -> Value {
    let g = &m.world.game;
    let off_map = m
        .start
        .random
        .snapshot()
        .streams
        .into_iter()
        .find(|(kind, _)| *kind == StreamId::OffMap)
        .map(|(_, state)| format!("{state:016x}"));
    json!({
        "tick":g.tick,"environment":environment(m),
        "chars":String::from_utf8(g.chars.clone()).unwrap(),
        "ground":String::from_utf8(g.ground.clone().unwrap()).unwrap(),
        "objects":String::from_utf8(g.objects.clone().unwrap()).unwrap(),
        "height":g.height,"scar32":m.world.scar.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
        "wear32":g.wear.as_ref().unwrap().iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
        "hp32":g.cell_hp.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),"state":m.world.cell_state,
        "units":g.units.values().map(|u|json!([u.id,m.rules.kinds[u.kind].name,u.owner,u.x,u.z,u.hp,u.supp])).collect::<Vec<_>>(),
        "strikes":g.strikes.iter().map(|s|{
            let t=s.runtime.as_ref().unwrap();
            json!([t.kind.name(),s.owner,s.x,s.z,s.dir,t.delay,t.left,t.next,s.live])
        }).collect::<Vec<_>>(),
        "negativeLog":m.world.cell_log().iter().any(|row|row.height<0),"offMapState":off_map
    })
}

fn replay(trace: &Value, m: &mut Match, expected_ticks: usize, rounds: usize, kind: &str) {
    let frames = trace["frames"].as_array().unwrap();
    let actions = trace["actions"].as_array().unwrap();
    assert_eq!(frames.len(), expected_ticks);
    assert_eq!(actions.len(), rounds);
    assert_eq!(trace["runs"].as_array().unwrap().len(), rounds);
    compare(&support_projection(m), &trace["initial"], "support initial");
    let mut applied = 0_usize;
    let mut dispatched = 0_usize;
    let before_houses = m.world.game.chars.iter().filter(|c| **c == b'B').count();
    if kind == "bombing" {
        assert_eq!(before_houses, 8, "the complete original authored house row");
    }
    for (index, frame) in frames.iter().enumerate() {
        let before_tick = m.world.game.tick;
        let before_applied = applied;
        for action in actions
            .iter()
            .filter(|a| a["tick"].as_i64() == Some(before_tick))
        {
            assert_eq!(action["owner"], 0);
            assert_eq!(action["denial"], Value::Null);
            assert_eq!(action["request"]["t"], "support");
            assert_eq!(action["request"]["kind"], kind);
            let expected_request = if kind == "dive" {
                json!({"t":"support","kind":"dive","x":21,"z":21})
            } else {
                json!({"t":"support","kind":kind,"x":20,"z":21,"dir":0})
            };
            assert_eq!(action["request"], expected_request);
            assert_eq!(action["resetCooldown"], applied > 0);
            if applied > 0 {
                // Original repeated-barrage loops explicitly reset this input.
                m.start.players[0].support.insert(kind.into(), 0.);
            }
            m.enqueue_command(
                OrderKey {
                    tick: m.world.game.tick as u64 + 1,
                    issuer: 0,
                    sequence: applied as u64 + 1,
                },
                Command::from_json(&action["request"]).unwrap(),
            )
            .unwrap();
            applied += 1;
        }
        let report = m
            .advance_tick()
            .unwrap_or_else(|e| panic!("{} tick {}: {e}", trace["name"], index + 1));
        assert_eq!(report.tick, index as i64 + 1);
        assert_eq!(
            report.commands.len(),
            applied - before_applied,
            "actual support dispatch count"
        );
        for command in &report.commands {
            assert!(
                command.denial.is_none(),
                "original support request must be accepted"
            );
            assert_eq!(command.key.tick, report.tick as u64);
            assert_eq!(command.key.issuer, 0);
        }
        dispatched += report.commands.len();
        for phase in [
            TickPhase::Support,
            TickPhase::SupportTimers,
            TickPhase::LivingWeather,
        ] {
            assert!(report.phases.contains(&phase));
            assert!(!report.unavailable.contains(&phase));
        }
        assert!(
            m.finish.is_none(),
            "original scene must reach its complete timeline"
        );
        compare(&support_projection(m), frame, "actual support tick");
        if kind == "bombing" && index == 179 {
            let g = &m.world.game;
            assert!(
                g.chars.iter().filter(|c| **c == b'B').count() <= before_houses / 4,
                "most of the row is gone"
            );
            assert!(
                g.chars.iter().filter(|c| **c == b'+').count() >= 10,
                "big craters"
            );
            assert!(
                g.height
                    .as_ref()
                    .unwrap()
                    .iter()
                    .filter(|h| **h < 0)
                    .count()
                    >= 9 * 3,
                "each bomb digs a 3x3 patch"
            );
            assert!(
                m.world.cell_log().iter().any(|row| row.height < 0),
                "the dip is sent to clients"
            );
        }
    }
    assert_eq!(applied, rounds);
    assert_eq!(
        dispatched, rounds,
        "every original purchased support request dispatched"
    );
    compare(&support_projection(m), &trace["final"], "support final");
}

fn no_horizontal_cliffs(m: &Match, label: &str) {
    let g = &m.world.game;
    let h = g.height.as_ref().unwrap();
    for c in 0..h.len() {
        if c % g.w > 0 {
            assert!(
                (i16::from(h[c]) - i16::from(h[c - 1])).abs() <= 1,
                "{label}"
            );
        }
    }
}

fn path(m: &Match, cache: &mut Pathfinding, from: Point, to: Point) -> Vec<Point> {
    let g = &m.world.game;
    assert!(g.flags.iter().all(|flag| flag & 3 != 3));
    assert!(m.world.mines.is_empty());
    let near = vec![0; g.flags.len()];
    let mines = vec![false; g.flags.len()];
    let fires: Vec<_> = (0..g.flags.len())
        .map(|c| m.world.fires.contains_key(&c))
        .collect();
    let v = m.world.versions;
    cache
        .find_path(
            PathTerrain {
                navigation: TerrainView {
                    width: g.w,
                    height: g.h,
                    flags: &g.flags,
                    heights: g.height.as_deref(),
                    world_known: false,
                    knowledge_id: 0,
                    versions: Versions {
                        infantry: v.infantry,
                        vehicle: v.vehicle,
                        naval: v.naval,
                        obstruction: v.obstruction,
                        knowledge: 0,
                    },
                    changed: None,
                    changes: &[],
                },
                near_walls: &near,
                known_mines: &mines,
                fires: &fires,
                bunkers: &[],
                wear: g.wear.as_deref(),
                wetness: g.wet.unwrap(),
                roads: m.world.roads,
            },
            PathRules::compiled().unwrap(),
            &m.rules.kinds[m.rules.kind("rifle").unwrap()],
            from,
            to,
            None,
        )
        .unwrap()
        .waypoints
}

#[test]
fn eng47_original_bombing_runs_five_passes_and_all_literal_outcomes() {
    let f = selected_fixture();
    let trace = selected_scene(&f, "Bombing:0");
    let actors = trace["setup"]["actors"].as_array().unwrap();
    assert_eq!(actors.len(), 1);
    assert_eq!(actors[0]["type"], "tank");
    assert_eq!(actors[0]["owner"], 1);
    assert_eq!(actors[0]["x"], 20);
    assert_eq!(actors[0]["z"], 21);
    let target = actors[0]["id"].as_u64().unwrap() as u32;
    let mut m = make_scene(&f["input"], trace);
    assert!(
        trace["runs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|run| run["seconds"] == 9)
    );
    replay(trace, &mut m, 900, 5, "bombing");
    no_horizontal_cliffs(&m, "no cliffs from craters");
    assert!(
        m.world
            .game
            .units
            .get(&target)
            .is_none_or(|u| u.hp < 360. * 0.5),
        "tank under the bombs is wrecked or badly hurt"
    );
}

#[test]
fn eng50_original_sixteen_barrages_sink_adjacent_rows_without_cliffs() {
    let f = selected_fixture();
    let trace = selected_scene(&f, "Scarring:1");
    assert!(trace["setup"]["actors"].as_array().unwrap().is_empty());
    assert!(
        trace["runs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|run| run["seconds"] == 10)
    );
    let mut m = make_scene(&f["input"], trace);
    replay(trace, &mut m, 3200, 16, "artillery");
    let g = &m.world.game;
    let h = g.height.as_ref().unwrap();
    assert!(
        [7, 13]
            .iter()
            .any(|y| h[y * 20..y * 20 + 20].iter().any(|h| *h < 0)),
        "ground beside the shell holes sinks"
    );
    no_horizontal_cliffs(&m, "no cliffs from shelling");
}

#[test]
fn eng51_original_dive_reopens_the_same_warmed_cliff_pathfinder() {
    let f = selected_fixture();
    let trace = selected_scene(
        &f,
        "A blast that lowers a cliff makes a previously unreachable region walkable:0",
    );
    assert!(trace["setup"]["actors"].as_array().unwrap().is_empty());
    let queries = trace["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 2);
    assert_eq!(queries[0]["tick"], 0);
    assert_eq!(queries[1]["tick"], 102);
    let mut m = make_scene(&f["input"], trace);
    let mut cache = Pathfinding::new();
    assert_eq!(queries[0]["from"], json!({"x":5,"z":21}));
    assert_eq!(queries[0]["to"], json!({"x":35,"z":21}));
    let from = point(&queries[0]["from"]);
    let to = point(&queries[0]["to"]);
    assert_eq!(queries[0]["from"], queries[1]["from"]);
    assert_eq!(queries[0]["to"], queries[1]["to"]);
    let before = path(&m, &mut cache, from, to);
    compare(
        &json!(before.iter().copied().map(p).collect::<Vec<_>>()),
        &queries[0]["path"],
        "original cliff before",
    );
    assert!(before.is_empty(), "two-level cliff separates the map");
    let version = m.world.versions.infantry;
    replay(trace, &mut m, 102, 1, "dive");
    assert_eq!(
        m.world.game.height.as_ref().unwrap()[10 * m.world.game.w + 10],
        0,
        "blast lowers the cliff, two levels in the middle of the hole"
    );
    assert!(m.world.versions.infantry > version);
    let after = path(&m, &mut cache, from, to);
    compare(
        &json!(after.iter().copied().map(p).collect::<Vec<_>>()),
        &queries[1]["path"],
        "original cliff after",
    );
    assert!(!after.is_empty(), "height changes invalidate weak regions");
}

fn moved_trace(f: &Value, name: &str) -> Match {
    let trace = selected_scene(f, name);
    let mut m = make_scene(&f["input"], trace);
    let frames = trace["frames"].as_array().unwrap();
    let actions = trace["actions"].as_array().unwrap();
    let mut applied = 0;
    let mut sequence = 0;
    let mut dispatched = 0;
    let mut collapses = Vec::new();
    let mut moved_crushes = 0;
    assert_eq!(name, "Fortifications wire:0");
    assert!(trace["queries"].as_array().unwrap().is_empty());
    compare(&support_projection(&m), &trace["initial"], "moved initial");
    let wire_first =
        (32.0 / m.rules.kinds[m.rules.kind("rifle").unwrap()].speed * 20.0).ceil() as usize;
    for (index, frame) in frames.iter().enumerate() {
        let before_tick = m.world.game.tick;
        for action in actions
            .iter()
            .filter(|a| a["tick"].as_i64() == Some(before_tick))
        {
            assert_ne!(action["kind"], "put");
            assert_eq!(action["denial"], Value::Null);
            assert_eq!(action["owner"], 0);
            assert_eq!(action["resetCooldown"], false);
            sequence += 1;
            m.enqueue_command(
                OrderKey {
                    tick: m.world.game.tick as u64 + 1,
                    issuer: 0,
                    sequence,
                },
                Command::from_json(&action["request"]).unwrap(),
            )
            .unwrap();
            applied += 1;
        }
        let before_chars = m.world.game.chars.clone();
        let report = m
            .advance_tick()
            .unwrap_or_else(|e| panic!("{name} tick {}: {e}", index + 1));
        assert!(report.commands.iter().all(|c| c.denial.is_none()));
        dispatched += report.commands.len();
        assert!(report.phases.contains(&TickPhase::Movement));
        assert!(m.finish.is_none(), "complete original moved timeline");
        for effect in report.environment() {
            if let ww2_core::environment::Effect::Collapse(collapse) = effect
                && collapse.section.is_none()
            {
                let g = &m.world.game;
                let cell = g.cell_of(collapse.position.x, collapse.position.z).unwrap();
                assert!(matches!(before_chars[cell], b'H' | b'X' | b'#' | b'T'));
                assert!(
                    g.units
                        .values()
                        .any(|u| m.rules.kinds[u.kind].name == "tank"
                            && g.cell_of(u.x, u.z) == Some(cell)),
                    "actual tank contact caused the terrain change"
                );
                moved_crushes += 1;
            }
        }
        let shots: Vec<_> = report
            .events
            .iter()
            .filter_map(|event| match event {
                ww2_core::match_tick::TickEvent::Environment(effect)
                | ww2_core::match_tick::TickEvent::Support(
                    ww2_core::match_support::SupportEvent::Environment(effect),
                ) => Some(effect),
                _ => None,
            })
            .filter_map(|effect| {
                if let ww2_core::environment::Effect::Collapse(c) = effect
                    && c.section.is_none()
                {
                    Some(effect.to_json())
                } else {
                    None
                }
            })
            .collect();
        if !shots.is_empty() {
            collapses.push(json!({"tick":report.tick,"shots":shots}));
        }
        compare(
            &support_projection(&m),
            frame,
            &format!("{name}:{}", index + 1),
        );
        if index + 1 == wire_first {
            let rifle = m
                .world
                .game
                .units
                .values()
                .find(|u| m.rules.kinds[u.kind].name == "rifle")
                .unwrap();
            assert!(
                rifle.x < 30.0,
                "wire slows the squad in the original open-ground time"
            );
        }
    }
    assert_eq!(applied, actions.len());
    assert_eq!(
        dispatched as u64, sequence,
        "every original request used actual command dispatch"
    );
    assert!(moved_crushes > 0, "actual moved crushing emitted effects");
    let expected = trace["contactCollapses"].as_array().unwrap();
    assert_eq!(collapses.len(), expected.len());
    for (actual, expected) in collapses.iter().zip(expected) {
        compare(actual, expected, "crushing chronological effect batch");
    }
    assert_eq!(frames.len(), wire_first + 200);
    m
}

#[test]
fn eng45_original_wire_crossing_uses_moved_crushing_and_events() {
    let f = selected_fixture();
    let m = moved_trace(&f, "Fortifications wire:0");
    let g = &m.world.game;
    let rifle = g
        .units
        .values()
        .find(|u| m.rules.kinds[u.kind].name == "rifle")
        .unwrap();
    assert!(rifle.x > 34.0, "but it gets through");
    assert_eq!(
        g.chars[15 * g.w + 10],
        b'.',
        "the tank flattened the wire it drove over"
    );
}

#[test]
fn eng51_original_scarring_dive_has_the_complete_round_hole_before_fill() {
    let f = selected_fixture();
    let trace = selected_scene(&f, "Scarring:0");
    let mut m = make_scene(&f["input"], trace);
    let action = &trace["actions"][0];
    assert_eq!(
        action["request"],
        json!({"t":"support","kind":"dive","x":21,"z":21})
    );
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        Command::from_json(&action["request"]).unwrap(),
    )
    .unwrap();
    compare(
        &support_projection(&m),
        &trace["initial"],
        "round hole initial",
    );
    for (index, frame) in trace["frames"]
        .as_array()
        .unwrap()
        .iter()
        .take(120)
        .enumerate()
    {
        let report = m
            .advance_tick()
            .unwrap_or_else(|e| panic!("Scarring dive tick{}: {e}", index + 1));
        assert!(report.commands.iter().all(|c| c.denial.is_none()));
        compare(
            &support_projection(&m),
            frame,
            "original Scarring before Fill",
        );
    }
    assert_eq!(m.world.game.tick, 120);
    let g = &m.world.game;
    let h = g.height.as_ref().unwrap();
    assert_eq!(
        [
            h[10 * g.w + 10],
            h[10 * g.w + 11],
            h[11 * g.w + 11],
            h[12 * g.w + 12]
        ],
        [-2, -1, -1, 0]
    );
    assert_eq!(g.chars[10 * g.w + 11], b'+', "the ring is cratered too");
    // The original paid Fill continuation belongs to ENG-52 and stays unexecuted here.
}

#[test]
fn eng50_original_short_shelling_uses_actual_cell_damage_and_client_state() {
    use ww2_core::environment::{Blast, Context, DamageContext};
    let f = selected_fixture();
    let scenes: Vec<_> = f["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["name"] == "Shelling short of a crater marks the ground:0")
        .collect();
    assert_eq!(scenes.len(), 1);
    let trace = scenes[0];
    assert!(trace["frames"].as_array().unwrap().is_empty());
    assert_eq!(trace["componentInput"], json!([[],{"x":21,"z":21},1,40,1]));
    let mut authored = f["input"]["map"].clone();
    authored["w"] = json!(20);
    authored["h"] = json!(20);
    authored["rows"] = json!(vec![".".repeat(20); 20]);
    assert_eq!(trace["map"], authored);
    let mut m = make_scene(&f["input"], trace);
    compare(
        &support_projection(&m),
        &trace["initial"],
        "short shelling initial",
    );
    assert!(
        m.world.game.units.is_empty(),
        "the original component actor list is empty"
    );
    let random = m.start.random.snapshot();
    let tick = m.world.game.tick;
    let ids = Vec::new();
    let mut context = Context {
        tick: u64::try_from(tick).unwrap(),
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &ids,
        grid: &m.grid,
        rain: m.world.game.rain,
        wet: m.world.game.wet.unwrap(),
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    };
    let skip = std::collections::BTreeSet::new();
    m.environment
        .damage_cells(
            &mut m.world,
            Blast {
                at: Point { x: 21.0, z: 21.0 },
                radius: 1.0,
                damage: 40.0,
                scar: 1.0,
            },
            &mut context,
            &mut DamageContext::plain(&skip),
        )
        .unwrap();
    assert_eq!(
        m.world.game.tick, tick,
        "the original component check advances no clock"
    );
    assert_eq!(
        m.start.random.snapshot(),
        random,
        "this component consumes no OffMap draw"
    );
    compare(
        &support_projection(&m),
        &trace["componentFinal"],
        "original short shelling complete output",
    );
    let cell = m.world.game.cell_of(21.0, 21.0).unwrap();
    assert!(m.world.scar[cell] > 0.02 && m.world.scar[cell] < 1.0);
    assert_eq!(m.world.game.chars[cell], b'.', "churned, not cratered");
    assert_ne!((m.world.cell_state[cell] >> 5) & 3, 0, "clients are told");
}

#[test]
fn eng51_actual_dive_skips_the_second_dent_under_a_river() {
    let f = selected_fixture();
    let original = selected_scene(&f, "Scarring:0");
    for (surface, expected_surface, expected_level) in [(".", b'+', -2), ("W", b'W', -1)] {
        // Authored initial terrain only. Source digAt first dents either cell;
        // its second, inner-half pass excludes W (shared/sim.js:2779-2784).
        // This new branch witness has Source-read expectations, not a new capture.
        let mut scene = json!({"map":original["map"],"setup":original["setup"]});
        let mut row = scene["map"]["rows"][10].as_str().unwrap().to_owned();
        row.replace_range(10..11, surface);
        scene["map"]["rows"][10] = json!(row);
        let mut m = make_scene(&f["input"], &scene);
        let cell = 10 * m.world.game.w + 10;
        assert_eq!(m.world.game.height.as_ref().unwrap()[cell], 0);
        assert_eq!(m.world.game.chars[cell], surface.as_bytes()[0]);
        m.enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            Command::from_json(&json!({"t":"support","kind":"dive","x":21,"z":21})).unwrap(),
        )
        .unwrap();
        let mut dispatched = 0;
        for _ in 0..120 {
            let report = m.advance_tick().unwrap();
            assert!(
                report
                    .commands
                    .iter()
                    .all(|command| command.denial.is_none())
            );
            dispatched += report.commands.len();
            assert!(report.phases.contains(&TickPhase::Support));
            assert!(m.finish.is_none(), "the actual dive timeline must complete");
        }
        assert_eq!(dispatched, 1);
        assert_eq!(m.world.game.tick, 120);
        assert_eq!(m.world.game.height.as_ref().unwrap()[cell], expected_level);
        assert_eq!(m.world.game.chars[cell], expected_surface);
        assert!(
            m.world.game.strikes.is_empty(),
            "the real dive was consumed"
        );
    }
}
