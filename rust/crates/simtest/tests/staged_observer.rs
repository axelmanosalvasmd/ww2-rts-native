#[path = "common/staged_observer.rs"]
mod common;
use common::*;
use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    economy::Mode,
    fog::Fog,
    game::{Point, Wreck},
    match_routing::{RecipientContext, prepare_recipient_routing},
    match_runtime::Match,
    match_world_memory::{WorldRoutingContext, prepare_world_routing},
    navigation::TerrainView,
    pathfinding::{PathRules, Pathfinding},
    produced_units::{ProduceInput, ProducedPlacer, ProductionBuilding},
    route_knowledge::{LiveRouting, RouteQuery},
    staged_observer::StagedObserver,
    world::Recipient,
};
fn context(m: &Match) -> RecipientContext<'_> {
    RecipientContext {
        world: &m.world,
        environment: &m.environment,
        rules: &m.rules,
        building_cells: &m.building_cells,
        start: &m.start,
        options: &m.options,
    }
}
fn world_context(m: &Match) -> WorldRoutingContext<'_> {
    let w = m.world_runtime.as_ref().unwrap();
    WorldRoutingContext {
        world: &m.world,
        environment: &m.environment,
        rules: &m.rules,
        start: &m.start,
        rails: &w.rails,
        regions: &w.regions,
        known_buildings: &w.known_buildings,
        local_defender: &w.local_defender,
    }
}
fn placement(m: &Match, c: &Value) -> ww2_core::produced_units::ProducedPlacement {
    let s = &c["setup"];
    let g = &m.world.game;
    ProducedPlacer::new()
        .produce(
            &m.rules,
            Catalog::compiled().unwrap(),
            TerrainView {
                width: g.w,
                height: g.h,
                flags: &g.flags,
                heights: g.height.as_deref(),
                world_known: false,
                knowledge_id: 0,
                versions: Default::default(),
                changed: None,
                changes: &[],
            },
            &g.units,
            ProduceInput {
                id: idx(&c["id"]) as u32,
                owner: 0,
                kind: m.rules.kind(c["buy"]["type"].as_str().unwrap()).unwrap(),
                scatter_index: idx(&s["scatterIndex"]) as u32,
                player_spawn: m.start.players[0].spawn,
                player_rally: None,
                building: ProductionBuilding {
                    at: point(&s["building"]["at"]),
                    kind: m
                        .rules
                        .kind(s["building"]["kind"].as_str().unwrap())
                        .unwrap(),
                    rally: Some(point(&s["building"]["rally"])),
                },
                mode: if c["mode"] == "world" {
                    Mode::World
                } else {
                    Mode::Conquest
                },
                tick: g.tick as u64,
            },
        )
        .unwrap()
}
fn route(
    paths: &mut Pathfinding,
    q: RouteQuery<'_>,
    live: LiveRouting<'_>,
    m: &Match,
    u: &ww2_core::game::Unit,
    to: Point,
) -> Value {
    let r = paths
        .find_path(
            q.path_terrain(live).unwrap(),
            PathRules::compiled().unwrap(),
            &m.rules.kinds[u.kind],
            u.at(),
            to,
            None,
        )
        .unwrap();
    json!({"path":r.waypoints.iter().map(|p|[bits(p.x),bits(p.z)]).collect::<Vec<_>>(),"routeEnd":[bits(r.route_end.x),bits(r.route_end.z)]})
}
fn fog(f: &Fog, team: i32) -> Value {
    f.teams.get(&team).map_or(
        Value::Null,
        |f| json!({"key":f.key,"version":f.version,"vis":runs(f.vis.iter().map(|v|json!(v)))}),
    )
}

