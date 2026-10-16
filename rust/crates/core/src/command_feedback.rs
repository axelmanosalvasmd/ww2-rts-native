//! Feedback for existing command and placement refusal codes.
//!
//! This table contains no target details and does not change command admission.

/// Sentences from the pinned prototype's client/availability.js DENY_SENTENCES.
pub const DENY_SENTENCES: &[(&str, &str)] = &[
    ("mp", "Not enough manpower"),
    ("mun", "Not enough munitions"),
    ("fuel", "Not enough fuel"),
    ("ammo", "Not enough carried ammunition"),
    ("pop", "The army is at its limit"),
    ("cooldown", "That order is still on cooldown"),
    ("unseen", "That target is not visible"),
    ("notVisible", "That spot is not visible"),
    ("blocked", "That spot is blocked or uneven"),
    ("needs", "The required unit or building is missing"),
    ("max", "That unit or position is at its limit"),
    ("suddenDeath", "Not during Sudden Death"),
    ("queueFull", "The training queue is full"),
    ("ordersFull", "This unit already has 8 queued orders"),
    ("retreating", "That squad is retreating"),
    ("noBuilders", "Select a builder squad"),
    ("noCover", "No cover within reach"),
    ("notWaiting", "That recruit is no longer waiting"),
    (
        "scenario",
        "This scenario cannot start with the selected sides, factions or mode",
    ),
    ("territory", "Build inside territory your team owns"),
    ("coast", "A Shipyard needs open water beside it"),
    ("tier", "Needs a higher HQ tier"),
    ("rail", "Too close to the railway or a station"),
    ("save", "The match could not be saved or loaded"),
    (
        "station",
        "Rail moves start within 30 m of one of your stations and go to another station on your land",
    ),
    (
        "railCut",
        "No open line: every region on the way must be yours and its bridges standing",
    ),
    (
        "airfield",
        "Paratroopers fly from one of your finished Airfields, at most 300 m away",
    ),
    ("shore", "Too far from the shore to land"),
];

/// A full queue means training for buy, and unit orders for other named commands.
pub fn denial_sentence(code: &str, command: Option<&str>) -> &'static str {
    let code = if code == "queueFull" && command.is_some_and(|c| !c.is_empty() && c != "buy") {
        "ordersFull"
    } else {
        code
    };
    DENY_SENTENCES
        .iter()
        .find_map(|&(key, sentence)| (key == code).then_some(sentence))
        .unwrap_or("That order cannot happen")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmb151_every_prototype_denial_has_the_exact_sentence() {
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/deny-sentences.json")).unwrap();
        let expected = expected["sentences"].as_object().unwrap();
        assert_eq!(DENY_SENTENCES.len(), expected.len());
        let unique: std::collections::BTreeSet<_> =
            DENY_SENTENCES.iter().map(|row| row.0).collect();
        assert_eq!(unique.len(), expected.len());
        for (code, sentence) in expected {
            assert_eq!(
                denial_sentence(code, None),
                sentence.as_str().unwrap(),
                "{code}"
            );
            if code != "queueFull" {
                assert_eq!(
                    denial_sentence(code, Some("move")),
                    sentence.as_str().unwrap(),
                    "{code}"
                );
            }
        }
        for command in [None, Some(""), Some("buy")] {
            assert_eq!(
                denial_sentence("queueFull", command),
                "The training queue is full"
            );
        }
        for command in ["move", "build", "ability", "unknown"] {
            assert_eq!(
                denial_sentence("queueFull", Some(command)),
                "This unit already has 8 queued orders"
            );
        }
        for code in ["", "unsupported", "futureReason"] {
            assert_eq!(
                denial_sentence(code, Some("move")),
                "That order cannot happen"
            );
        }
    }
}
