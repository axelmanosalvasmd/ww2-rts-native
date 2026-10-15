use super::*;
use crate::game::Point;

// Declared result-component inputs. These do not construct or restore a Match.
fn report() -> MatchResult {
    MatchResult {
        winner: 2,
        teams: vec![0, 2, 0],
        names: vec!["sim-a".into(), "sim-b".into(), "sim-c".into()],
        reason: EndReason::VictoryPoints,
        at: Point { x: 7.3, z: 21.3 },
        end_tick: 219,
        story: vec![PlayerStory::default(); 3],
        timeline: vec![MatchSample::Conquest {
            t: 11.0,
            vp: vec![4.0, 0.0, 750.0],
        }],
        win_vp: Some(750.0),
        mode: "conquest",
    }
}
fn names() -> Vec<String> {
    vec!["Alice".into(), "Bob".into(), "Cara".into()]
}

#[test]
fn mode90_typed_report_preserves_personal_history_and_rejects_incompatible_facts() {
    let completed = CompletedReport::compose(report(), &names(), false, None).unwrap();
    assert_eq!(completed.result().names, names());
    assert_eq!(completed.result().end_tick, 219);
    let mut people = vec![PersonHistory::default(); 3];
    let before = people.clone();
    assert!(completed.remember(&mut people[..2]).is_err());
    assert_eq!(people, before, "roster rejection must be atomic");
    completed.remember(&mut people).unwrap();
    let lobby = RoomReport::Completed(Box::new(completed));
    assert_eq!(
        lobby.personal_outcome(&people[1], true),
        Some(LastMatch {
            team: 2,
            outcome: Outcome::Victory
        })
    );
    assert_eq!(
        lobby.personal_outcome(&people[0], true),
        Some(LastMatch {
            team: 0,
            outcome: Outcome::Defeat
        })
    );
    let bob = people.remove(1);
    people.push(PersonHistory::default());
    assert_eq!(lobby.personal_outcome(&bob, true).unwrap().team, 2);
    assert_eq!(lobby.personal_outcome(&bob, false), None);
    assert_eq!(lobby.personal_outcome(people.last().unwrap(), true), None);
    assert_eq!(RoomReport::HostEnded.personal_outcome(&bob, true), None);
    assert_eq!(RoomReport::HostEnded.fields(), json!({"ended":true}));
    assert_eq!(
        RoomReport::HostEnded.recipient_fields(&bob, true),
        json!({"ended":true,"you":null})
    );
    assert_eq!(bob.last_match().unwrap().outcome, Outcome::Victory);
    let mut draw = report();
    draw.winner = -1;
    draw.reason = EndReason::Draw;
    CompletedReport::compose(draw, &names(), false, None)
        .unwrap()
        .remember(&mut people)
        .unwrap();
    assert!(
        people
            .iter()
            .all(|p| p.last_match().unwrap().outcome == Outcome::Draw)
    );

    let mut invalid = report();
    invalid.teams.pop();
    assert!(CompletedReport::compose(invalid, &names(), false, None).is_err());
    let mut invalid = report();
    invalid.winner = 1;
    assert!(CompletedReport::compose(invalid, &names(), false, None).is_err());
    let mut invalid = report();
    invalid.winner = -1;
    assert!(CompletedReport::compose(invalid, &names(), false, None).is_err());
    let mut invalid = report();
    invalid.story[0].kills = f64::NAN;
    assert!(CompletedReport::compose(invalid, &names(), false, None).is_err());
    let mut invalid = report();
    invalid.story[0].mp_spent = 1.25;
    assert!(CompletedReport::compose(invalid, &names(), false, None).is_err());
    let mut invalid = report();
    invalid.win_vp = None;
    assert!(CompletedReport::compose(invalid, &names(), false, None).is_err());
    let mut invalid = report();
    invalid.timeline = vec![MatchSample::Territories {
        t: 11.0,
        points: vec![2.0, 0.0, 1.0],
        structures: vec![1.0, 0.0, 0.0],
    }];
    assert!(CompletedReport::compose(invalid, &names(), false, None).is_err());
    let mut invalid = report();
    invalid.timeline = vec![MatchSample::Conquest {
        t: 11.0,
        vp: vec![750.0],
    }];
    assert!(CompletedReport::compose(invalid, &names(), false, None).is_err());
    let mut invalid = report();
    invalid.mode = "unimplemented";
    assert!(CompletedReport::compose(invalid, &names(), false, None).is_err());
}