fn staged_birth(
    m: &Match,
    born: &ww2_core::produced_units::ProducedPlacement,
    path: &Value,
) -> ww2_core::unit_publication::StagedUnit {
    let mut unit = born.unit.clone();
    let value = |v: &Value| f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap());
    unit.path = path["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| Point {
            x: value(&p[0]),
            z: value(&p[1]),
        })
        .collect();
    let mut control = ww2_core::match_commands::UnitControl::new_for_mode(
        &unit,
        &m.rules,
        if m.world_runtime.is_some() {
            Mode::World
        } else {
            Mode::Conquest
        },
    )
    .unwrap();
    control.aim = born.aim;
    control.route_end = Some(Point {
        x: value(&path["routeEnd"][0]),
        z: value(&path["routeEnd"][1]),
    });
    control.rally_drift = true;
    ww2_core::unit_publication::StagedUnit {
        unit,
        control,
        recruitment: None,
    }
}
#[test]
fn original_produce_unit_sees_birth_only_at_source_lazy_cache_boundaries() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/staged-observer.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    for c in fixture["cases"].as_array().unwrap() {
        let mut m = load(c);
        setup(&mut m, c);
        let label = c["name"].as_str().unwrap();
        let warm = c["setup"]["warm"].as_str().unwrap();
        let mut paths = Pathfinding::new();
        if c["mode"] == "world" {
            let mut memory = std::mem::take(&mut m.world_runtime.as_mut().unwrap().memory);
            for h in c["setup"]["peerReads"].as_array().unwrap() {
                if !h["change"].is_null() {
                    m.world
                        .set_cell(
                            idx(&h["change"]["c"]),
                            h["change"]["ch"].as_str().unwrap().as_bytes()[0],
                        )
                        .unwrap();
                }
                let p = prepare_world_routing(world_context(&m), &mut memory, 1, None).unwrap();
                assert_eq!(
                    view(p.knowledge(), p.query(), p.live(), p.wrecks()),
                    h["view"],
                    "peer view {label}"
                );
                let mut fork = p.fork_paths(&paths);
                let peer = &m.world.game.units[&(idx(&c["setup"]["peer"]["id"]) as u32)];
                let found = route(&mut fork, p.query(), p.live(), &m, peer, point(&h["goal"]));
                assert_eq!(found["path"], h["path"], "peer path {label}");
                assert_eq!(found["routeEnd"], h["routeEnd"], "peer end {label}");
                let (_, commit) = p.prepare_publication(&mut m.world, &m.environment).unwrap();
                commit.commit();
                paths = fork;
            }

            if warm != "cold" {
                let p = prepare_world_routing(world_context(&m), &mut memory, 0, None).unwrap();
                if !c["prior"].is_null() {
                    assert_eq!(
                        view(p.knowledge(), p.query(), p.live(), p.wrecks()),
                        c["prior"],
                        "prior {label}"
                    );
                }
                let (_, commit) = p.prepare_publication(&mut m.world, &m.environment).unwrap();
                commit.commit();
            }
            if c["setup"]["advance"] == true {
                m.world.game.tick += 1;
            }
            m.world.fires.insert(idx(&c["setup"]["fireCell"]), 20.0);
            let born = placement(&m, c);
            let observer =
                StagedObserver::new(&m.world.game, &m.rules, born.unit.id, 0, &born.unit).unwrap();
            if !c["setup"]["peer"].is_null() {
                let team = m.world.game.players[1].team;
                let before = memory
                    .team(team)
                    .unwrap()
                    .query()
                    .navigation()
                    .flags
                    .as_ptr();
                let p = prepare_world_routing(world_context(&m), &mut memory, 0, Some(observer))
                    .unwrap();
                let mut fork = p.fork_paths(&paths);
                route(
                    &mut fork,
                    p.query(),
                    p.live(),
                    &m,
                    &born.unit,
                    born.rally.unwrap().destination,
                );
                drop(p);
                drop(fork);
                assert_eq!(
                    memory
                        .team(team)
                        .unwrap()
                        .query()
                        .navigation()
                        .flags
                        .as_ptr(),
                    before
                );
                let peer = &m.world.game.units[&(idx(&c["setup"]["peer"]["id"]) as u32)];
                let found = route(
                    &mut paths,
                    memory.team(team).unwrap().query(),
                    memory.live_for(1).unwrap().live(),
                    &m,
                    peer,
                    point(&c["setup"]["peerReads"].as_array().unwrap().last().unwrap()["goal"]),
                );
                assert_eq!(
                    found["path"],
                    c["setup"]["peerReads"].as_array().unwrap().last().unwrap()["path"],
                    "peer route after dropped preview"
                );
            }
            let p =
                prepare_world_routing(world_context(&m), &mut memory, 0, Some(observer)).unwrap();
            assert_eq!(
                view(p.knowledge(), p.query(), p.live(), p.wrecks()),
                c["view"],
                "birth {label}"
            );
            let mut fork = p.fork_paths(&paths);
            let path = route(
                &mut fork,
                p.query(),
                p.live(),
                &m,
                &born.unit,
                born.rally.unwrap().destination,
            );
            assert_eq!(path["path"], c["expected"]["path"], "path {label}");
            assert_eq!(path["routeEnd"], c["expected"]["routeEnd"], "end {label}");
            let staged = staged_birth(&m, &born, &path);
            let (game, commit) = p.prepare_publication(&mut m.world, &m.environment).unwrap();
            let mut controls = std::collections::BTreeMap::new();
            let mut recruitment = std::collections::BTreeMap::new();
            let players = game.players.len();
            let mode = game.mode;
            let parts = ww2_core::unit_publication::PublicationParts {
                units: &mut game.units,
                grid: &mut m.grid,
                controls: &mut controls,
                recruitment: &mut recruitment,
                next_id: &mut m.next_unit_id,
            };
            let publication = if mode.is_some() {
                ww2_core::unit_publication::prepare_world_publication(
                    parts,
                    &m.rules,
                    players,
                    mode,
                    vec![staged],
                )
            } else {
                ww2_core::unit_publication::prepare_publication(
                    parts,
                    &m.rules,
                    players,
                    vec![staged],
                )
            }
            .unwrap();
            commit.commit();
            paths = fork;
            assert_eq!(publication.publish(), vec![born.unit.id]);
            let rows: Vec<_> = memory
                .team(game.players[0].team)
                .unwrap()
                .rows()
                .iter()
                .map(|r| {
                    let r = &r.observed.row;
                    json!([r.cell, char::from(r.ch).to_string(), r.height, r.state])
                })
                .collect();
            assert_eq!(json!(rows), c["expected"]["rows"], "rows {label}");
            let team = memory.team(m.world.game.players[0].team).unwrap();
            let live = memory.live_for(0).unwrap();
            let found = route(
                &mut paths,
                team.query(),
                live.live(),
                &m,
                &m.world.game.units[&(idx(&c["id"]) as u32)],
                point(&c["expected"]["worldGoal"]),
            );
            assert_eq!(
                found["path"], c["expected"]["path"],
                "committed World reroute {label}"
            );
            if !c["setup"]["peer"].is_null() {
                let team = memory.team(m.world.game.players[1].team).unwrap();
                let live = memory.live_for(1).unwrap();
                assert_eq!(
                    view(team.knowledge(), team.query(), live.live(), live.wrecks()),
                    c["peerAfter"]["view"],
                    "peer committed view {label}"
                );
                let peer = &m.world.game.units[&(idx(&c["setup"]["peer"]["id"]) as u32)];
                let found = route(
                    &mut paths,
                    team.query(),
                    live.live(),
                    &m,
                    peer,
                    point(&c["setup"]["peerReads"].as_array().unwrap().last().unwrap()["goal"]),
                );
                assert_eq!(
                    found["path"], c["peerAfter"]["path"],
                    "peer after other-team success {label}"
                );
                assert_eq!(
                    found["routeEnd"], c["peerAfter"]["routeEnd"],
                    "peer after other-team success end {label}"
                );
            }
        } else {
            let mut state = m.recipient_routing(0).unwrap().clone();
            let mut f = Fog::default();
            for h in c["setup"]["historyReads"].as_array().unwrap() {
                if !h["change"].is_null() {
                    m.world
                        .set_cell(
                            idx(&h["change"]["c"]),
                            h["change"]["ch"].as_str().unwrap().as_bytes()[0],
                        )
                        .unwrap();
                }
                let p =
                    prepare_recipient_routing(context(&m), &mut state, &mut f, 0, None).unwrap();
                assert_eq!(
                    view(p.knowledge(), p.query(), p.live(), p.wrecks()),
                    h["view"],
                    "history view {label}"
                );
                let mut fork = p.fork_paths(&paths);
                let scout = &m.world.game.units[&(idx(&c["setup"]["scout"]["id"]) as u32)];
                let found = route(&mut fork, p.query(), p.live(), &m, scout, point(&h["goal"]));
                assert_eq!(found["path"], h["path"], "history path {label}");
                assert_eq!(found["routeEnd"], h["routeEnd"], "history end {label}");
                let (_, commit) = p.prepare_publication(&mut m.world, &m.environment).unwrap();
                commit.commit();
                paths = fork;
            }
            if warm == "route" || warm == "fire" {
                let p =
                    prepare_recipient_routing(context(&m), &mut state, &mut f, 0, None).unwrap();
                assert_eq!(
                    view(p.knowledge(), p.query(), p.live(), p.wrecks()),
                    c["prior"],
                    "prior {label}"
                );
                let (_, commit) = p.prepare_publication(&mut m.world, &m.environment).unwrap();
                commit.commit();
            }
            if warm == "fog" {
                f.team(&m.world.game, &m.rules, m.world.game.players[0].team);
            }
            if c["setup"]["advance"] == true {
                m.world.game.tick += 1;
            }
            assert_eq!(
                fog(&f, m.world.game.players[0].team),
                c["beforeFog"],
                "fog prior {label}"
            );
            m.world.fires.insert(idx(&c["setup"]["fireCell"]), 20.0);
            let born = placement(&m, c);
            let pose = json!(
                [
                    born.unit.x,
                    born.unit.z,
                    born.unit.rot,
                    born.aim,
                    born.unit.ghost
                ]
                .map(bits)
            );
            assert_eq!(pose, c["expected"]["pose"], "pose {label}");
            assert_eq!(bits(born.unit.hp), c["expected"]["hp"].as_str().unwrap());
            let observer =
                StagedObserver::new(&m.world.game, &m.rules, born.unit.id, 0, &born.unit).unwrap();
            let p = prepare_recipient_routing(context(&m), &mut state, &mut f, 0, Some(observer))
                .unwrap();
            assert_eq!(
                view(p.knowledge(), p.query(), p.live(), p.wrecks()),
                c["view"],
                "birth {label}"
            );
            let mut fork = p.fork_paths(&paths);
            let path = route(
                &mut fork,
                p.query(),
                p.live(),
                &m,
                &born.unit,
                born.rally.unwrap().destination,
            );
            assert_eq!(path["path"], c["expected"]["path"], "path {label}");
            assert_eq!(path["routeEnd"], c["expected"]["routeEnd"], "end {label}");
            let staged = staged_birth(&m, &born, &path);
            let (game, commit) = p.prepare_publication(&mut m.world, &m.environment).unwrap();
            let mut controls = std::collections::BTreeMap::new();
            let mut recruitment = std::collections::BTreeMap::new();
            let players = game.players.len();
            let mode = game.mode;
            let parts = ww2_core::unit_publication::PublicationParts {
                units: &mut game.units,
                grid: &mut m.grid,
                controls: &mut controls,
                recruitment: &mut recruitment,
                next_id: &mut m.next_unit_id,
            };
            let publication = if mode.is_some() {
                ww2_core::unit_publication::prepare_world_publication(
                    parts,
                    &m.rules,
                    players,
                    mode,
                    vec![staged],
                )
            } else {
                ww2_core::unit_publication::prepare_publication(
                    parts,
                    &m.rules,
                    players,
                    vec![staged],
                )
            }
            .unwrap();
            commit.commit();
            paths = fork;
            assert_eq!(publication.publish(), vec![born.unit.id]);
            assert_eq!(
                fog(&f, game.players[0].team),
                c["expected"]["fog"],
                "fog birth {label}"
            );
            let rows: Vec<_> = state
                .remembered()
                .iter()
                .map(|r| {
                    let r = &r.row;
                    json!([r.cell, char::from(r.ch).to_string(), r.height, r.state])
                })
                .collect();
            assert_eq!(json!(rows), c["expected"]["rows"], "rows {label}");
            let found = route(
                &mut paths,
                state.query(),
                state.live(),
                &m,
                &m.world.game.units[&(idx(&c["id"]) as u32)],
                point(&c["expected"]["worldGoal"]),
            );
            assert_eq!(
                found["path"], c["expected"]["path"],
                "committed reroute {label}"
            );
        }
    }
}

