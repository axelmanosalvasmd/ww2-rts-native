//! Private, versioned native checkpoints. No transport or filesystem ownership.
use crate::match_runtime::Match;
use serde_json::Value;

pub(crate) type StaticStr = &'static str;

pub(crate) fn compiled_rules() -> crate::rules::Rules {
    crate::rules::Rules::compiled().expect("compiled rules")
}

pub(crate) fn sorted_hash_map<S, K, V>(
    map: &std::collections::HashMap<K, V>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    K: serde::Serialize + Ord + std::hash::Hash,
    V: serde::Serialize,
{
    serde::Serialize::serialize(
        &map.iter().collect::<std::collections::BTreeMap<_, _>>(),
        serializer,
    )
}
pub(crate) fn sorted_hash_set<S, T>(
    set: &std::collections::HashSet<T>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: serde::Serialize + Ord + std::hash::Hash,
{
    serde::Serialize::serialize(
        &set.iter().collect::<std::collections::BTreeSet<_>>(),
        serializer,
    )
}

impl Match {
    /// Persist output awaiting room delivery without changing producer ownership.
    pub fn save_events_json(events: &[crate::match_tick::TickEvent]) -> Result<Value, String> {
        crate::save_codec::to_value(&events)
    }

    pub fn events_from_save_json(
        value: Value,
    ) -> Result<Vec<crate::match_tick::TickEvent>, String> {
        crate::save_codec::from_value(value)
    }

    /// Save every canonical ordinary-match owner, including queues and RNG state.
    /// The outer room envelope and durable storage belong to the server.
    pub fn save_json(&self) -> Result<Value, String> {
        self.validate_save_admission()?;
        let pointers = self.save_storage_pointers();
        let mut ids = std::collections::BTreeMap::new();
        for (index, pointer) in pointers.into_iter().enumerate() {
            ids.entry(pointer).or_insert(index + 1);
        }
        Ok(crate::save_codec::normalize_pointer_fields(
            crate::save_codec::to_value(self)?,
            &ids,
        ))
    }

    /// Restore a native checkpoint and refuse states outside ordinary admission.
    pub fn from_save_json(value: Value) -> Result<Self, String> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut state: Self = crate::save_codec::from_value(value)?;
            let bindings = (1..=state.save_storage_pointers().len()).collect::<Vec<_>>();
            state.relocate_save_storage(&bindings)?;
            state.validate_save_admission()?;
            Ok(state)
        }))
        .map_err(|_| "malformed native save state".to_owned())?
    }

    fn validate_save_admission(&self) -> Result<(), String> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.check_save_scope()?;
            self.validate_tick_state()?;
            crate::orders::OrderQueue::restore(self.commands.queue.snapshot())
                .map_err(|e| format!("invalid saved command queue: {e:?}"))?;
            self.preflight_ordinary_snapshot()
                .map_err(|e| format!("snapshot admission: {e:?}"))?;
            self.ordinary_snapshot_raw_lists()?;
            Ok(())
        }))
        .map_err(|_| "malformed native save state".to_owned())?
    }

    fn check_save_scope(&self) -> Result<(), String> {
        if self.rules != compiled_rules() {
            return Err("save rules differ from the compiled rules".into());
        }
        if self.options.mode == crate::economy::Mode::World
            || self.world.game.mode == Some(crate::game::ModeKind::World)
            || self.world_runtime.is_some()
            || self.world.geography().is_some()
        {
            return Err("World save admission is unavailable".into());
        }
        Ok(())
    }
}

