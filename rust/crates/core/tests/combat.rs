//! Calls to unchanged reference functions provide expected values and ordered property writes.
use serde_json::{Value, json};
use ww2_core::{
    combat::*,
    economy::{Mode, TechState},
    game::Point,
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/combat.json")).unwrap()
}
fn n(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn opt(v: &Value) -> Option<f64> {
    v.as_f64()
}
fn bit(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn b(v: &Value) -> bool {
    v.as_bool().unwrap_or(false)
}
fn id(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
fn point(v: &Value) -> Point {
    Point {
        x: n(&v["x"]),
        z: n(&v["z"]),
    }
}
fn encode(v: Value) -> Value {
    match v {
        Value::Number(n) => bit(n.as_f64().unwrap()),
        Value::Array(a) => json!(a.into_iter().map(encode).collect::<Vec<_>>()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, encode(v))).collect()),
        x => x,
    }
}
fn retreat(u: &Value) -> RetreatState {
    RetreatState {
        retreating: b(&u["retreating"]),
        forced: b(&u["logistics"]["forced"]),
        stranded: b(&u["logistics"]["stranded"]),
        move_speed: opt(&u["moveSpeed"]).unwrap_or(0.0),
        vx: opt(&u["vx"]).unwrap_or(0.0),
        vz: opt(&u["vz"]).unwrap_or(0.0),
    }
}
fn target<'a>(row: &'a Value, cargo: &'a [u32]) -> UnitCombatState<'a> {
    let u = &row["unit"];
    UnitCombatState {
        id: id(&u["id"]),
        owner: u["owner"].as_i64().unwrap() as i32,
        kind: u["type"].as_str().unwrap(),
        hp: n(&u["hp"]),
        xp: n(&u["xp"]),
        at: point(u),
        rotation: n(&u["rot"]),
        retreat: retreat(u),
        garrisoned: u["garrison"].as_i64().unwrap() >= 0,
        in_trench: b(&row["input"]["trench"]),
        sprint: n(&u["sprint"]),
        suppression: n(&u["supp"]),
        hit_by: u["hitBy"].as_u64().map(|n| n as u32),
        cargo,
    }
}
fn tech(v: &Value) -> Option<TechState> {
    (!v.is_null())
        .then(|| TechState::new(3, id(&v[0]) as u8, id(&v[1]) as u8, id(&v[2]) as u8).unwrap())
}
fn context(v: &Value) -> CombatContext {
    CombatContext {
        tick: opt(&v["tick"]).unwrap_or(200.0),
        mode: Mode::from_kind(v["mode"].as_str()),
        horde_defenders: v["defenders"].as_u64().unwrap_or(1) as u32,
        allied: b(&v["allied"]),
        tech_enabled: !v["tech"].is_null(),
        attacker_tech: tech(&v["tech"][0]),
        defender_tech: tech(&v["tech"][1]),
    }
}
fn events(events: Vec<CombatEvent>) -> Value {
    let mut out = Vec::new();
    for event in events {
        match event {
            CombatEvent::Health { unit, value } => {
                assert_eq!(unit, 2);
                out.push(json!({"kind":"hp","value":value}));
            }
            CombatEvent::DisembarkRider {
                carrier,
                rider,
                minimum_suppression,
            } => {
                assert_eq!(carrier, 2);
                assert_eq!(minimum_suppression, 60.0);
                out.push(json!({"kind":"rider","rider":rider}));
            }
            CombatEvent::LastHit { owner, .. } => out.push(json!({"kind":"lastHit","value":owner})),
            CombatEvent::DeathImpulse { value, .. } => out.push(
                json!({"kind":"impulse","value":{"dir":value.direction,"impulse":value.magnitude}}),
            ),
            CombatEvent::ResetRetarget { .. } => out.push(json!({"kind":"retarget","value":0})),
            CombatEvent::HitSource {
                shooter,
                tick,
                from,
                ..
            } => {
                out.push(json!({"kind":"hitBy","value":shooter}));
                out.push(json!({"kind":"hitAt","value":tick}));
                out.push(json!({"kind":"hitFrom","value":{"x":from.x,"z":from.z}}));
            }
            CombatEvent::AntiTankHit { tick, .. } => out.push(json!({"kind":"atHit","value":tick})),
            CombatEvent::Experience { value, .. } => out.push(json!({"kind":"xp","value":value})),
            CombatEvent::SuppressionRecord { unit } => {
                out.push(json!({"kind":"record","value":unit}))
            }
            CombatEvent::Suppression { value, .. } => {
                out.push(json!({"kind":"supp","value":value}))
            }
        }
    }
    encode(json!(out))
}
#[test]
fn role_armor_veterancy_retreat_and_threat_match_reference() {
    for row in fixture()["helper"].as_array().unwrap() {
        let a = &row["args"];
        let op = row["op"].as_str().unwrap();
        let result = match op {
            "role" => bit(role_mul(a[0].as_str().unwrap(), a[1].as_str().unwrap()).unwrap()),
            "vet" => bit(f64::from(
                veterancy(a[0].as_str().unwrap(), n(&a[1])).unwrap(),
            )),
            "bunker" => bit(bunker_mul(
                a[0].as_str().unwrap(),
                Mode::from_kind(a[1].as_str()),
                id(&a[2]),
            )
            .unwrap()),
            "retreat" => json!(retreat_protected(retreat(&a[0])).unwrap()),
            "armor" => {
                let angle = n(&a[1]);
                bit(armor_mul(
                    a[0].as_str().unwrap(),
                    Point { x: 3.0, z: 4.0 },
                    0.7,
                    Point {
                        x: 3.0 + ww2_core::math::cos(angle),
                        z: 4.0 + ww2_core::math::sin(angle),
                    },
                )
                .unwrap())
            }
            "threat" => {
                let u = &a[1];
                let t = &a[2];
                json!(
                    fired_on_by(
                        n(&a[0]),
                        ThreatState {
                            id: id(&u["id"]),
                            hit_by: Some(id(&u["hitBy"])),
                            hit_at: n(&u["hitAt"]),
                            target_id: None,
                            shot_at: None
                        },
                        ThreatState {
                            id: id(&t["id"]),
                            hit_by: None,
                            hit_at: 0.0,
                            target_id: Some(id(&t["targetId"])),
                            shot_at: opt(&t["shotAt"])
                        }
                    )
                    .unwrap()
                )
            }
            _ => panic!("unknown helper {op}"),
        };
        assert_eq!(result, row["result"], "{op} {a}");
    }
}
#[test]
fn damage_and_every_scalar_mutation_match_original_functions() {
    let f = fixture();
    for (i, row) in f["damage"].as_array().unwrap().iter().enumerate() {
        let a = &row["input"];
        let cargo: Vec<_> = row["unit"]["cargo"]
            .as_array()
            .unwrap()
            .iter()
            .map(id)
            .collect();
        let t = target(row, &cargo);
        let s = &row["source"];
        let w = &row["weapon"];
        let source = CombatSource {
            kind: s["type"].as_str().unwrap(),
            xp: n(&s["xp"]),
            airborne: !s["air"].is_null(),
        };
        let p = Impact {
            owner: a["owner"].as_i64().unwrap_or(0) as i32,
            shooter: 1,
            weapon: WeaponDamage {
                infantry: n(&w["inf"]),
                vehicle: n(&w["veh"]),
                flame: opt(&w["flame"]),
            },
            damage_scale: opt(&a["scale"]),
            energy: opt(&a["energy"]).unwrap_or(1.0),
            armor_piercing: b(&a["ap"]),
            ap_naval_only: b(&a["apNaval"]),
            ap_multiplier: opt(&a["apMultiplier"]).unwrap_or(1.5),
            launch_origin: Point { x: 20.0, z: 13.0 },
            at: Point { x: 13.0, z: 13.0 },
            direction: if a["direction"].is_null() {
                Point { x: 1.0, z: 0.0 }
            } else {
                point(&a["direction"])
            },
        };
        let result = flight_damage(context(a), p, source, &t).unwrap();
        assert_eq!(bit(result.damage), row["result"], "damage {i}: {a}");
        assert_eq!(events(result.events), row["events"], "events {i}: {a}");
    }
}
#[test]
fn suppression_combination_guards_and_write_order_match_reference() {
    let f = fixture();
    for (i, row) in f["suppression"].as_array().unwrap().iter().enumerate() {
        let a = &row["input"];
        let t = target(row, &[]);
        let p = SuppressionInput {
            owner: 0,
            shooter: 1,
            amount: opt(&a["amount"]).unwrap_or(20.0),
            scale: opt(&a["suppScale"]),
            flame: row["weapon"]["flame"].as_f64().is_some_and(|v| v != 0.0),
            launch_origin: Point { x: 20.0, z: 13.0 },
            already_suppressed: b(&a["duplicate"]),
            cover_mul: n(&row["cover"]),
            cover_behind: n(&row["behind"]),
        };
        assert_eq!(
            events(flight_suppress(context(a), p, &t).unwrap()),
            row["events"],
            "suppression {i}: {a}"
        );
    }
}
#[test]
fn rider_request_preserves_source_order_and_non_thrown_contract() {
    let f = fixture();
    let mut checked = 0;
    for row in f["damage"].as_array().unwrap() {
        if row["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["kind"] != "rider")
        {
            continue;
        }
        checked += 1;
        for rider in row["ridersAfter"].as_array().unwrap() {
            assert_eq!(rider["riding"], bit(0.0));
            let supp =
                f64::from_bits(u64::from_str_radix(rider["supp"].as_str().unwrap(), 16).unwrap());
            assert!(supp >= 60.0);
            assert_eq!(rider["orders"], encode(json!([{"t":"move","x":30,"z":30}])));
            assert_eq!(rider["path"], encode(json!([{"x":31,"z":31}])));
        }
        assert_eq!(row["targetAfter"]["cargo"], encode(json!([404])));
    }
    assert!(checked > 0);
}
#[test]
fn invalid_inputs_fail_without_partial_effects() {
    assert!(role_mul("unknown", "rifle").is_err());
    assert!(veterancy("rifle", f64::NAN).is_err());
    assert!(veterancy("rifle", -1.0).is_err());
    assert!(bunker_mul("bunker", Mode::Horde, 0).is_err());
    assert!(
        armor_mul(
            "tank",
            Point { x: 0.0, z: 0.0 },
            f64::INFINITY,
            Point { x: 1.0, z: 0.0 }
        )
        .is_err()
    );
    assert!(
        retreat_protected(RetreatState {
            vx: f64::NAN,
            ..Default::default()
        })
        .is_err()
    );
    let f = fixture();
    let row = &f["suppression"][0];
    let mut t = target(row, &[]);
    t.hp = f64::NAN;
    let p = SuppressionInput {
        owner: 0,
        shooter: 1,
        amount: 1.0,
        scale: None,
        flame: false,
        launch_origin: Point { x: 1.0, z: 0.0 },
        already_suppressed: false,
        cover_mul: 1.0,
        cover_behind: 0.0,
    };
    assert!(flight_suppress(context(&row["input"]), p, &t).is_err());
    t.hp = 100.0;
    assert!(
        flight_suppress(
            context(&row["input"]),
            SuppressionInput {
                cover_behind: 1.1,
                ..p
            },
            &t
        )
        .is_err()
    );
}

#[test]
fn weapon_catalog_and_invalid_damage_inputs_are_checked() {
    let catalog = ww2_core::catalog::Catalog::compiled().unwrap();
    for (kind, def) in catalog.table("UNITS").unwrap().object().unwrap() {
        if let Some(w) = def.get("w") {
            if w.get("inf")
                .and_then(ww2_core::catalog::Value::number)
                .is_none()
                || w.get("veh")
                    .and_then(ww2_core::catalog::Value::number)
                    .is_none()
            {
                assert!(WeaponDamage::from_catalog(kind).is_err());
                continue;
            }
            let weapon = WeaponDamage::from_catalog(kind).unwrap();
            assert_eq!(
                weapon.infantry.to_bits(),
                w.get("inf").unwrap().number().unwrap().to_bits()
            );
            assert_eq!(
                weapon.vehicle.to_bits(),
                w.get("veh").unwrap().number().unwrap().to_bits()
            );
        }
    }
    let f = fixture();
    let row = &f["damage"][0];
    let target = target(row, &[]);
    let source = CombatSource {
        kind: "tank",
        xp: 0.0,
        airborne: false,
    };
    let impact = Impact {
        owner: 0,
        shooter: 1,
        weapon: WeaponDamage::from_catalog("tank").unwrap(),
        damage_scale: None,
        energy: 1.0,
        armor_piercing: false,
        ap_naval_only: false,
        ap_multiplier: 1.5,
        launch_origin: Point { x: 1.0, z: 1.0 },
        at: Point { x: 0.0, z: 0.0 },
        direction: Point { x: 1.0, z: 0.0 },
    };
    for bad in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(
            flight_damage(
                context(&row["input"]),
                Impact {
                    energy: bad,
                    ..impact
                },
                source,
                &target
            )
            .is_err()
        );
        assert!(
            flight_damage(
                context(&row["input"]),
                Impact {
                    damage_scale: Some(bad),
                    ..impact
                },
                source,
                &target
            )
            .is_err()
        );
    }
    let before = target.hp;
    assert!(
        flight_damage(
            context(&row["input"]),
            impact,
            CombatSource {
                xp: f64::MAX,
                ..source
            },
            &target
        )
        .is_ok()
    );
    assert_eq!(target.hp, before);
}
