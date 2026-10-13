use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{game::Point, match_firing::AbilityBlast, random::StreamId};
#[path = "common/firing.rs"]
mod common;
use common::{bits, compare, make, snapshot};
fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/ability-flights.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn projection(m: &mut ww2_core::match_runtime::Match) -> Value {
    let mut value = snapshot(m);
    value["blasts"] = json!(
        m.firing
            .flights
            .values()
            .map(|p| {
                let s = p.blast_source.unwrap();
                json!([
                    p.id,
                    bits(s.infantry),
                    bits(s.vehicle),
                    s.terrain.map(bits),
                    bits(s.suppression),
                    (s.scar != 0.0).then(|| bits(s.scar)),
                    s.anti_garrison.map(bits)
                ])
            })
            .collect::<Vec<_>>()
    );
    value["grenades"] = json!(
        m.firing
            .grenades
            .iter()
            .map(|n| json!([
                n.flight,
                n.owner,
                bits(n.at.x),
                bits(n.at.z),
                bits(n.remaining)
            ]))
            .collect::<Vec<_>>()
    );
    value["nextFlight"] = json!(m.firing.next_flight);
    value
}
#[test]
fn actual_ability_and_fps_launches_and_blasts_match_source() {
    let fixture = fixture();
    let mut checkpoints = 0;
    for c in fixture["cases"].as_array().unwrap() {
        let mut m = make(c);
        m.world.game.units.get_mut(&1).unwrap().garrison =
            c["garrison"].as_u64().map(|v| v as usize);
        m.world.game.tick = 1;
        let kind = m.world.game.units[&1].kind;
        if c["barrage"].as_bool().unwrap() {
            let ids = m
                .launch_weapon_salvo(
                    1,
                    point(&c["at"]),
                    c["requested"].as_u64().map(|v| v as u32),
                )
                .unwrap();
            assert!(!ids.is_empty());
        } else {
            let blast = AbilityBlast::from_unit(kind, &m.rules).unwrap();
            let count = if ww2_core::catalog::Catalog::compiled()
                .unwrap()
                .unit(&m.rules.kinds[kind].name)
                .unwrap()
                .get("w")
                .is_some_and(|w| {
                    w.get("perModel")
                        .and_then(ww2_core::catalog::Value::boolean)
                        == Some(true)
                }) {
                let def = ww2_core::catalog::Catalog::compiled()
                    .unwrap()
                    .unit(&m.rules.kinds[kind].name)
                    .unwrap();
                (m.world.game.units[&1].hp / def.get("hpPer").unwrap().number().unwrap()).ceil()
                    as u32
            } else {
                1
            };
            let debit = (!c["fps"].as_bool().unwrap_or(false)).then_some(count);
            assert_eq!(
                m.launch_ability_flight(1, point(&c["at"]), blast, c["duration"].as_f64(), debit)
                    .unwrap(),
                1
            );
        }
        let mut previous = 1;
        for step in c["steps"].as_array().unwrap() {
            let tick = step["tick"].as_i64().unwrap();
            for t in previous + 1..=tick {
                m.world.game.tick = t - 1;
                m.capture_firing_prior().unwrap();
                m.world.game.tick = t;
                m.step_firing_flights().unwrap();
                m.advance_firing_grenades(0.05).unwrap();
                m.advance_firing_salvos(0.05).unwrap();
            }
            compare(
                &projection(&mut m),
                &step["expected"],
                &format!("{} {} tick{tick}", c["kind"], c["variation"]),
            );
            let state = m
                .start
                .random
                .snapshot()
                .streams
                .into_iter()
                .find(|(id, _)| *id == StreamId::Combat);
            if let Some((_, state)) = state {
                assert_eq!(format!("{state:016x}"), step["random"].as_str().unwrap());
            } else {
                assert!(!c["barrage"].as_bool().unwrap());
            }
            previous = tick;
            checkpoints += 1;
        }
    }
    assert_eq!(checkpoints, 210);
}
#[test]
fn invalid_launches_are_atomic_and_only_success_returns_a_flight() {
    let f = fixture();
    let c = &f["cases"][0];
    let mut m = make(c);
    m.world.game.tick = 1;
    let blast = AbilityBlast::from_unit(m.world.game.units[&1].kind, &m.rules).unwrap();
    let firing = m.firing.clone();
    let random = m.start.random.snapshot();
    let ammo = m.logistics.clone();
    for (at, duration, debit) in [
        (
            Point {
                x: f64::NAN,
                z: 15.0,
            },
            None,
            None,
        ),
        (point(&c["at"]), Some(-1.0), Some(1)),
        (point(&c["at"]), None, Some(0)),
    ] {
        assert!(
            m.launch_ability_flight(1, at, blast, duration, debit)
                .is_err()
        );
        assert_eq!(m.firing, firing);
        assert_eq!(m.start.random.snapshot(), random);
        assert_eq!(m.logistics, ammo);
    }
    m.firing.next_flight = u64::MAX;
    let before = m.firing.clone();
    assert!(
        m.launch_ability_flight(1, point(&c["at"]), blast, None, Some(1))
            .is_err()
    );
    assert_eq!(m.firing, before);
    m.firing.next_flight = 1;
    let id = m
        .launch_ability_flight(1, point(&c["at"]), blast, None, Some(1))
        .unwrap();
    assert!(m.firing.flights.contains_key(&id));
    assert_eq!(m.firing.grenades[0].flight, id);
    assert_eq!(m.start.random.snapshot(), random);
    let before = m.firing.clone();
    m.firing.next_flight = id;
    assert!(
        m.launch_ability_flight(1, point(&c["at"]), blast, None, None)
            .is_err()
    );
    m.firing.next_flight = before.next_flight;
    assert_eq!(m.firing, before);
    m.firing.flights.remove(&id);
    m.advance_firing_grenades(0.05).unwrap();
    assert!(m.firing.grenades.is_empty());
}