fn known_string(value: &str) -> Option<&'static str> {
    const RUNTIME: &[&str] = &[
        "building",
        "house",
        "bridge",
        "intact",
        "damaged",
        "burning",
        "failed",
        "victory",
        "defeat",
        "draw",
        "soil",
        "road",
        "wood",
        "timber",
        "stone",
        "concrete",
        "steel",
        "water",
        "sniper",
        "commando",
        "shell",
        "rocket",
        "nade",
        "arty",
        "bomb",
        "boom",
        "flame",
        "grenade",
        "artillery",
        "handoff",
        "timeout",
        "defense",
        "regroup-blocked",
        "withdrawn",
        "members-unavailable",
        "armor",
        "machine-gun",
        "losses",
    ];
    if let Some(found) = RUNTIME
        .iter()
        .copied()
        .find(|candidate| *candidate == value)
    {
        return Some(found);
    }
    fn find(v: &'static crate::catalog::Value, wanted: &str) -> Option<&'static str> {
        use crate::catalog::Value;
        match v {
            Value::String(s) if s == wanted => Some(s.as_str()),
            Value::Array(v) => v.iter().find_map(|v| find(v, wanted)),
            Value::Object(v) => v.iter().find_map(|(key, v)| {
                if key == wanted {
                    Some(key.as_str())
                } else {
                    find(v, wanted)
                }
            }),
            _ => None,
        }
    }
    if let Some(found) = crate::match_tutorial::LINES
        .iter()
        .flat_map(|row| row.iter())
        .copied()
        .find(|line| *line == value)
    {
        return Some(found);
    }
    crate::catalog::Catalog::compiled()
        .ok()?
        .tables()
        .iter()
        .find_map(|(k, v)| {
            if k == value {
                Some(k.as_str())
            } else {
                find(v, value)
            }
        })
}

pub(crate) fn static_string<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<&'static str, D::Error> {
    let value = <String as serde::Deserialize>::deserialize(d)?;
    known_string(&value)
        .ok_or_else(|| serde::de::Error::custom(format!("unknown compiled save string: {value}")))
}