#[test]
fn aborts_keep_recipient_buffers_receipts_fog_and_prewarmed_search_history() {
    use std::collections::BTreeMap;
    use ww2_core::{
        match_commands::UnitControl,
        unit_publication::{self, PublicationParts, StagedUnit},
    };
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/staged-observer.json")).unwrap();
    let c = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "conquest:tank:route:same-tick:history")
        .unwrap();
    let mut m = load(c);
    setup(&mut m, c);
    let mut state = m.recipient_routing(0).unwrap().clone();
    let mut f = Fog::default();
    let mut paths = Pathfinding::new();
    for h in c["setup"]["historyReads"].as_array().unwrap() {
        if !h["change"].is_null() {
            m.world
                .set_cell(
                    idx(&h["change"]["c"]),
                    h["change"]["ch"].as_str().unwrap().as_bytes()[0],
                )
                .unwrap();
        }
        let p = prepare_recipient_routing(context(&m), &mut state, &mut f, 0, None).unwrap();
        let mut fork = p.fork_paths(&paths);
        let scout = &m.world.game.units[&(idx(&c["setup"]["scout"]["id"]) as u32)];
        let found = route(&mut fork, p.query(), p.live(), &m, scout, point(&h["goal"]));
        assert_eq!(found["path"], h["path"]);
        let (_, commit) = p.prepare_publication(&mut m.world, &m.environment).unwrap();
        commit.commit();
        paths = fork;
    }
    let before = view(
        state.knowledge(),
        state.query(),
        state.live(),
        state.wrecks(),
    );
    let fog_before = fog(&f, 0);
    let buffers = (
        state.query().navigation().flags.as_ptr(),
        state.query().navigation().heights.unwrap().as_ptr(),
    );
    let pending = m.world.pending(Recipient::Player(0)).cloned();
    let journal = (m.world.journal_base(), m.world.journal().to_vec());
    let born = placement(&m, c);
    let observer =
        StagedObserver::new(&m.world.game, &m.rules, born.unit.id, 0, &born.unit).unwrap();
    // A route succeeds, then actual publication validation rejects the control.
    let p = prepare_recipient_routing(context(&m), &mut state, &mut f, 0, Some(observer)).unwrap();
    let mut fork = p.fork_paths(&paths);
    route(
        &mut fork,
        p.query(),
        p.live(),
        &m,
        &born.unit,
        born.rally.unwrap().destination,
    );
    let (game, receipt) = p.prepare_publication(&mut m.world, &m.environment).unwrap();
    let mut controls = BTreeMap::new();
    let mut recruitment = BTreeMap::new();
    let mut control = UnitControl::new(&born.unit, &m.rules).unwrap();
    control.aim = f64::NAN;
    assert!(
        unit_publication::prepare_publication(
            PublicationParts {
                units: &mut game.units,
                grid: &mut m.grid,
                controls: &mut controls,
                recruitment: &mut recruitment,
                next_id: &mut m.next_unit_id
            },
            &m.rules,
            game.players.len(),
            vec![StagedUnit {
                unit: born.unit.clone(),
                control,
                recruitment: None
            }]
        )
        .is_err()
    );
    drop(receipt);
    drop(fork);
    assert_eq!(
        view(
            state.knowledge(),
            state.query(),
            state.live(),
            state.wrecks()
        ),
        before
    );
    assert_eq!(fog(&f, 0), fog_before);
    assert_eq!(
        (
            state.query().navigation().flags.as_ptr(),
            state.query().navigation().heights.unwrap().as_ptr()
        ),
        buffers
    );
    assert_eq!(m.world.pending(Recipient::Player(0)), pending.as_ref());
    assert_eq!(
        (m.world.journal_base(), m.world.journal().to_vec()),
        journal
    );
    assert!(!m.world.game.units.contains_key(&born.unit.id));
    let h = c["setup"]["historyReads"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    let scout = &m.world.game.units[&(idx(&c["setup"]["scout"]["id"]) as u32)];
    let found = route(
        &mut paths,
        state.query(),
        state.live(),
        &m,
        scout,
        point(&h["goal"]),
    );
    assert_eq!(
        found["path"], h["path"],
        "same-clock original route after rejected birth"
    );
    // Dropping a prepared receipt is equally atomic without reaching publication.
    let p = prepare_recipient_routing(context(&m), &mut state, &mut f, 0, Some(observer)).unwrap();
    drop(p);
    assert_eq!(
        (
            state.query().navigation().flags.as_ptr(),
            state.query().navigation().heights.unwrap().as_ptr()
        ),
        buffers
    );
    assert_eq!(m.world.pending(Recipient::Player(0)), pending.as_ref());
    // A changed source frame is rejected before any deferred acknowledgements.
    let p = prepare_recipient_routing(context(&m), &mut state, &mut f, 0, Some(observer)).unwrap();
    m.world.game.tick += 1;
    assert!(p.prepare_publication(&mut m.world, &m.environment).is_err());
    m.world.game.tick -= 1;
    for change in 0..3 {
        let p =
            prepare_recipient_routing(context(&m), &mut state, &mut f, 0, Some(observer)).unwrap();
        let cell = idx(&c["setup"]["fireCell"]);
        match change {
            0 => {
                m.world.fires.insert(cell, 20.0);
            }
            1 => {
                m.world.mines.insert(cell, 1);
            }
            _ => {
                m.world.mine_seen.insert(cell, 1);
            }
        }
        assert!(p.prepare_publication(&mut m.world, &m.environment).is_err());
        match change {
            0 => {
                m.world.fires.remove(&cell);
            }
            1 => {
                m.world.mines.remove(&cell);
            }
            _ => {
                m.world.mine_seen.remove(&cell);
            }
        }
        assert_eq!(m.world.pending(Recipient::Player(0)), pending.as_ref());
    }
    assert_eq!(m.world.pending(Recipient::Player(0)), pending.as_ref());
    assert_eq!(
        view(
            state.knowledge(),
            state.query(),
            state.live(),
            state.wrecks()
        ),
        before
    );
    for invalid in [0, idx(&c["setup"]["building"]["id"]) as u32] {
        let mut u = born.unit.clone();
        u.id = invalid;
        assert!(StagedObserver::new(&m.world.game, &m.rules, invalid, 0, &u).is_err());
    }
    let mut bad = born.unit.clone();
    bad.owner = 1;
    assert!(StagedObserver::new(&m.world.game, &m.rules, bad.id, 0, &bad).is_err());
    bad = born.unit.clone();
    bad.kind = m.rules.kinds.len();
    assert!(StagedObserver::new(&m.world.game, &m.rules, bad.id, 0, &bad).is_err());
    bad = born.unit.clone();
    bad.x = f64::NAN;
    assert!(StagedObserver::new(&m.world.game, &m.rules, bad.id, 0, &bad).is_err());
    let flag = m.world.game.flags.pop().unwrap();
    assert!(prepare_recipient_routing(context(&m), &mut state, &mut f, 0, Some(observer)).is_err());
    m.world.game.flags.push(flag);
    f.teams.insert(0, ww2_core::fog::TeamFog::new(1));
    assert!(prepare_recipient_routing(context(&m), &mut state, &mut f, 0, None).is_err());
    assert_eq!(f.teams[&0].vis.len(), 1);
    assert_eq!(
        (
            state.query().navigation().flags.as_ptr(),
            state.query().navigation().heights.unwrap().as_ptr()
        ),
        buffers
    );
    assert_eq!(m.world.pending(Recipient::Player(0)), pending.as_ref());
}

