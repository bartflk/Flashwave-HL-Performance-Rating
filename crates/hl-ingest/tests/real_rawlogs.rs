//! The raw-log parser against logs.tf's own totals for the same two matches:
//! a 2026 Swiftwater scrim and a 2014 TF2Center lobby. logs.tf built its
//! summary from these exact files, so every player's kills, assists and
//! kills-by-victim-class must come out identical.

use hl_ingest::rawlog::{parse, unzip, RawLog};
use serde_json::Value;
use std::collections::BTreeMap;

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).expect(&path)
}

fn check(log_id: i64) {
    let raw: RawLog = parse(&unzip(&fixture(&format!("log_{log_id}.log.zip"))).unwrap());
    let want: Value = serde_json::from_slice(&fixture(&format!("logstf_totals_{log_id}.json"))).unwrap();

    let mut kills: BTreeMap<String, i64> = BTreeMap::new();
    let mut assists: BTreeMap<String, i64> = BTreeMap::new();
    let mut by_class: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for k in raw.kills.iter().filter(|k| k.counts()) {
        let id = format!("[U:1:{}]", k.killer.account);
        *kills.entry(id.clone()).or_default() += 1;
        let class = k.victim.class.map(|c| c.as_str()).unwrap_or("unknown");
        // logs.tf spells Heavy `heavyweapons`.
        let class = if class == "heavy" { "heavyweapons" } else { class };
        *by_class.entry(id).or_default().entry(class.to_string()).or_default() += 1;
    }
    for k in raw.kills.iter().filter(|k| k.assist_counts()) {
        *assists.entry(format!("[U:1:{}]", k.assister.unwrap())).or_default() += 1;
    }

    for (id, p) in want["players"].as_object().unwrap() {
        assert_eq!(kills.get(id).copied().unwrap_or(0), p["kills"].as_i64().unwrap(), "{log_id} kills of {id}");
        let ck: BTreeMap<String, i64> = want["classkills"]
            .get(id)
            .and_then(Value::as_object)
            .map(|m| m.iter().map(|(c, n)| (c.clone(), n.as_i64().unwrap())).collect())
            .unwrap_or_default();
        assert_eq!(by_class.get(id).cloned().unwrap_or_default(), ck, "{log_id} classkills of {id}");
        assert_eq!(assists.get(id).copied().unwrap_or(0), p["assists"].as_i64().unwrap(), "{log_id} assists of {id}");
    }
}

#[test]
fn kills_match_logstf_on_a_2026_scrim() {
    check(4121291);
}

#[test]
fn kills_match_logstf_on_a_2014_lobby() {
    check(513611);
}
