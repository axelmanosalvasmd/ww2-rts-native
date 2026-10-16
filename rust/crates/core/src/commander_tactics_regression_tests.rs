use super::{
    tests::{acts, fixture, run, unit},
    *,
};
use crate::commander_assault::{Member, Operation, Phase, Role};

fn assault_input() -> crate::commander_ai::Input {
    let mut input = fixture(vec![
        unit(1, "rifle", 40.0),
        unit(2, "rifle", 44.0),
        unit(3, "mg", 38.0),
        unit(10, "hq", 10.0),
        unit(11, "barracks", 10.0),
        unit(12, "motorpool", 10.0),
        unit(13, "airfield", 10.0),
    ]);
    input.tick = 4000;
    input.points[0].owner = Some(1);
    input
}

fn assault_state(input: &crate::commander_ai::Input, phase: Phase) -> State {
    let now = input.tick as f64 / 20.0;
    let stage = Point { x: 60.0, z: 20.0 };
    let members = input.units[..3]
        .iter()
        .map(|u| Member {
            id: u.id,
            role: if u.kind == "mg" {
                Role::Support
            } else {
                Role::Line
            },
            last: u.at,
            progress_at: now,
            order: Some(stage),
            placement: Some(stage),
            acknowledged: false,
        })
        .collect();
    State {
        assault: crate::commander_assault::AssaultState {
            operation: Some(Operation {
                id: 1,
                phase,
                started: now - 12.0,
                state_at: now - 12.0,
                point: Some(0),
                at: input.points[0].at,
                stage,
                initial: 3,
                members,
                reason: None,
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn assault_phase_transition_keeps_new_build_and_assist_assignments() {
    for assist in [false, true] {
        let mut input = assault_input();
        if assist {
            input.units.iter_mut().find(|u| u.id == 10).unwrap().hp *= 0.5;
        } else {
            input.units.retain(|u| u.id != 10);
            input.manpower = 200.0;
        }
        let mut state = assault_state(&input, Phase::Assemble);
        let commands = run(input, &mut state, Level::Normal);
        let work = if assist { "assist" } else { "build" };
        assert!(acts(&commands, work, 1), "{commands:?}");
        for command in ["move", "amove", "retreat"] {
            assert!(!acts(&commands, command, 1), "{work}: {commands:?}");
        }
        let operation = state.assault.operation.as_ref().unwrap();
        assert_eq!(operation.phase, Phase::Advance);
        assert!(!operation.members.iter().any(|m| m.id == 1));
        assert!(
            acts(&commands, "amove", 2),
            "the remaining assault advances"
        );
        assert_eq!(commands.iter().filter(|c| c["t"] == work).count(), 1);
    }
}

#[test]
fn assault_ignores_cleared_contacts_but_keeps_unseen_armor_threats() {
    for (x, visible) in [(55.0, true), (100.0, false)] {
        let input = assault_input();
        let mut state = assault_state(&input, Phase::Advance);
        let mut previous = input.clone();
        previous.tick -= 40;
        let mut tank = unit(90, "tank", x);
        tank.owner = 1;
        previous.units.push(tank.clone());
        state
            .mind
            .update(&Observation::new(previous).unwrap(), &[], true);
        let sightings = [Sighting {
            id: tank.id,
            kind: tank.kind,
            owner: tank.owner,
            at: tank.at,
            time: 199.0,
            worth: 300.0,
        }];
        plan(
            &Observation::new(input).unwrap(),
            &sightings,
            &mut state,
            Level::Normal,
            &mut MatchRandom::new(19),
        )
        .unwrap();
        assert_eq!(state.cleared_sightings.contains_key(&90), visible);
        assert_eq!(
            state.assault.operation.as_ref().unwrap().phase,
            if visible {
                Phase::Advance
            } else {
                Phase::Withdraw
            }
        );
    }
}