#[test]
fn mode90_horde_record_uses_raw_human_kills_held_time_and_previous_best() {
    let names = vec!["Alice".into(), "Bob".into()];
    let story = vec![
        PlayerStory {
            kills: 2.25,
            ..PlayerStory::default()
        },
        PlayerStory {
            kills: 3.5,
            ..PlayerStory::default()
        },
        PlayerStory {
            kills: 999.0,
            ..PlayerStory::default()
        },
    ];
    let key = HordeRecordKey {
        map: "Crossroads".into(),
        humans: 2,
        army: ArmySize::Standard,
    };
    assert_eq!(key.storage_key().unwrap(), "Crossroads|2|standard");
    let mut records = HordeRecords::default();
    let first = HordeRun::from_counters(3, 339, &story, &names).unwrap();
    assert_eq!(first.kills, 5.75);
    assert_eq!(first.time, 17.0);
    let first_result = records.finish(&key, first.clone()).unwrap();
    assert!(first_result.record());
    assert_eq!(first_result.best(), None);
    let tied = records.finish(&key, first.clone()).unwrap();
    assert!(!tied.record());
    assert_eq!(tied.best(), Some(&first));
    let longer = HordeRun::from_counters(3, 360, &story, &names).unwrap();
    let improved = records.finish(&key, longer.clone()).unwrap();
    assert!(improved.record());
    assert_eq!(improved.best(), Some(&first));
    let lower = HordeRun::from_counters(2, 2000, &story, &names).unwrap();
    assert!(!records.finish(&key, lower).unwrap().record());
    let higher = HordeRun::from_counters(4, 260, &story, &names).unwrap();
    let result = records.finish(&key, higher.clone()).unwrap();
    assert!(result.record());
    assert_eq!(result.best(), Some(&longer));
    assert_eq!(records.best(&key).unwrap(), Some(&higher));
    let restored = HordeRecords::restore([(key.clone(), higher.clone())]).unwrap();
    assert_eq!(restored.best(&key).unwrap(), Some(&higher));
    assert_eq!(restored.entries().count(), 1);
    assert!(
        HordeRecords::restore([(key.clone(), higher.clone()), (key.clone(), higher.clone())])
            .is_err()
    );
    let other = HordeRecordKey {
        map: key.map.clone(),
        humans: 2,
        army: ArmySize::Large,
    };
    assert_eq!(records.best(&other).unwrap(), None);
    let wrong_team_size = HordeRecordKey {
        map: key.map.clone(),
        humans: 1,
        army: key.army,
    };
    assert!(records.finish(&wrong_team_size, higher).is_err());
    assert!(HordeRun::from_counters(3, 339, &story[..2], &names).is_err());
    let mut bad = story.clone();
    bad[0].kills = f64::INFINITY;
    assert!(HordeRun::from_counters(3, 339, &bad, &names).is_err());

    let mut facts = report();
    facts.mode = "horde";
    facts.teams = vec![0, 0, 1];
    facts.winner = 1;
    facts.reason = EndReason::Structures;
    facts.win_vp = None;
    facts.timeline = vec![MatchSample::Territories {
        t: 11.0,
        points: vec![2.0, 1.0],
        structures: vec![0.0, 0.0],
    }];
    let completed = CompletedReport::compose(facts.clone(), &names, false, Some(result)).unwrap();
    assert_eq!(completed.result().end_tick, 219);
    assert_eq!(completed.horde().unwrap().run().time, 13.0);
    let fields = completed.fields();
    assert_eq!(fields["horde"]["time"], 13.0);
    assert_eq!(fields["horde"]["best"]["time"], 18.0);
    assert!(fields["winVp"].is_null());
    assert_eq!(completed.result().names, vec!["Alice", "Bob", "sim-c"]);
    let too_short = records
        .finish(
            &key,
            HordeRun::from_counters(5, 20, &story, &names).unwrap(),
        )
        .unwrap();
    assert!(CompletedReport::compose(facts.clone(), &names, false, Some(too_short)).is_err());
    assert!(CompletedReport::compose(facts.clone(), &names, false, None).is_err());
    assert!(CompletedReport::compose(facts.clone(), &names, true, Some(first_result)).is_err());
    let sandbox = CompletedReport::compose(facts, &names, true, None).unwrap();
    assert!(sandbox.horde().is_none());
}
