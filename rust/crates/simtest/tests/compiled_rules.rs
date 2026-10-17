//! The compiled constructor is independent of the fixture loader. Compare its entire typed result,
//! including source order and every floating-point bit, against the existing JavaScript recording.
use simtest::load::load_rules;
use ww2_core::rules::{
    LaneRules, MovementRules, Rules, TrafficRules, UnitKind, VehicleProfile, VisionRules,
    WeatherKind,
};

// Exhaustive destructuring makes additions to these structs require an explicit comparison here.
fn floating_bits(rules: &Rules) -> Vec<u64> {
    let Rules {
        kinds,
        vision,
        movement,
        weather,
    } = rules;
    let mut bits = Vec::new();
    for UnitKind {
        name: _,
        vision,
        lookout: _,
        camo: _,
        structure: _,
        building: _,
        infantry: _,
        hull,
        speed,
        radius,
        naval: _,
        deep: _,
        air: _,
        rail: _,
        weapon: _,
        ability_speed,
        vehicle,
    } in kinds
    {
        bits.extend([vision, hull, speed, radius].map(|n| n.to_bits()));
        bits.extend(ability_speed.iter().map(|n| n.to_bits()));
        if let Some(VehicleProfile {
            acceleration,
            reverse_acceleration,
            braking,
            reverse_speed,
            hull_turn,
            tracked: _,
        }) = vehicle
        {
            bits.extend(
                [
                    acceleration,
                    reverse_acceleration,
                    braking,
                    reverse_speed,
                    hull_turn,
                ]
                .map(|n| n.to_bits()),
            );
        }
    }
    let VisionRules {
        camo_range,
        dust_seen,
        air_see_range,
        trench_hide,
        smoke_sight,
        level_height,
        eye,
        min_level: _,
        wood_sight,
        high_ground_vision,
        garrison_vision,
        house_vision,
        rain_sight,
        recon_len,
        recon_width,
    } = vision;
    bits.extend(
        [
            camo_range,
            dust_seen,
            air_see_range,
            trench_hide,
            smoke_sight,
            level_height,
            eye,
            wood_sight,
            high_ground_vision,
            garrison_vision,
            rain_sight,
            recon_len,
            recon_width,
        ]
        .map(|n| n.to_bits()),
    );
    bits.extend(house_vision.iter().flatten().map(|n| n.to_bits()));
    let MovementRules {
        tick,
        retreat_speed,
        slope,
        ford_speed,
        wet_ford,
        wet_ground,
        wire_speed,
        trench_speed,
        trench_rally,
        trench_arc,
        wood_speed,
        wood_cover,
        road_speed,
        mud_speed,
        churn,
        reverse_speed,
        house_hp,
        terrain_hp,
        corner_bulge,
        traffic,
        lane,
    } = movement;
    bits.extend(
        [
            tick,
            retreat_speed,
            wet_ford,
            wet_ground,
            wire_speed,
            trench_speed,
            trench_rally,
            trench_arc,
            wood_speed,
            wood_cover,
            road_speed,
            churn,
            reverse_speed,
            corner_bulge,
        ]
        .map(|n| n.to_bits()),
    );
    bits.extend(
        slope
            .iter()
            .chain(ford_speed)
            .chain(mud_speed)
            .chain(house_hp)
            .map(|n| n.to_bits()),
    );
    bits.extend(terrain_hp.iter().map(|(_, n)| n.to_bits()));
    let TrafficRules {
        horizon,
        wait,
        replan,
        age,
        max_yield,
        ghost,
        ghost_time,
    } = traffic;
    bits.extend([horizon, wait, replan, age, max_yield, ghost, ghost_time].map(|n| n.to_bits()));
    let LaneRules {
        ahead,
        margin,
        hold,
    } = lane;
    bits.extend([ahead, margin, hold].map(|n| n.to_bits()));
    for WeatherKind {
        name: _,
        sight,
        rains: _,
        inf,
        veh,
    } in weather
    {
        bits.extend([sight, inf, veh].map(|n| n.to_bits()));
    }
    bits
}

#[test]
fn compiled_rules_match_every_recorded_field_and_float_bit() {
    let fixture =
        simtest::parse(include_str!("fixtures/vision.json")).expect("existing JS recording");
    let expected = load_rules(&fixture["tables"]).expect("independent fixture loader");
    let actual = Rules::compiled().expect("compiled rules");
    // This covers names, order, booleans, optional values and every container's length.
    assert_eq!(actual, expected);
    let actual_bits = floating_bits(&actual);
    let expected_bits = floating_bits(&expected);
    assert_eq!(actual_bits.len(), expected_bits.len());
    for (index, (actual, expected)) in actual_bits.iter().zip(expected_bits).enumerate() {
        assert_eq!(*actual, expected, "floating-point field {index}");
    }
}
