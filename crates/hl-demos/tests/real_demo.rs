//! The header and sidecar of real demos from the owner's install.

use hl_demos::scan::read_sidecar;
use hl_demos::DemoHeader;
use std::path::Path;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// Values read by hand from the file: flashwav2026-09-17_21-00-24.dem.
#[test]
fn parses_a_real_pov_header() {
    let bytes = std::fs::read(fixture("pov_swiftwater_header.bin")).unwrap();
    let h = DemoHeader::parse(&bytes).unwrap();
    assert_eq!(h.map, "pl_swiftwater_final1");
    assert_eq!(h.recorder, "flashy");
    assert_eq!(h.game_dir, "tf");
    assert!((h.playback_s / 60.0 - 61.9).abs() < 0.1, "about 61.9 minutes, got {}", h.playback_s);
    let rate = h.tick_rate().unwrap();
    assert!((rate - 66.67).abs() < 0.01, "TF2 runs at 66.67 ticks/s, got {rate}");
}

#[test]
fn rejects_a_file_that_is_not_a_demo() {
    let mut bytes = std::fs::read(fixture("pov_swiftwater_header.bin")).unwrap();
    bytes[0] = b'X';
    assert!(DemoHeader::parse(&bytes).is_err());
    assert!(DemoHeader::parse(&bytes[..100]).is_err());
}

#[test]
fn reads_killstreak_markers_from_the_sidecar() {
    // The sidecar sits next to the demo with a .json extension.
    let dem = fixture("sidecar_killstreak.dem");
    let events = read_sidecar(&dem);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "Killstreak");
    assert_eq!(events[0].value.as_deref(), Some("4"));
    assert_eq!(events[0].tick, 33062);
}