pub(crate) fn optional_static_string<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<&'static str>, D::Error> {
    let value = <Option<String> as serde::Deserialize>::deserialize(d)?;
    value
        .map(|v| {
            known_string(&v).ok_or_else(|| {
                serde::de::Error::custom(format!("unknown compiled save string: {v}"))
            })
        })
        .transpose()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_commands::Command,
        match_runtime::{MatchOptions, PlayerConfig},
        orders::OrderKey,
        random::StreamId,
        weather::WeatherChoice,
    };
    use serde_json::json;

    pub(crate) fn ordinary() -> Match {
        let map = AuthoritativeMap::from_json(
            &serde_json::to_vec(&json!({
                "name":"Save lane", "w":64, "h":40, "rows":vec![".".repeat(64);40],
                "heights":vec!["0".repeat(64);40],
                "spawns":[{"x":8,"y":10},{"x":40,"y":32}],
                "points":[{"x":4,"y":36,"vp":0,"mp":0}]
            }))
            .unwrap(),
        )
        .unwrap();
        Match::new(
            &map,
            MatchOptions {
                seed: 701,
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
                army: ArmySize::Endless,
                weather: WeatherChoice::Map,
                weather_seed: Some(1),
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

    fn assert_checkpoint_equal(actual: &Value, expected: &Value) {
        fn difference(a: &Value, b: &Value, path: &str) -> Option<String> {
            if a == b {
                return None;
            }
            if let (Some(a), Some(b)) = (a.as_array(), b.as_array()) {
                if a.len() != b.len() {
                    return Some(format!("{path}: lengths {} != {}", a.len(), b.len()));
                }
                if a.len() == 2 && a[0] == "map" && b[0] == "map" {
                    let aa = a[1].as_array().unwrap();
                    let bb = b[1].as_array().unwrap();
                    if aa.len() != bb.len() {
                        return Some(format!("{path}: map lengths {} != {}", aa.len(), bb.len()));
                    }
                    for (index, (left, right)) in aa.iter().zip(bb).enumerate() {
                        let key = left[0][1]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("entry[{index}]"));
                        if let Some(d) =
                            difference(&left[0], &right[0], &format!("{path}.{key}.key"))
                        {
                            return Some(d);
                        }
                        if let Some(d) = difference(&left[1], &right[1], &format!("{path}.{key}")) {
                            return Some(d);
                        }
                    }
                } else {
                    for (index, (left, right)) in a.iter().zip(b).enumerate() {
                        if let Some(d) = difference(left, right, &format!("{path}[{index}]")) {
                            return Some(d);
                        }
                    }
                }
            }
            Some(format!(
                "{path}: {} != {}",
                a.to_string().chars().take(120).collect::<String>(),
                b.to_string().chars().take(120).collect::<String>()
            ))
        }
        if let Some(message) = difference(actual, expected, "Match") {
            panic!("checkpoint mismatch: {message}");
        }
    }

    #[test]
    fn native_checkpoint_restores_queued_orders_rng_and_continues_identically() {
        let mut original = ordinary();
        original
            .enqueue_command(
                OrderKey {
                    tick: 3,
                    issuer: 0,
                    sequence: 4,
                },
                Command::from_json(&json!({"t":"move","orders":[[1,30,20]],"queue":false}))
                    .unwrap(),
            )
            .unwrap();
        for _ in 0..2 {
            original.advance_tick().unwrap();
        }
        let saved = original.save_json().unwrap();
        // Cross the actual JSON text boundary. No shared allocation survives.
        let json_text = serde_json::to_vec(&saved).unwrap();
        let mut restored =
            Match::from_save_json(serde_json::from_slice(&json_text).unwrap()).unwrap();
        assert_checkpoint_equal(&restored.save_json().unwrap(), &saved);
        assert_eq!(
            restored.start.random.snapshot(),
            original.start.random.snapshot()
        );
        for _ in 0..8 {
            original.advance_tick().unwrap();
            restored.advance_tick().unwrap();
            assert_checkpoint_equal(
                &restored.save_json().unwrap(),
                &original.save_json().unwrap(),
            );
        }
        assert_eq!(
            restored.start.random.next_u64(StreamId::Combat),
            original.start.random.next_u64(StreamId::Combat)
        );
    }

    #[test]
    fn native_checkpoint_preserves_nan_payloads_and_signed_zero() {
        let mut state = ordinary();
        let sentinel = f64::from_bits(0x7ff8_0000_0000_0035);
        state.world.game.units.get_mut(&1).unwrap().pace = sentinel;
        state.world.game.units.get_mut(&1).unwrap().rot = -0.0;
        state.world.game.cell_hp[0] = f32::from_bits(0x7fc0_0035);
        let restored = Match::from_save_json(state.save_json().unwrap()).unwrap();
        assert_eq!(
            restored.world.game.units[&1].pace.to_bits(),
            sentinel.to_bits()
        );
        assert_eq!(
            restored.world.game.units[&1].rot.to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(restored.world.game.cell_hp[0].to_bits(), 0x7fc0_0035);
    }

    #[test]
    fn native_checkpoint_rejects_malformed_tree() {
        assert!(Match::from_save_json(json!(["map", []])).is_err());
        assert!(Match::from_save_json(Value::Null).is_err());
    }
    #[test]
    fn native_checkpoint_missing_elimination_defaults_and_present_owner_keeps_float_bits() {
        let state = ordinary();
        let mut legacy = state.save_json().unwrap();
        legacy[1]
            .as_array_mut()
            .unwrap()
            .retain(|entry| entry[0] != json!(["string", "elimination"]));
        let restored = Match::from_save_json(legacy).unwrap();
        assert!(restored.elimination.is_none());
        let elimination = crate::elimination_modes::EliminationState {
            teams: 2,
            bunkers: vec![1, 2],
            time_left: Some(f64::from_bits(0x3ff0000000000001)),
            sudden_death: true,
            nodes: vec![crate::match_world::ResourceNode {
                cell: 4,
                at: crate::game::Point { x: -0.0, z: 8.0 },
                depot: Some(5),
                rate: f64::from_bits(0x4000000000000001),
                fuel: true,
            }],
        };
        let encoded = crate::save_codec::to_value(&elimination).unwrap();
        let decoded: crate::elimination_modes::EliminationState = crate::save_codec::from_value(
            serde_json::from_slice(&serde_json::to_vec(&encoded).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(decoded, elimination);
        assert_eq!(
            decoded.time_left.unwrap().to_bits(),
            elimination.time_left.unwrap().to_bits()
        );
        assert_eq!(decoded.nodes[0].at.x.to_bits(), (-0.0f64).to_bits());
        assert_eq!(
            decoded.nodes[0].rate.to_bits(),
            elimination.nodes[0].rate.to_bits()
        );
    }
    #[test]
    fn native_checkpoint_refuses_changed_compiled_rules() {
        let mut state = ordinary();
        state.rules.kinds[0].vision += 1.0;
        assert!(state.save_json().unwrap_err().contains("compiled rules"));
    }
}