#[test]
fn world_preview_abort_does_not_trim_journal_or_publish_birth_memory() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/staged-observer.json")).unwrap();
    let c = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "world:rifle:cold:same-tick")
        .unwrap();
    let mut m = load(c);
    setup(&mut m, c);
    let mut memory = std::mem::take(&mut m.world_runtime.as_mut().unwrap().memory);
    for _ in 0..65_537 {
        m.world.touch(25 * m.world.game.w + 25).unwrap();
    }
    let journal = (m.world.journal_base(), m.world.journal().to_vec());
    let born = placement(&m, c);
    let observer =
        StagedObserver::new(&m.world.game, &m.rules, born.unit.id, 0, &born.unit).unwrap();
    let p = prepare_world_routing(world_context(&m), &mut memory, 0, Some(observer)).unwrap();
    let (_, receipt) = p.prepare_publication(&mut m.world, &m.environment).unwrap();
    drop(receipt);
    assert!(memory.team(m.world.game.players[0].team).is_none());
    assert!(memory.live_for(0).is_none());
    assert_eq!(
        (m.world.journal_base(), m.world.journal().to_vec()),
        journal
    );
    assert!(!m.world.game.units.contains_key(&born.unit.id));
    let p = prepare_world_routing(world_context(&m), &mut memory, 0, Some(observer)).unwrap();
    let mut fork = p.fork_paths(&Pathfinding::new());
    let path = route(
        &mut fork,
        p.query(),
        p.live(),
        &m,
        &born.unit,
        born.rally.unwrap().destination,
    );
    let staged = staged_birth(&m, &born, &path);
    let (game, receipt) = p.prepare_publication(&mut m.world, &m.environment).unwrap();
    let mut controls = std::collections::BTreeMap::new();
    let mut recruitment = std::collections::BTreeMap::new();
    let players = game.players.len();
    let mode = game.mode;
    let publication = ww2_core::unit_publication::prepare_world_publication(
        ww2_core::unit_publication::PublicationParts {
            units: &mut game.units,
            grid: &mut m.grid,
            controls: &mut controls,
            recruitment: &mut recruitment,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        players,
        mode,
        vec![staged],
    )
    .unwrap();
    receipt.commit();
    assert_eq!(publication.publish(), vec![born.unit.id]);
    assert!(m.world.journal_base() > journal.0);
    assert!(m.world.journal().len() < journal.1.len());
    assert!(memory.team(m.world.game.players[0].team).is_some());
}
