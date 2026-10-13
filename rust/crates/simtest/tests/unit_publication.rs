use serde_json::json;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{LOCAL_DEFENDER_OWNER, ModeKind, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_publication::{
        PublicationParts, SpawnActor, StagedUnit, prepare_publication, prepare_world_publication,
    },
    weather::WeatherChoice,
};
fn make() -> Match {
    let map = json!({"w":30,"h":30,"rows":vec!["..............................";30],"spawns":[{"x":3,"y":3},{"x":25,"y":25}],"points":[{"id":"front","x":15,"y":15}]});
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 731,
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
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(17),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn staged(m: &Match, id: u32, index: u32) -> StagedUnit {
    StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind("rifle").unwrap(),
        id,
        index,
        m.start.players[0].spawn,
        Some(Point { x: 17.0, z: 19.0 }),
    )
    .unwrap()
}
#[test]
fn full_batch_validation_and_abort_keep_canonical_state() {
    let mut m = make();
    let id = m.next_unit_id;
    let index = m.world.game.units.len() as u32;
    let first = staged(&m, id, index);
    let mut second = staged(&m, id + 1, index + 1);
    second.control.aim = f64::NAN;
    let before = format!(
        "{:?}{:?}{:?}",
        m.world.game.units, m.commands.units, m.start.economy
    );
    assert!(
        prepare_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id
            },
            &m.rules,
            2,
            vec![first, second]
        )
        .is_err()
    );
    assert_eq!(m.next_unit_id, id);
    assert_eq!(
        before,
        format!(
            "{:?}{:?}{:?}",
            m.world.game.units, m.commands.units, m.start.economy
        )
    );
    let first = staged(&m, id, index);
    let led = 1;
    let hp = m.world.game.units[&led].hp;
    let mut guard = prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        vec![first],
    )
    .unwrap();
    guard.stage_existing_hp(led, hp - 20.0).unwrap();
    assert_eq!(guard.existing_hp(led), Some(hp - 20.0));
    drop(guard);
    assert_eq!(m.world.game.units[&led].hp, hp);
    assert_eq!(m.next_unit_id, id);
    assert_eq!(
        before,
        format!(
            "{:?}{:?}{:?}",
            m.world.game.units, m.commands.units, m.start.economy
        )
    );
    // A restored allocator below the existing highest ID is invalid even when the
    // particular candidate ID is unused; publication must preserve insertion order.
    let gap = id - 2;
    m.next_unit_id = gap;
    m.world.game.units.remove(&gap);
    m.commands.units.remove(&gap);
    let draft = staged(&m, gap, index);
    assert!(
        prepare_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id
            },
            &m.rules,
            2,
            vec![draft]
        )
        .is_err()
    );
}
#[test]
fn named_world_actor_requires_explicit_world_policy() {
    let mut m = make();
    let id = m.next_unit_id;
    let index = m.world.game.units.len() as u32;
    let kind = m.rules.kind("rifle").unwrap();
    let spawn = m.start.players[0].spawn;
    let actor = SpawnActor::WorldLocalDefender { spawn };
    assert!(
        StagedUnit::ordinary_with_actor(&m.world.game, &m.rules, actor, kind, id, index, None)
            .is_err()
    );
    m.world.game.mode = Some(ModeKind::World);
    let mut invalid_guard =
        StagedUnit::ordinary_with_actor(&m.world.game, &m.rules, actor, kind, id, index, None)
            .unwrap();
    invalid_guard.control.guard_home = Some(Point {
        x: f64::NAN,
        z: 0.0,
    });
    assert!(
        prepare_world_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id,
            },
            &m.rules,
            2,
            m.world.game.mode,
            vec![invalid_guard]
        )
        .is_err()
    );
    let draft =
        StagedUnit::ordinary_with_actor(&m.world.game, &m.rules, actor, kind, id, index, None)
            .unwrap();
    assert_eq!(draft.unit.owner, LOCAL_DEFENDER_OWNER);
    assert!(
        prepare_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id,
            },
            &m.rules,
            2,
            vec![draft]
        )
        .is_err()
    );
    let mut draft =
        StagedUnit::ordinary_with_actor(&m.world.game, &m.rules, actor, kind, id, index, None)
            .unwrap();
    draft.unit.owner = 17;
    assert!(
        prepare_world_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id,
            },
            &m.rules,
            2,
            Some(ModeKind::World),
            vec![draft]
        )
        .is_err()
    );
    let draft =
        StagedUnit::ordinary_with_actor(&m.world.game, &m.rules, actor, kind, id, index, None)
            .unwrap();
    assert!(
        prepare_world_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id,
            },
            &m.rules,
            2,
            None,
            vec![draft]
        )
        .is_err()
    );
    let mut draft =
        StagedUnit::ordinary_with_actor(&m.world.game, &m.rules, actor, kind, id, index, None)
            .unwrap();
    draft.control.guard_home = Some(draft.unit.at());
    let home = draft.control.guard_home;
    prepare_world_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        m.world.game.mode,
        vec![draft],
    )
    .unwrap()
    .publish();
    assert_eq!(m.world.game.team_of(&m.world.game.units[&id]), -1);
    assert_eq!(m.commands.units[&id].guard_home, home);
    assert!(
        m.world
            .game
            .allied(LOCAL_DEFENDER_OWNER, LOCAL_DEFENDER_OWNER)
    );
    assert!(!m.world.game.allied(LOCAL_DEFENDER_OWNER, 0));
    assert_eq!(m.next_unit_id, id + 1);
}
#[test]
fn ordered_spawn_batch_hp_and_disjoint_payment_publish_once() {
    let mut m = make();
    let id = m.next_unit_id;
    let index = m.world.game.units.len() as u32;
    let mut first = staged(&m, id, index);
    first.unit.hp = 20.0;
    first.unit.hold_pos = true;
    first.control.operative = true;
    let second = staged(&m, id + 1, index + 1);
    let old_hp = m.world.game.units[&1].hp;
    m.start.economy.players[0].mp = 1000.0;
    let charge = ww2_core::economy::price_of(Mode::Conquest, "rifle")
        .unwrap()
        .mp
        * 2.0;
    let mut publication = prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        vec![first, second],
    )
    .unwrap();
    publication.stage_existing_hp(1, old_hp - 20.0).unwrap();
    // The entity guard leaves wallets and the metadata owner's callback disjoint.
    m.start.economy.players[0].mp -= charge;
    let mut observed = Vec::new();
    let ids = publication.publish_with(|u, c| observed.push((u.id, c.operative)));
    assert_eq!(ids, vec![id, id + 1]);
    assert_eq!(observed, vec![(id, true), (id + 1, false)]);
    assert_eq!(m.next_unit_id, id + 2);
    assert_eq!(m.world.game.units[&1].hp, old_hp - 20.0);
    assert_eq!(m.world.game.units[&id].hp, 20.0);
    assert!(m.world.game.units[&id].hold_pos);
    assert!(m.commands.units[&id].operative);
    assert_eq!(m.world.game.units[&id].at(), Point { x: 17.0, z: 19.0 });
    assert_eq!(m.start.economy.players[0].mp, 1000.0 - charge);
    let indexed = m
        .grid
        .in_order(&m.world.game.units, (17.0, 19.0), 0.0, |id, _| {
            ids.contains(&id)
        });
    assert_eq!(indexed, ids);
}
