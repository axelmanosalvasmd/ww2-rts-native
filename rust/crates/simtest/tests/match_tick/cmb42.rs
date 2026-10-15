//! Named sniper and AT preferences through real prerequisites, purchases and ticks.
//! These are unrun native witnesses, not newly recorded Source replays.
use super::make;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ww2_core::{
    game::Point,
    match_commands::Command,
    match_runtime::Match,
    match_tick::{TickEvent, TickPhase, TickReport},
    orders::OrderKey,
    random::StreamId,
    recipient_snapshot::SnapshotCache,
};

fn queue(m: &mut Match, sequences: &mut [u64; 2], owner: usize, input: Value) {
    sequences[owner] += 1;
    let key = OrderKey {
        tick: u64::try_from(m.world.game.tick).unwrap() + 1,
        issuer: u32::try_from(owner).unwrap(),
        sequence: sequences[owner],
    };
    m.enqueue_command(key, Command::from_json(&input).unwrap())
        .unwrap();
}

fn command_buy(m: &mut Match, owner: usize, kind: &str, from: Option<u32>) -> u32 {
    let retained = UnchangedActors::capture(m);
    let before = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
    let wallet = m.start.economy.players[owner].mp;
    let spent = m.story[owner].mp_spent;
    let price = ww2_core::economy::price_of(m.options.mode, kind).unwrap().mp;
    let mut input = json!({"t":"buy","unit":kind});
    if let Some(producer) = from {
        input["from"] = json!(producer);
    }
    let command = Command::from_json(&input).unwrap();
    assert!(matches!(
        &command,
        Command::Buy { kind: parsed, from: producer } if parsed == kind && *producer == from
    ));
    assert_eq!(m.execute_command(owner, &command, false).unwrap(), None);
    retained.assert_retained(m);
    assert_eq!(m.world.game.units.len(), before.len() + 1);
    assert!(before.iter().all(|id| m.world.game.units.contains_key(id)));
    let born = m
        .world
        .game
        .units
        .keys()
        .filter(|id| !before.contains(*id))
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(born.len(), 1, "one actual command-published recruit");
    let id = born[0];
    let actor = &m.world.game.units[&id];
    assert_eq!(actor.owner, owner);
    assert_eq!(m.rules.kinds[actor.kind].name, kind);
    let maker = m.recruit_metadata[&id].building;
    assert!(
        before.contains(&maker),
        "producer existed before the buy command"
    );
    let producer = &m.world.game.units[&maker];
    assert_eq!(producer.owner, owner);
    assert!(producer.hp > 0.0);
    assert_eq!(producer.built, Some(1.0));
    assert!(m.production.building(maker).is_some());
    if let Some(required) = from {
        assert_eq!(maker, required, "command kept its explicit producer binding");
    }
    assert_eq!(
        m.start.economy.players[owner].mp.to_bits(),
        (wallet - price).to_bits()
    );
    assert_eq!(
        m.story[owner].mp_spent.to_bits(),
        (spent + price).to_bits()
    );
    id
}

struct UnchangedActors {
    rows: BTreeMap<u32, (usize, usize, u64)>,
    combat: Option<u64>,
}

impl UnchangedActors {
    fn capture(m: &Match) -> Self {
        Self {
            rows: m
                .world
                .game
                .units
                .iter()
                .map(|(id, actor)| (*id, (actor.kind, actor.owner, actor.hp.to_bits())))
                .collect(),
            combat: combat_state(m),
        }
    }

    fn assert_at_tick(&self, m: &Match) {
        assert_eq!(m.world.game.units.len(), self.rows.len());
        self.assert_retained(m);
    }

    fn assert_retained(&self, m: &Match) {
        for (id, expected) in &self.rows {
            let actor = &m.world.game.units[id];
            assert_eq!(
                (actor.kind, actor.owner, actor.hp.to_bits()),
                *expected,
                "actor identity/health at tick {}: {id}",
                m.world.game.tick
            );
        }
        assert_eq!(combat_state(m), self.combat, "no combat draws or RNG reset");
    }
}

fn passive_tick(m: &mut Match) -> TickReport {
    let previous = m.world.game.tick;
    let pending = m.commands.pending_commands();
    let report = m.advance_tick().unwrap();
    assert_eq!(report.tick, previous + 1);
    assert_eq!(report.commands.len(), pending, "consume every queued input");
    assert!(report.commands.iter().all(|result| result.denial.is_none()));
    assert!(report.phases.contains(&TickPhase::Firing));
    assert_eq!(
        report.phases.contains(&TickPhase::Vision),
        report.tick % 4 == 1,
        "canonical visibility phase cadence"
    );
    assert!(report.phases.contains(&TickPhase::Research));
    assert!(report.retired.is_empty(), "no staged actor may die");
    assert!(
        report
            .events
            .iter()
            .all(|event| !matches!(event, TickEvent::Firing(_)))
    );
    report
}

