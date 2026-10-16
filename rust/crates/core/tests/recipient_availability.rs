use serde_json::{Value, json};
use ww2_core::{
    client_view::ClientView,
    recipient_availability::{availability, deny_sentence},
};

fn row(id: u32, kind: &str, owner: i32) -> Value {
    json!([id, kind, owner, 10, 10, 0, 0, 100, 0, 0, 0, 0, 0, 0, 1])
}
fn snapshot() -> Value {
    json!({"t":"s","mode":{"kind":"conquest"},"mp":10000,"mun":10000,"fuel":10000,
        "units":[row(1,"rifle",0),row(2,"hq",0),row(3,"barracks",0),row(4,"motorpool",0),row(5,"armory",0)],
        "tech":{"tier":3,"armory":{"inf":0,"guns":0,"armor":0},"lab":[]}})
}
fn view(s: Value) -> ClientView {
    let mut view = ClientView::new();
    view.apply(&json!({"t":"start","map":{"w":20,"h":20}}).to_string())
        .unwrap();
    view.apply(&s.to_string()).unwrap();
    view
}
fn reason(s: Value, mut action: Value) -> String {
    action["slot"] = json!(0);
    if action.get("ids").is_none() {
        action["ids"] = json!([1]);
    }
    availability(Some(&view(s)), &action).unwrap().reason
}

#[test]
fn if03_recipient_buy_checks_mode_money_population_maximum_and_selected_producer() {
    let action = json!({"t":"buy","unit":"tank"});
    assert_eq!(reason(snapshot(), action.clone()), "");
    let mut s = snapshot();
    s["mp"] = json!(299);
    assert_eq!(reason(s, action.clone()), "Needs 300 MP");
    let mut s = snapshot();
    s["mode"]["kind"] = json!("classic");
    s["fuel"] = json!(59);
    assert_eq!(reason(s, action.clone()), "Needs 60 fuel");
    let mut s = snapshot();
    s["tech"]["tier"] = json!(1);
    assert_eq!(reason(s, action.clone()), "Needs Company HQ");
    assert_eq!(
        reason(snapshot(), json!({"t":"buy","unit":"tank","from":3})),
        "Selected building cannot produce this unit"
    );
    assert_eq!(
        reason(snapshot(), json!({"t":"buy","unit":"engineer"})),
        "Unavailable in this mode"
    );
    assert_eq!(
        reason(snapshot(), json!({"t":"buy","unit":"gunboat"})),
        "Needs a map with a sea"
    );
    let mut s = snapshot();
    s["units"].as_array_mut().unwrap().push(row(6, "tiger", 0));
    assert_eq!(
        reason(s, json!({"t":"buy","unit":"tiger"})),
        "Maximum of this unit reached"
    );
    let mut s = snapshot();
    s["world"] = json!({"cap":1});
    assert_eq!(reason(s, action), "Army at its limit (1/1)");
    // Queued population and five waiting recruits count before a Classic purchase.
    let mut s = snapshot();
    s["mode"]["kind"] = json!("classic");
    s["queues"] = json!([[4, "tank", 1, 35, "tank", "tank", "tank", "tank", "tank"]]);
    assert_eq!(
        reason(s, json!({"t":"buy","unit":"tank"})),
        "The training queue is full"
    );
}

#[test]
fn if03_recipient_horde_facilities_are_shared_only_with_non_invader_allies() {
    let mut s = snapshot();
    s["mode"] = json!({"kind":"horde","slot":2});
    s["units"] = json!([row(1, "rifle", 0), row(2, "barracks", 1)]);
    assert_eq!(
        reason(s.clone(), json!({"t":"buy","unit":"rifle","teams":[0,0,1]})),
        ""
    );
    assert_eq!(
        reason(s.clone(), json!({"t":"buy","unit":"rifle","teams":[0,1,1]})),
        "Needs a HQ"
    );
    s["mode"]["slot"] = json!(1);
    assert_eq!(
        reason(s, json!({"t":"buy","unit":"rifle","teams":[0,0]})),
        "Needs a HQ"
    );
}

