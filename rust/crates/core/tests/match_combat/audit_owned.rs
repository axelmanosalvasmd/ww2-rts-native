//! Supplemental owned consumers. Authored only; no Source capture or native run.
use super::*;

#[test]
fn cmb38_rifle_and_at_hit_actual_pillbox_front_and_rear_sections() {
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/match-combat.json")).unwrap();
    // These are explicit component inputs. This is not an ordinary Build or
    // firing-loop replay. Original sim.js:4264,4273 supplies the four expectations.
    for (weapon, front, rear) in [("rifle", 0.15_f64, 0.30_f64), ("at", 120.0, 240.0)] {
        for (direction, expected) in [(-1.0, front), (1.0, rear)] {
            let from = Point { x: if direction < 0.0 { 31.0 } else { 5.0 }, z: 21.0 };
            let mut m = make(&json!({"map":fixture["cases"][0]["map"],"units":[
                {"id":1,"type":weapon,"owner":0,"x":from.x,"z":from.z},
                {"id":2,"type":"pillbox","owner":1,"x":20,"z":20,"rot":0}
            ]}));
            let cells = [189, 190, 209, 210];
            for cell in cells { m.world.set_cell(cell, b'B').unwrap(); }
            {
                let box_actor = m.world.game.units.get_mut(&2).unwrap();
                box_actor.built = Some(1.0);
                box_actor.cells = Some(cells.iter().map(|cell| *cell as u32).collect());
                assert_eq!(box_actor.hp, 1500.0);
            }
            m.world.register_building_structure(2, &cells, 1500.0, 1500.0).unwrap();
            let kind = m.rules.kind(weapon).unwrap();
            let source = FlightSource { shooter:1, kind, owner:0, airborne:false };
            let impact = Impact {
                owner:0, shooter:1, weapon:WeaponDamage::from_catalog(weapon).unwrap(),
                damage_scale:None, energy:1.0, armor_piercing:false, ap_naval_only:false,
                ap_multiplier:1.5, launch_origin:from, at:Point{x:19.0,z:21.0},
                direction:Point{x:direction,z:0.0},
            };
            let before = m.world.game.units[&2].hp;
            let before_section = m.world.game.sections[&209].hp;
            let order = [1,2];
            let mut timers = Timers::default();
            let (taken, _) = DamageWorld {
                world:&mut m.world, combat:&mut m.combat, grid:&mut m.grid,
                rules:&m.rules, logistics:&m.logistics, production:&m.production,
                mode:Mode::Default, tech_enabled:false, environment:&mut m.environment,
                environment_seed:&mut m.start.environment_seed, next_id:&mut m.next_unit_id,
                unit_order:&order, wind_angle:0.0, wind_strength:0.5,
            }.local_contact(source, ContactTarget::Cell(209), impact, "cmb38-pillbox-contact", None, &mut timers).unwrap();
            let after = m.world.game.units[&2].hp;
            let section_after = m.world.game.sections[&209].hp;
            assert!((taken-expected).abs() < 1e-9, "{weapon} direction {direction}: actual section hit");
            assert!((before-after-expected).abs() < 1e-9, "canonical pillbox health");
            assert!((before_section-section_after-expected).abs() < 1e-9, "actual contacted section");
            assert!((m.combat.get(1).unwrap().xp-expected).abs() < 1e-9, "actual attacker XP credit");
            assert_eq!(m.world.game.units[&2].cells.as_ref().unwrap().len(),4);
            assert!(m.world.game.units[&2].hp > 0.0);
        }
    }
}
