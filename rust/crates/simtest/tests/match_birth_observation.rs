//! Actual Match publication against the shared original birth-view fixture.
//! Fog-only warm setup stays in the provider test; these consumers exercise cold,
//! prior-route and prior-fire reads through canonical Match ownership.
#[path = "common/staged_observer.rs"]
mod common;
use common::{bits, idx, load, point, setup, view};
use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    economy::Mode,
    economy_state::{ModeContext, WorldContext},
    game::LOCAL_DEFENDER_OWNER,
    match_commands::UnitControl,
    match_runtime::Match,
    production::{Context, UnitView},
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/staged-observer.json")).unwrap()
}
fn prepare(m: &mut Match, c: &Value) {
    setup(m, c);
    m.start.economy.players[0].mp = c["setup"]["mp"].as_f64().unwrap();
    for unit in m.world.game.units.values() {
        m.commands.insert(
            unit.id,
            UnitControl::new_for_mode(unit, &m.rules, m.options.mode).unwrap(),
        );
        m.combat.initialize(unit.id);
        m.firing.initialize(unit.id);
        m.abilities.initialize(unit.id);
        m.recovery.initialize(unit.id);
    }
    let building = idx(&c["setup"]["building"]["id"]) as u32;
    m.production.initialize_building(building).unwrap();
    m.building_rallies
        .insert(building, Some(point(&c["setup"]["building"]["rally"])));
}
fn ordinary_view(m: &Match) -> Value {
    let recipient = m.recipient_routing(0).unwrap();
    view(
        recipient.knowledge(),
        recipient.query(),
        recipient.live(),
        recipient.wrecks(),
    )
}
fn warm_ordinary(m: &mut Match, c: &Value) {
    for read in c["setup"]["historyReads"].as_array().unwrap() {
        if !read["change"].is_null() {
            m.world
                .set_cell(
                    idx(&read["change"]["c"]),
                    read["change"]["ch"].as_str().unwrap().as_bytes()[0],
                )
                .unwrap();
        }
        m.refresh_recipient_routing(0).unwrap();
        assert_eq!(ordinary_view(m), read["view"]);
        let scout = idx(&c["setup"]["scout"]["id"]) as u32;
        let goal = point(&read["goal"]);
        let move_command = ww2_core::match_commands::Command::from_json(
            &json!({"t":"move","ids":[scout],"x":goal.x,"z":goal.z}),
        )
        .unwrap();
        assert_eq!(m.execute_command(0, &move_command, false).unwrap(), None);
        let u = &m.world.game.units[&scout];
        assert_eq!(
            json!(
                u.path
                    .iter()
                    .map(|p| [bits(p.x), bits(p.z)])
                    .collect::<Vec<_>>()
            ),
            read["path"]
        );
        assert_eq!(
            m.commands.units[&scout]
                .route_end
                .map(|p| json!([bits(p.x), bits(p.z)])),
            Some(read["routeEnd"].clone())
        );
    }
    if !c["prior"].is_null() {
        m.refresh_recipient_routing(0).unwrap();
        assert_eq!(ordinary_view(m), c["prior"]);
    }
    if c["setup"]["advance"] == true {
        m.world.game.tick += 1;
    }
    m.world.fires.insert(idx(&c["setup"]["fireCell"]), 20.0);
}
fn assert_birth(m: &Match, c: &Value, id: u32) {
    assert_eq!(id, idx(&c["id"]) as u32);
    let u = &m.world.game.units[&id];
    let control = &m.commands.units[&id];
    assert_eq!(
        json!([u.x, u.z, u.rot, control.aim, u.ghost].map(bits)),
        c["expected"]["pose"]
    );
    assert_eq!(bits(u.hp), c["expected"]["hp"].as_str().unwrap());
    assert_eq!(
        json!(
            u.path
                .iter()
                .map(|p| [bits(p.x), bits(p.z)])
                .collect::<Vec<_>>()
        ),
        c["expected"]["path"]
    );
    assert_eq!(
        control.route_end.map(|p| json!([bits(p.x), bits(p.z)])),
        Some(c["expected"]["routeEnd"].clone())
    );
    assert_eq!(u.world_goal, Some(point(&c["expected"]["worldGoal"])));
    assert!(u.drift && control.rally_drift);
    assert_eq!(m.combat.get(id), Some(&Default::default()));
    assert_eq!(m.firing.get(id), Some(&Default::default()));
    assert_eq!(m.abilities.get(id), Some(&Default::default()));
    assert_eq!(m.recovery.get(id), Some(&Default::default()));
    assert!(
        m.grid
            .in_order(&m.world.game.units, (u.x, u.z), 1.0, |_, _| true)
            .contains(&id)
    );
}
#[test]
fn recorded_buys_publish_original_birth_observations_and_four_metadata_rows() {
    let mut count = 0;
    for c in fixture()["cases"].as_array().unwrap() {
        if c["mode"] != "conquest" || c["setup"]["warm"] == "fog" {
            continue;
        }
        let mut m = load(c);
        prepare(&mut m, c);
        warm_ordinary(&mut m, c);
        let done = m
            .recorded_buy(
                0,
                c["buy"]["type"].as_str().unwrap(),
                Some(idx(&c["buy"]["b"]) as u32),
            )
            .unwrap();
        assert_birth(&m, c, done.unit);
        assert_eq!(ordinary_view(&m), c["view"], "{}", c["name"]);
        let rows: Vec<_> = m
            .recipient_routing(0)
            .unwrap()
            .remembered()
            .iter()
            .map(|r| {
                json!([
                    r.row.cell,
                    char::from(r.row.ch).to_string(),
                    r.row.height,
                    r.row.state
                ])
            })
            .collect();
        assert_eq!(json!(rows), c["expected"]["rows"]);
        assert_eq!(
            m.start.economy.players[0].mp,
            c["buy"]["afterWallet"]["mp"].as_f64().unwrap()
        );
        assert_eq!(
            m.story[0].mp_spent,
            c["buy"]["beforeWallet"]["mp"].as_f64().unwrap() - m.start.economy.players[0].mp
        );
        count += 1;
    }
    assert!(count >= 7);
}
#[test]
fn legal_world_training_publishes_original_produce_helper_birth_views() {
    let mut count = 0;
    for c in fixture()["cases"].as_array().unwrap() {
        if c["mode"] != "world" || c["setup"]["warm"] == "fog" {
            continue;
        }
        let mut m = load(c);
        prepare(&mut m, c);
        if !c["prior"].is_null() {
            m.refresh_world_routing(0).unwrap();
        }
        m.world.fires.insert(idx(&c["setup"]["fireCell"]), 20.0);
        let builder = idx(&c["buy"]["b"]) as u32;
        let kind = c["buy"]["type"].as_str().unwrap();
        let units: Vec<_> = m
            .world
            .game
            .units
            .values()
            .map(|u| UnitView {
                id: u.id,
                owner: (u.owner != LOCAL_DEFENDER_OWNER).then_some(u.owner),
                kind: &m.rules.kinds[u.kind].name,
                hp: u.hp,
                built: u.built.unwrap_or(1.0),
            })
            .collect();
        let regions = m.world_economy_regions().unwrap();
        let context = Context {
            mode: ModeContext {
                kind: Mode::World,
                ..Default::default()
            },
            army: m.options.army,
            tech_enabled: m.options.tech,
            naval: m.world.game.naval,
            sudden_death: false,
            units: &units,
            drops: &[],
            world: Some(WorldContext { regions: &regions }),
        };
        m.production
            .queue_unit(
                &context,
                &mut m.start.economy,
                0,
                m.start.players[0].config.faction,
                kind,
                Some(builder),
            )
            .unwrap();
        let before = m.start.economy.clone();
        let dt = Catalog::compiled()
            .unwrap()
            .unit(kind)
            .unwrap()
            .get("train")
            .unwrap()
            .number()
            .unwrap();
        let done = m.step_training(builder, dt).unwrap().unwrap();
        assert_birth(&m, c, done.unit);
        assert_eq!(m.start.economy, before);
        assert!(m.production.building(builder).unwrap().jobs().is_empty());
        let runtime = m.world_runtime.as_ref().unwrap();
        let memory = runtime.memory.team(m.world.game.players[0].team).unwrap();
        let live = runtime.memory.live_for(0).unwrap();
        assert_eq!(
            view(
                memory.knowledge(),
                memory.query(),
                live.live(),
                live.wrecks()
            ),
            c["view"]
        );
        let rows: Vec<_> = memory
            .rows()
            .iter()
            .map(|r| {
                let r = &r.observed.row;
                json!([r.cell, char::from(r.ch).to_string(), r.height, r.state])
            })
            .collect();
        assert_eq!(json!(rows), c["expected"]["rows"]);
        count += 1;
    }
    assert_eq!(count, 3);
}