#[test]
fn if03_recipient_build_research_and_waiting_cancellation_keep_denial_precedence() {
    let mut s = snapshot();
    s["units"][0][1] = json!("engineer");
    s["mode"]["kind"] = json!("classic");
    assert_eq!(
        reason(s.clone(), json!({"t":"build","kind":"airfield"})),
        ""
    );
    s["units"][0][12] = json!(1);
    assert_eq!(
        reason(s.clone(), json!({"t":"build","kind":"airfield"})),
        "That squad is retreating"
    );
    s["mode"]["suddenDeath"] = json!(true);
    assert_eq!(
        reason(s, json!({"t":"build","kind":"airfield"})),
        "Not during Sudden Death"
    );
    assert_eq!(reason(snapshot(), json!({"t":"tech","kind":"inf"})), "");
    let mut s = snapshot();
    s["tech"]["lab"] = json!([["guns", 10, 30]]);
    assert_eq!(
        reason(s.clone(), json!({"t":"tech","kind":"inf"})),
        "Each Armory researches one line at a time"
    );
    assert_eq!(
        reason(s, json!({"t":"tech","kind":"guns"})),
        "Already researching"
    );
    let mut s = snapshot();
    s["tech"]["tier"] = json!(1);
    s["tech"]["armory"]["inf"] = json!(1);
    assert_eq!(
        reason(s, json!({"t":"tech","kind":"inf"})),
        "Needs Company HQ"
    );
    let mut s = snapshot();
    s["productionJobs"] = json!([[3,[{"id":7,"status":"waiting"},{"id":8,"status":"training"}]]]);
    assert_eq!(
        reason(s.clone(), json!({"t":"cancelProduction","id":3,"job":7})),
        ""
    );
    assert_eq!(
        reason(s, json!({"t":"cancelProduction","id":3,"job":8})),
        "That recruit is no longer waiting"
    );
}

#[test]
fn if03_recipient_support_fort_and_ability_checks_use_delivered_cooldowns_and_selection() {
    let mut s = snapshot();
    s["sup"] = json!({"smoke":0.01});
    assert_eq!(
        reason(s, json!({"t":"support","kind":"smoke"})),
        "Cooldown 1 s"
    );
    let mut s = snapshot();
    s["mode"]["kind"] = json!("world");
    s["mun"] = json!(89);
    assert_eq!(
        reason(s.clone(), json!({"t":"support","kind":"para"})),
        "Needs 90 munitions"
    );
    s["mun"] = json!(90);
    assert!(
        reason(s, json!({"t":"support","kind":"para"}))
            .starts_with("Paratroopers fly from one of your finished Airfields")
    );
    let mut s = snapshot();
    s["mp"] = json!(0);
    assert_eq!(
        reason(s.clone(), json!({"t":"dig","kind":"trench","queue":true})),
        ""
    );
    assert_eq!(reason(s.clone(), json!({"t":"entrench"})), "");
    assert_eq!(
        reason(s.clone(), json!({"t":"dig","kind":"trench"})),
        "Needs 30 MP"
    );
    s["mode"]["kind"] = json!("classic");
    s["mun"] = json!(0);
    s["units"][0][11] = json!(4.1);
    assert_eq!(
        reason(s.clone(), json!({"t":"ability","unit":"rifle"})),
        "Cooldown 5 s"
    );
    assert_eq!(
        reason(
            s.clone(),
            json!({"t":"ability","unit":"rifle","queue":true})
        ),
        ""
    );
    s["units"][0][11] = json!(0);
    assert_eq!(
        reason(s, json!({"t":"ability","unit":"rifle"})),
        "Needs 15 munitions"
    );
    assert_eq!(
        reason(snapshot(), json!({"t":"stance","ids":[2]})),
        "Select a unit"
    );
    assert_eq!(
        reason(snapshot(), json!({"t":"cover","ids":[4]})),
        "Select an infantry squad"
    );
}

