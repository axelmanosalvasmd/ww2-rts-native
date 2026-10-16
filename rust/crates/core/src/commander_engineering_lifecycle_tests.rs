use super::*;
use crate::{
    commander_ai::Level,
    commander_tactics,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::Command,
    match_human_observation::HumanObservationMemory,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    random::MatchRandom,
    recipient_snapshot::SnapshotCache,
    weather::WeatherChoice,
};

fn scene(kind: &str) -> (Match, u32, usize) {
    let mut rows = vec![vec![b'.'; 60]; 60];
    if kind == "bridge" {
        for row in &mut rows {
            row[20] = b'W';
        }
        rows[20][20] = b'=';
    }
    let rows: Vec<_> = rows
        .into_iter()
        .map(|row| String::from_utf8(row).unwrap())
        .collect();
    let point = if kind == "bridge" { 30 } else { 20 };
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI engineering lifecycle", "w":60,"h":60,
            "rows":rows,"heights":vec!["0".repeat(60);60],
            "spawns":[{"x":4,"y":4},{"x":55,"y":55}],"points":[{"x":point,"y":point}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 751,
            mode: Mode::Default,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::Usa,
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
    state.start.economy.players[0].mp = 1000.0;
    state.start.points[0].owner = Some(0);
    let worker = state
        .world
        .game
        .units
        .values()
        .find(|unit| unit.owner == 0 && state.rules.kinds[unit.kind].name == "rifle")
        .unwrap()
        .id;
    let unit = state.world.game.units.get_mut(&worker).unwrap();
    unit.x = 33.0;
    unit.z = 41.0;
    state.grid.update(worker, unit);
    let cell = 20 * 60 + 20;
    if kind == "bridge" {
        assert_eq!(state.world.initial().chars[cell], b'=');
        let ids: Vec<_> = state.world.game.units.keys().copied().collect();
        let mut context = crate::environment::Context {
            tick: state.world.game.tick as u64,
            seed: &mut state.start.environment_seed,
            next_id: &mut state.next_unit_id,
            unit_order: &ids,
            grid: &state.grid,
            rain: 0.0,
            wet: 0.0,
            wind_angle: 0.0,
            wind_strength: 0.0,
        };
        state
            .environment
            .damage_section(
                &mut state.world,
                cell,
                5000.0,
                &mut context,
                &crate::environment::DamageContext::plain(&BTreeSet::new()),
            )
            .unwrap();
        assert_eq!(state.world.game.chars[cell], b'W');
        assert!(state.world.game.sections[&(cell as u32)].failed);
    } else if kind == "fill" {
        state.world.set_cell(cell, b'F').unwrap();
    } else {
        state.world.set_cell(cell, b'N').unwrap();
        state.world.mines.insert(cell, 1);
        state.world.mine_seen.insert(cell, 1);
    }
    (state, worker, cell)
}

fn crew(policy: &commander_tactics::State, kind: &str) -> Value {
    serde_json::to_value(&policy.field).unwrap()[format!("{kind}_crew")].clone()
}

fn bridges(policy: &commander_tactics::State) -> Value {
    serde_json::to_value(&policy.field).unwrap()["bridges"].clone()
}

fn ticks(state: &mut Match, count: usize) {
    for _ in 0..count {
        state.advance_tick().unwrap();
        assert!(state.tick_state.fault().is_none());
        let snapshots = SnapshotCache::new(state).unwrap();
        for slot in 0..state.start.players.len() {
            snapshots.recipient(slot).unwrap();
        }
    }
}

#[test]
fn if25_ai_engineering_delivered_fill_and_mines_survive_ordinary_ticks_and_all_seat_snapshots() {
    for kind in ["fill", "demine"] {
        let (mut state, worker, cell) = scene(kind);
        ticks(&mut state, 2);
        let mut memory = HumanObservationMemory::new(0);
        let view = state.human_commander_observation(&mut memory).unwrap();
        let context = view.packet.input().runtime.as_ref().unwrap();
        let active = context
            .field
            .as_ref()
            .expect("ordinary producer must select the active field policy");
        if kind == "fill" {
            assert!(context.spoiled_cells.contains(&cell));
            assert_eq!(active.initial_chars[cell], b'.');
        } else {
            assert_eq!(active.found_mines, [Point { x: 41.0, z: 41.0 }]);
            assert!(active.mines.is_empty());
        }
        let mut policy = commander_tactics::State::default();
        let mut random = MatchRandom::new(751);
        let commands =
            commander_tactics::plan(&view.packet, &[], &mut policy, Level::Normal, &mut random)
                .unwrap();
        let dig = commands
            .iter()
            .find(|command| command["t"] == "dig" && command["kind"] == kind)
            .unwrap_or_else(|| panic!("no {kind} proposal in {commands:?}"));
        assert_eq!(dig["ids"][0], worker);
        assert_eq!(
            state
                .execute_command(0, &Command::from_json(dig).unwrap(), false)
                .unwrap(),
            None
        );
        assert!(state.world.game.units[&worker].dig);
        assert_eq!(state.field_works.jobs()[&worker].kind.as_str(), kind);
        assert!(
            state.field_works.jobs()[&worker]
                .cells
                .iter()
                .any(|row| row.cell == cell)
        );
        let encoded = serde_json::to_vec(&policy).unwrap();
        let mut resumed: commander_tactics::State = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(resumed, policy);
        ticks(&mut state, 4);
        let view = state.human_commander_observation(&mut memory).unwrap();
        commander_tactics::plan(&view.packet, &[], &mut resumed, Level::Normal, &mut random)
            .unwrap();
        let jobs = crew(&resumed, kind);
        assert_eq!(jobs[worker.to_string().as_str()]["dug"], true);
        ticks(&mut state, 240);
        assert!(!state.world.game.units[&worker].dig);
        assert_eq!(state.world.game.chars[cell], b'.');
        assert!(!state.world.mines.contains_key(&cell));
        let view = state.human_commander_observation(&mut memory).unwrap();
        commander_tactics::plan(&view.packet, &[], &mut resumed, Level::Normal, &mut random)
            .unwrap();
        assert!(crew(&resumed, kind).as_object().unwrap().is_empty());
    }
}

#[test]
fn if25_ai_engineering_remembered_bridge_crew_survives_rebuild_ticks_and_all_seat_snapshots() {
    let (mut state, worker, cell) = scene("bridge");
    ticks(&mut state, 2);
    let mut memory = HumanObservationMemory::new(0);
    let view = state.human_commander_observation(&mut memory).unwrap();
    assert!(
        view.packet
            .input()
            .runtime
            .as_ref()
            .unwrap()
            .field
            .as_ref()
            .expect("ordinary bridge packet must use the active field policy")
            .bridges
            .contains(&cell)
    );
    assert_eq!(view.packet.input().chars[cell], b'W');
    let mut policy = commander_tactics::State::default();
    let mut random = MatchRandom::new(751);
    let commands =
        commander_tactics::plan(&view.packet, &[], &mut policy, Level::Normal, &mut random)
            .unwrap();
    let dig = commands
        .iter()
        .find(|command| command["t"] == "dig" && command["kind"] == "bridge")
        .unwrap_or_else(|| panic!("no bridge proposal in {commands:?}"));
    assert_eq!(dig["ids"][0], worker);
    assert_eq!(dig["x"], 41.0);
    assert_eq!(dig["z"], 41.0);
    assert_eq!(
        state
            .execute_command(0, &Command::from_json(dig).unwrap(), false)
            .unwrap(),
        None
    );
    assert!(state.world.game.units[&worker].dig);
    assert_eq!(state.field_works.jobs()[&worker].kind.as_str(), "bridge");
    assert_eq!(bridges(&policy), json!([cell]));
    assert_eq!(
        crew(&policy, "bridge")[worker.to_string().as_str()]["dug"],
        false
    );
    let mut resumed: commander_tactics::State =
        serde_json::from_slice(&serde_json::to_vec(&policy).unwrap()).unwrap();
    assert_eq!(resumed, policy);
    ticks(&mut state, 4);
    let view = state.human_commander_observation(&mut memory).unwrap();
    commander_tactics::plan(&view.packet, &[], &mut resumed, Level::Normal, &mut random).unwrap();
    assert_eq!(
        crew(&resumed, "bridge")[worker.to_string().as_str()]["dug"],
        true
    );
    let active: commander_tactics::State =
        serde_json::from_slice(&serde_json::to_vec(&resumed).unwrap()).unwrap();
    assert_eq!(active, resumed);
    ticks(&mut state, 240);
    assert!(!state.world.game.units[&worker].dig);
    assert_eq!(state.world.game.chars[cell], b'=');
    assert!(!state.world.game.sections[&(cell as u32)].failed);
    let view = state.human_commander_observation(&mut memory).unwrap();
    let commands =
        commander_tactics::plan(&view.packet, &[], &mut resumed, Level::Normal, &mut random)
            .unwrap();
    assert!(crew(&resumed, "bridge").as_object().unwrap().is_empty());
    assert_eq!(bridges(&resumed), json!([cell]));
    assert!(
        !commands
            .iter()
            .any(|command| command["t"] == "dig" && command["kind"] == "bridge")
    );
}