fn tick(m: &mut Match, unchanged: &UnchangedActors) -> TickReport {
    let report = passive_tick(m);
    unchanged.assert_at_tick(m);
    report
}

fn stage(m: &mut Match, goals: &[(u32, Point)], unchanged: &UnchangedActors) {
    let mut arrived = false;
    for _ in 0..1200 {
        tick(m, unchanged);
        if goals.iter().all(|(id, goal)| {
            let actor = &m.world.game.units[id];
            actor.path.is_empty()
                && ww2_core::math::hypot(actor.x - goal.x, actor.z - goal.z) <= 1e-6
        }) {
            arrived = true;
            break;
        }
    }
    assert!(arrived, "ordinary moves must reach every authored spot");
    // Actual stationary ticks satisfy the weapon setup and visibility cadence.
    for _ in 0..20 {
        tick(m, unchanged);
    }
}

fn owner_row(m: &Match, owner: usize, id: u32) -> Value {
    let projection = SnapshotCache::new(m).unwrap().recipient(owner).unwrap();
    projection.fields["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0].as_u64() == Some(u64::from(id)))
        .unwrap()
        .clone()
}

fn combat_state(m: &Match) -> Option<u64> {
    m.start
        .random
        .snapshot()
        .streams
        .into_iter()
        .find_map(|(stream, state)| (stream == StreamId::Combat).then_some(state))
}

#[test]
fn cmb42_named_sniper_prefers_mg_through_real_buy_move_tick_and_owner_view() {
    let arena: Value =
        serde_json::from_str(include_str!("../fixtures/base-pending-paratroopers.json")).unwrap();
    let mut m = make(&json!({"map":arena["map"],"army":"standard","enemy":true}));
    let original = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
    assert_eq!(original.len(), 10, "retain both complete starting armies");
    for player in &mut m.start.economy.players {
        player.mp = 5000.0;
    }
    let wallets = m
        .start
        .economy
        .players
        .iter()
        .map(|p| p.mp)
        .collect::<Vec<_>>();
    let sniper = command_buy(&mut m, 0, "sniper", None);
    let rifle = command_buy(&mut m, 1, "rifle", None);
    let mg = command_buy(&mut m, 1, "mg", None);
    let births = [sniper, rifle, mg];
    assert_eq!(births.into_iter().collect::<BTreeSet<_>>().len(), 3);
    for (id, owner, kind) in [(sniper, 0, "sniper"), (rifle, 1, "rifle"), (mg, 1, "mg")] {
        assert!(!original.contains(&id));
        let actor = &m.world.game.units[&id];
        assert_eq!(actor.owner, owner);
        assert_eq!(m.rules.kinds[actor.kind].name, kind);
        assert!(m.recruit_metadata.contains_key(&id));
    }
    let price = |kind| ww2_core::economy::price_of(m.options.mode, kind).unwrap().mp;
    assert_eq!(m.start.economy.players[0].mp, wallets[0] - price("sniper"));
    assert_eq!(
        m.start.economy.players[1].mp,
        wallets[1] - price("rifle") - price("mg")
    );
    // This is the original named test's one-time holdFire helper input.
    // Real Hold Fire would clear target selection, so it stays off for the search.
    m.firing.get_mut(sniper).unwrap().cooldown = 1e9;
    let unchanged = UnchangedActors::capture(&m);
    let mut sequences = [0, 0];
    for owner in 0..2 {
        let ids = m
            .world
            .game
            .units
            .values()
            .filter(|actor| actor.owner == owner)
            .map(|actor| actor.id)
            .collect::<Vec<_>>();
        for (key, on) in [("holdFire", true), ("holdPos", true), ("autoRetreat", false)] {
            queue(
                &mut m,
                &mut sequences,
                owner,
                json!({"t":"stance","ids":ids,"key":key,"on":on}),
            );
        }
        queue(
            &mut m,
            &mut sequences,
            owner,
            json!({"t":"autocast","ids":ids,"on":false}),
        );
    }
    tick(&mut m, &unchanged);
    assert!(m.commands.units.values().all(|control| !control.auto_cast));
    let goals = [
        (sniper, Point { x: 60.0, z: 80.0 }),
        (rifle, Point { x: 72.0, z: 80.0 }),
        (mg, Point { x: 90.0, z: 80.0 }),
    ];
    queue(
        &mut m,
        &mut sequences,
        0,
        json!({"t":"move","orders":[[sniper,60,80]]}),
    );
    queue(
        &mut m,
        &mut sequences,
        1,
        json!({"t":"move","orders":[[rifle,72,80],[mg,90,80]]}),
    );
    stage(&mut m, &goals, &unchanged);
    for (target, reach) in [(rifle, 12.0), (mg, 30.0)] {
        let shooter = &m.world.game.units[&sniper];
        let target_actor = &m.world.game.units[&target];
        let distance =
            ww2_core::math::hypot(target_actor.x - shooter.x, target_actor.z - shooter.z);
        assert!((distance - reach).abs() <= 1e-6, "named target distance");
        assert!(m.world.game.players[0].sees(target));
        assert!(m.can_shoot_target(sniper, target).unwrap());
        assert_eq!(owner_row(&m, 0, target)[0], target);
    }
    assert_eq!(m.firing.get(sniper).unwrap().target_id, 0);
    queue(
        &mut m,
        &mut sequences,
        0,
        json!({"t":"stance","ids":[sniper],"key":"holdFire","on":false}),
    );
    for _ in 0..20 {
        tick(&mut m, &unchanged);
        assert_eq!(m.commands.units[&sniper].attack_id, 0, "no ordered target");
        assert!(m.world.game.players[0].sees(rifle));
        assert!(m.world.game.players[0].sees(mg));
        assert!(m.can_shoot_target(sniper, rifle).unwrap());
        assert!(m.can_shoot_target(sniper, mg).unwrap());
        assert_eq!(
            m.firing.get(sniper).unwrap().target_id,
            mg,
            "original named sniper choice"
        );
        assert_eq!(
            owner_row(&m, 0, sniper)[9],
            mg,
            "owner receives the visible chosen target"
        );
        assert!(
            m.world.game.units[&rifle].hp > 0.0,
            "original named rifle survival"
        );
    }
    // Moving the same MG past the role boundary must let the nearer rifle win.
    // The real held tick clears the previous target before the next search.
    queue(
        &mut m,
        &mut sequences,
        0,
        json!({"t":"stance","ids":[sniper],"key":"holdFire","on":true}),
    );
    queue(
        &mut m,
        &mut sequences,
        1,
        json!({"t":"move","orders":[[mg,100,80]]}),
    );
    stage(&mut m, &[(mg, Point { x: 100.0, z: 80.0 })], &unchanged);
    assert!(m.can_shoot_target(sniper, mg).unwrap());
    assert_eq!(m.firing.get(sniper).unwrap().target_id, 0);
    queue(
        &mut m,
        &mut sequences,
        0,
        json!({"t":"stance","ids":[sniper],"key":"holdFire","on":false}),
    );
    for _ in 0..20 {
        tick(&mut m, &unchanged);
        assert_eq!(m.commands.units[&sniper].attack_id, 0, "no ordered target");
        assert!(m.world.game.players[0].sees(rifle));
        assert!(m.world.game.players[0].sees(mg));
        assert!(m.can_shoot_target(sniper, rifle).unwrap());
        assert!(m.can_shoot_target(sniper, mg).unwrap());
        assert_eq!(
            m.firing.get(sniper).unwrap().target_id,
            rifle,
            "MG role cannot override every distance"
        );
        assert_eq!(owner_row(&m, 0, sniper)[9], rifle);
    }
    // Owner 1 moves its own MG away. No visibility, pose or opponent view is forged.
    queue(
        &mut m,
        &mut sequences,
        0,
        json!({"t":"stance","ids":[sniper],"key":"holdFire","on":true}),
    );
    queue(
        &mut m,
        &mut sequences,
        1,
        json!({"t":"move","orders":[[mg,120,80]]}),
    );
    stage(&mut m, &[(mg, Point { x: 120.0, z: 80.0 })], &unchanged);
    assert!(!m.world.game.players[0].sees(mg));
    assert!(!m.can_shoot_target(sniper, mg).unwrap());
    let projection = SnapshotCache::new(&m).unwrap().recipient(0).unwrap();
    assert!(
        projection.fields["units"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row[0].as_u64() != Some(u64::from(mg)))
    );
    assert_eq!(owner_row(&m, 0, sniper)[9], 0);
    unchanged.assert_at_tick(&m);
    assert_eq!(m.world.game.units.len(), original.len() + births.len());
    assert!(original.iter().all(|id| m.world.game.units.contains_key(id)));
}

#[test]
fn cmb42_named_at_prefers_tank_after_real_research_construction_and_purchases() {
    use ww2_core::economy::{Technology, price_of, tech_cost};
    let arena: Value =
        serde_json::from_str(include_str!("../fixtures/base-pending-paratroopers.json")).unwrap();
    let mut m = make(&json!({"map":arena["map"],"army":"standard","enemy":true}));
    let original = UnchangedActors::capture(&m);
    assert_eq!(original.rows.len(), 10);
    let mut sequences = [0, 0];
    for owner in 0..2 {
        m.start.economy.players[owner].mp = 5000.0;
        let ids = m
            .world
            .game
            .units
            .values()
            .filter(|actor| actor.owner == owner)
            .map(|actor| actor.id)
            .collect::<Vec<_>>();
        for (key, on) in [("holdFire", true), ("holdPos", true), ("autoRetreat", false)] {
            queue(
                &mut m,
                &mut sequences,
                owner,
                json!({"t":"stance","ids":ids,"key":key,"on":on}),
            );
        }
        queue(
            &mut m,
            &mut sequences,
            owner,
            json!({"t":"autocast","ids":ids,"on":false}),
        );
    }
    tick(&mut m, &original);
    let research_spent = [m.story[0].mp_spent, m.story[1].mp_spent];
    let mut research_prices = [0.0; 2];
    for owner in 0..2 {
        let research = m.production.player(owner).unwrap();
        assert_eq!(research.tech().tier(), 1);
        assert_eq!(research.jobs().len(), 0);
        research_prices[owner] = tech_cost(research.tech(), Technology::Tier, m.options.mode)
            .unwrap()
            .unwrap()
            .mp;
        queue(
            &mut m,
            &mut sequences,
            owner,
            json!({"t":"tech","kind":"tier"}),
        );
    }
    let mut researched = false;
    for _ in 0..1600 {
        tick(&mut m, &original);
        if (0..2).all(|owner| m.production.player(owner).unwrap().tech().tier() == 2) {
            researched = true;
            break;
        }
    }
    assert!(
        researched,
        "ordinary HQ research must finish without timer edits"
    );
    for owner in 0..2 {
        assert_eq!(m.production.player(owner).unwrap().jobs().len(), 0);
        assert_eq!(
            m.story[owner].mp_spent - research_spent[owner],
            research_prices[owner]
        );
    }

    let build_spent = [m.story[0].mp_spent, m.story[1].mp_spent];
    let site_cost = price_of(m.options.mode, "motorpool").unwrap().mp;
    for owner in 0..2 {
        let builder = m
            .world
            .game
            .units
            .values()
            .find(|actor| actor.owner == owner && m.rules.kinds[actor.kind].name == "rifle")
            .unwrap()
            .id;
        let spawn = m.start.players[owner].spawn;
        let at = Point {
            x: spawn.x,
            z: spawn.z + 16.0,
        };
        assert!(
            m.recipient_sees(owner, at).unwrap(),
            "own visible build site"
        );
        queue(
            &mut m,
            &mut sequences,
            owner,
            json!({"t":"stance","ids":[builder],"key":"holdPos","on":false}),
        );
        queue(
            &mut m,
            &mut sequences,
            owner,
            json!({"t":"build","ids":[builder],"kind":"motorpool","x":at.x,"z":at.z}),
        );
    }
    passive_tick(&mut m);
    original.assert_retained(&m);
    let mut pools = [0; 2];
    for owner in 0..2 {
        let sites = m
            .world
            .game
            .units
            .values()
            .filter(|actor| actor.owner == owner && m.rules.kinds[actor.kind].name == "motorpool")
            .collect::<Vec<_>>();
        assert_eq!(sites.len(), 1, "one real published motor pool per owner");
        pools[owner] = sites[0].id;
        assert!(!original.rows.contains_key(&pools[owner]));
        assert!(sites[0].built.is_some_and(|built| (0.0..1.0).contains(&built)));
        assert_eq!(m.story[owner].mp_spent - build_spent[owner], site_cost);
    }
    let mut completed = false;
    for _ in 0..2400 {
        passive_tick(&mut m);
        original.assert_retained(&m);
        assert_eq!(m.world.game.units.len(), original.rows.len() + 2);
        if pools.iter().all(|id| m.world.game.units[id].built == Some(1.0)) {
            completed = true;
            break;
        }
    }
    assert!(completed, "ordinary builders must complete both paid sites");
    // The provider clears completed worker activities on their next actual turn.
    passive_tick(&mut m);
    original.assert_retained(&m);
    let wallets = [m.start.economy.players[0].mp, m.start.economy.players[1].mp];
    let at = command_buy(&mut m, 0, "at", Some(pools[0]));
    let rifle = command_buy(&mut m, 1, "rifle", None);
    let tank = command_buy(&mut m, 1, "tank", Some(pools[1]));
    assert_eq!(
        [at, rifle, tank].into_iter().collect::<BTreeSet<_>>().len(),
        3
    );
    for (id, owner, kind) in [(at, 0, "at"), (rifle, 1, "rifle"), (tank, 1, "tank")] {
        assert!(!original.rows.contains_key(&id));
        let actor = &m.world.game.units[&id];
        assert_eq!(actor.owner, owner);
        assert_eq!(m.rules.kinds[actor.kind].name, kind);
        assert!(m.recruit_metadata.contains_key(&id));
    }
    assert_eq!(m.recruit_metadata[&at].building, pools[0]);
    assert_eq!(m.recruit_metadata[&tank].building, pools[1]);
    assert_eq!(
        m.start.economy.players[0].mp,
        wallets[0] - price_of(m.options.mode, "at").unwrap().mp
    );
    assert_eq!(
        m.start.economy.players[1].mp,
        wallets[1] - price_of(m.options.mode, "rifle").unwrap().mp
            - price_of(m.options.mode, "tank").unwrap().mp
    );
    // Preserve the named Source helper's single initial cooldown input.
    m.firing.get_mut(at).unwrap().cooldown = 1e9;
    let unchanged = UnchangedActors::capture(&m);
    for owner in 0..2 {
        let ids = m
            .world
            .game
            .units
            .values()
            .filter(|actor| actor.owner == owner)
            .map(|actor| actor.id)
            .collect::<Vec<_>>();
        for (key, on) in [("holdFire", true), ("holdPos", true), ("autoRetreat", false)] {
            queue(
                &mut m,
                &mut sequences,
                owner,
                json!({"t":"stance","ids":ids,"key":key,"on":on}),
            );
        }
        queue(
            &mut m,
            &mut sequences,
            owner,
            json!({"t":"autocast","ids":ids,"on":false}),
        );
    }
    tick(&mut m, &unchanged);
    queue(
        &mut m,
        &mut sequences,
        0,
        json!({"t":"move","orders":[[at,60,80]]}),
    );
    queue(
        &mut m,
        &mut sequences,
        1,
        json!({"t":"move","orders":[[rifle,67,80]]}),
    );
    queue(
        &mut m,
        &mut sequences,
        1,
        json!({"t":"move","orders":[[tank,88,80]],"face":ww2_core::math::atan2(0.0,-1.0)}),
    );
    stage(
        &mut m,
        &[
            (at, Point { x: 60.0, z: 80.0 }),
            (rifle, Point { x: 67.0, z: 80.0 }),
            (tank, Point { x: 88.0, z: 80.0 }),
        ],
        &unchanged,
    );
    for _ in 0..20 {
        tick(&mut m, &unchanged);
    }
    assert!(m.world.game.units[&at].still.unwrap() >= 2.0);
    for (target, reach) in [(rifle, 7.0), (tank, 28.0)] {
        let shooter = &m.world.game.units[&at];
        let victim = &m.world.game.units[&target];
        assert!(
            (ww2_core::math::hypot(shooter.x - victim.x, shooter.z - victim.z) - reach).abs()
                <= 1e-6
        );
        assert!(m.world.game.players[0].sees(target));
        assert!(m.can_shoot_target(at, target).unwrap());
    }
    let shooter = &m.world.game.units[&at];
    let vehicle = &m.world.game.units[&tank];
    assert!(
        ww2_core::math::cos(
            ww2_core::math::atan2(shooter.z - vehicle.z, shooter.x - vehicle.x) - vehicle.rot
        ) > 0.5,
        "tank presents its actual front through ordinary facing Move"
    );
    queue(
        &mut m,
        &mut sequences,
        0,
        json!({"t":"stance","ids":[at],"key":"holdFire","on":false}),
    );
    for _ in 0..20 {
        tick(&mut m, &unchanged);
        assert_eq!(m.commands.units[&at].attack_id, 0);
        assert!(m.world.game.players[0].sees(rifle));
        assert!(m.world.game.players[0].sees(tank));
        assert!(m.can_shoot_target(at, rifle).unwrap());
        assert!(m.can_shoot_target(at, tank).unwrap());
        assert_eq!(
            m.firing.get(at).unwrap().target_id,
            tank,
            "original named AT choice"
        );
        assert_eq!(owner_row(&m, 0, at)[9], tank);
        assert!(m.world.game.units[&rifle].hp > 0.0);
    }
    assert_eq!(m.world.game.units.len(), original.rows.len() + 5);
    original.assert_retained(&m);
}