#[test]
fn if03_query_reads_only_delivered_rows_and_leaves_recipient_state_unchanged() {
    assert_eq!(
        availability(None, &json!({})).unwrap().reason,
        "Waiting for the army"
    );
    assert_eq!(
        reason(
            snapshot(),
            json!({"t":"buy","unit":"rifle","watching":true})
        ),
        "You are spectating"
    );
    let mut v = view(snapshot());
    assert!(availability(Some(&v), &json!({})).unwrap().ok);
    assert!(
        availability(Some(&v), &json!({"t":"buy","unit":"rifle","slot":-0.0}))
            .unwrap()
            .ok
    );
    assert_eq!(
        availability(Some(&v), &json!({"watching":true}))
            .unwrap()
            .reason,
        "You are spectating"
    );
    let before = v.clone();
    assert!(
        availability(Some(&v), &json!({"t":"buy","unit":"tank","slot":0}))
            .unwrap()
            .ok
    );
    assert_eq!(v, before);
    v.apply(
        &json!({"t":"s","units":[],"gone":[4],"held":[4,5],"mp":10000,"tech":{"tier":3}})
            .to_string(),
    )
    .unwrap();
    assert_eq!(
        availability(Some(&v), &json!({"t":"buy","unit":"tank","slot":0}))
            .unwrap()
            .reason,
        "Needs a Motor Pool"
    );
}

#[test]
fn if03_denial_codes_keep_queue_context_and_unknown_fallback_without_target_details() {
    assert_eq!(
        deny_sentence("queueFull", Some("buy")).unwrap(),
        "The training queue is full"
    );
    assert_eq!(
        deny_sentence("queueFull", None).unwrap(),
        "The training queue is full"
    );
    assert_eq!(
        deny_sentence("queueFull", Some("")).unwrap(),
        "The training queue is full"
    );
    assert_eq!(
        deny_sentence("queueFull", Some("move")).unwrap(),
        "This unit already has 8 queued orders"
    );
    assert_eq!(
        deny_sentence("ammo", None).unwrap(),
        "Not enough carried ammunition"
    );
    assert_eq!(
        deny_sentence("unseen", None).unwrap(),
        "That target is not visible"
    );
    assert_eq!(
        deny_sentence("notVisible", None).unwrap(),
        "That spot is not visible"
    );
    assert_eq!(
        deny_sentence("airfield", None).unwrap(),
        "Paratroopers fly from one of your finished Airfields, at most 300 m away"
    );
    assert_eq!(
        deny_sentence("station", None).unwrap(),
        "Rail moves start within 30 m of one of your stations and go to another station on your land"
    );
    assert_eq!(
        deny_sentence("new-code", None).unwrap(),
        "That order cannot happen"
    );
}

#[test]
fn if03_one_thousand_pure_snapshot_queries_and_every_deny_code_match_prototype() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/interface-availability.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 1000);
    for (i, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let actual = availability(Some(&view(case["snapshot"].clone())), &case["action"]).unwrap();
        assert_eq!(
            json!({"ok":actual.ok,"reason":actual.reason}),
            case["expected"],
            "query {i}: {}",
            case["action"]
        );
    }
    for case in fixture["denials"].as_array().unwrap() {
        assert_eq!(
            deny_sentence(case["code"].as_str().unwrap(), case["command"].as_str()).unwrap(),
            case["expected"].as_str().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn if03_focused_denial_precedence_and_threshold_queries_match_prototype() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/interface-availability.json")).unwrap();
    let cases = fixture["focusedCases"].as_array().unwrap();
    assert!(!cases.is_empty());
    let mut names = std::collections::BTreeSet::new();
    for case in cases {
        assert!(names.insert(case["name"].as_str().unwrap()));
        let actual = availability(Some(&view(case["snapshot"].clone())), &case["action"])
            .unwrap_or_else(|error| panic!("{}: {error}", case["name"]));
        assert_eq!(
            json!({"ok": actual.ok, "reason": actual.reason}),
            case["expected"],
            "{}: {}",
            case["name"],
            case["action"]
        );
    }
}
