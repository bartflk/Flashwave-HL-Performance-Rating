//! Demo files on disk: names, kinds, start times, folders, sidecars. Every
//! file is built here, in a temp folder of its own that goes away after.

use hl_demos::header::HEADER_LEN;
use hl_demos::scan::{filename_time, playdemo_arg, read_demo, read_sidecar, scan, scan_folder, STV_DIR};
use hl_demos::DemoHeader;
use std::path::{Path, PathBuf};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("hl-demos-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn header(map: &str, recorder: &str, playback_s: f32, ticks: i32) -> Vec<u8> {
    let mut b = vec![0u8; HEADER_LEN];
    b[..8].copy_from_slice(b"HL2DEMO\0");
    b[8..12].copy_from_slice(&3i32.to_le_bytes());
    b[12..16].copy_from_slice(&24i32.to_le_bytes());
    let mut put = |at: usize, s: &str| b[at..at + s.len()].copy_from_slice(s.as_bytes());
    put(16, "a server");
    put(276, recorder);
    put(536, map);
    put(796, "tf");
    b[1056..1060].copy_from_slice(&playback_s.to_le_bytes());
    b[1060..1064].copy_from_slice(&ticks.to_le_bytes());
    b
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn a_filename_timestamp_is_found_anywhere_in_the_name() {
    assert_eq!(filename_time("2026-09-17_21-00-24.dem").as_deref(), Some("2026-09-17 21:00:24"));
    assert_eq!(filename_time("my demo 2026-09-17_21-00-24 final.dem").as_deref(), Some("2026-09-17 21:00:24"));
    // Two stamps: the first wins.
    assert_eq!(filename_time("2026-01-02_03-04-05_2026-06-07_08-09-10.dem").as_deref(), Some("2026-01-02 03:04:05"));
    // Multi-byte characters in front do not break the scan or panic on a char boundary.
    assert_eq!(filename_time("名前☃2026-09-17_21-00-24.dem").as_deref(), Some("2026-09-17 21:00:24"));
    // Exactly the stamp and nothing else.
    assert_eq!(filename_time("2026-09-17_21-00-24").as_deref(), Some("2026-09-17 21:00:24"));
    // Not quite one.
    for bad in ["", "x", "2026-09-17_21-00-2", "2026-09-17 21-00-24", "2026/09/17_21-00-24", "2026-09-17_21:00:24", "名前名前名前名前名前名前名前"] {
        assert_eq!(filename_time(bad), None, "{bad:?}");
    }
}

#[test]
fn playdemo_names_are_relative_forward_slashed_and_extensionless() {
    let tf = Path::new("D:/tf");
    assert_eq!(playdemo_arg(tf, Path::new("D:/tf/demos/stv/x.dem")), "demos/stv/x");
    // A dot in the name stays; only the extension goes.
    assert_eq!(playdemo_arg(tf, Path::new("D:/tf/demos/a.b.dem")), "demos/a.b");
    assert_eq!(playdemo_arg(tf, Path::new("D:/tf/demos/UPPER.DEM")), "demos/UPPER");
    // A demo outside tf has no relative name: the full path, without a leading slash.
    assert_eq!(playdemo_arg(tf, Path::new("/elsewhere/x.dem")), "elsewhere/x");
    // Backslashes become forward slashes.
    assert_eq!(playdemo_arg(Path::new("C:\\games\\tf"), Path::new("C:\\games\\tf\\demos\\x.dem")), "demos/x");
}

#[test]
fn a_pov_demo_starts_at_its_end_minus_its_length() {
    let d = Dir::new("pov");
    let tf = d.0.join("tf");
    let path = tf.join("demos/flashwav2026-09-17_21-00-24.dem");
    write(&path, &header("pl_upward_f12", "flashy", 120.0, 8000));
    let f = read_demo(&tf, &path).unwrap();
    assert_eq!(f.kind, "pov");
    assert_eq!(f.start_utc, Some(f.mtime as f64 - 120.0));
    assert_eq!(f.playdemo_arg, "demos/flashwav2026-09-17_21-00-24");
    assert_eq!(f.filename_time.as_deref(), Some("2026-09-17 21:00:24"));
    assert_eq!(f.header.map, "pl_upward_f12");
}

#[test]
fn a_demo_never_closed_has_no_start_and_no_rate() {
    let d = Dir::new("unclosed");
    let tf = d.0.join("tf");
    let path = tf.join("demos/crashed.dem");
    write(&path, &header("pl_upward_f12", "flashy", 0.0, 0));
    let f = read_demo(&tf, &path).unwrap();
    assert_eq!(f.start_utc, None);
    assert_eq!(f.header.tick_rate(), None);
    // A negative or NaN length is not a length either.
    for bad in [-5.0f32, f32::NAN] {
        let p = tf.join("demos/odd.dem");
        write(&p, &header("x", "y", bad, 100));
        let f = read_demo(&tf, &p).unwrap();
        assert_eq!(f.start_utc, None, "{bad}");
        assert_eq!(f.header.tick_rate(), None, "{bad}");
    }
}

#[test]
fn stv_demos_are_told_by_folder_or_by_recorder_and_get_no_start_from_their_mtime() {
    let d = Dir::new("stv");
    let tf = d.0.join("tf");
    let downloaded = tf.join(STV_DIR).join("auto-1.dem");
    write(&downloaded, &header("pl_upward_f12", "someone", 1000.0, 66_000));
    let recorded = tf.join("demos/auto-2.dem");
    write(&recorded, &header("pl_upward_f12", "SourceTV Demo", 1000.0, 66_000));
    for p in [&downloaded, &recorded] {
        let f = read_demo(&tf, p).unwrap();
        assert_eq!(f.kind, "stv", "{}", p.display());
        assert_eq!(f.start_utc, None, "a download's mtime is not when it was recorded");
    }
}

#[test]
fn a_file_that_is_not_a_demo_or_is_cut_short_is_reported_not_fatal() {
    let d = Dir::new("bad");
    let tf = d.0.join("tf");
    write(&tf.join("demos/good.dem"), &header("pl_upward_f12", "flashy", 10.0, 600));
    write(&tf.join("demos/short.dem"), &header("pl_upward_f12", "flashy", 10.0, 600)[..500]);
    write(&tf.join("demos/empty.dem"), b"");
    let mut wrong = header("pl_upward_f12", "flashy", 10.0, 600);
    wrong[0] = b'X';
    write(&tf.join("demos/wrong.dem"), &wrong);
    write(&tf.join("demos/notes.txt"), b"not a demo");
    let (found, failed) = scan(&tf);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].file_name, "good.dem");
    assert_eq!(failed.len(), 3);
}

#[test]
fn scanning_covers_tf_demos_and_stv_and_ignores_a_missing_folder() {
    let d = Dir::new("scan");
    let tf = d.0.join("tf");
    // No demos folder at all.
    std::fs::create_dir_all(&tf).unwrap();
    assert!(scan(&tf).0.is_empty());
    assert!(scan(&d.0.join("nowhere")).0.is_empty());
    write(&tf.join("a.dem"), &header("m", "r", 1.0, 66));
    write(&tf.join("demos/B.DEM"), &header("m", "r", 1.0, 66));
    write(&tf.join("demos/stv/c.dem"), &header("m", "r", 1.0, 66));
    write(&tf.join("demos/deeper/d.dem"), &header("m", "r", 1.0, 66));
    let names: Vec<String> = scan(&tf).0.into_iter().map(|f| f.file_name).collect();
    assert_eq!(names.len(), 3, "{names:?}");
    assert!(!names.contains(&"d.dem".to_string()), "only tf, tf/demos and tf/demos/stv");
}

#[test]
fn an_archive_folder_finds_demos_in_subfolders_and_dates_them_by_the_file() {
    let d = Dir::new("archive");
    let tf = d.0.join("tf");
    let archive = d.0.join("archive");
    write(&archive.join("s34/week3/x.dem"), &header("pl_upward_f12", "SourceTV Demo", 300.0, 20_000));
    write(&archive.join("top.dem"), &header("pl_upward_f12", "SourceTV Demo", 0.0, 0));
    let mut deep = archive.clone();
    for i in 0..12 {
        deep = deep.join(format!("l{i}"));
    }
    write(&deep.join("too-deep.dem"), &header("m", "r", 1.0, 66));
    let (found, failed) = scan_folder(&tf, &archive);
    assert!(failed.is_empty());
    let x = found.iter().find(|f| f.file_name == "x.dem").unwrap();
    // An STV demo in an archive copy is dated by its file, unlike a download.
    assert_eq!(x.start_utc, Some(x.mtime as f64 - 300.0));
    let top = found.iter().find(|f| f.file_name == "top.dem").unwrap();
    assert_eq!(top.start_utc, None);
    assert!(!found.iter().any(|f| f.file_name == "too-deep.dem"));
}

#[test]
fn sidecars_with_missing_fields_or_garbage() {
    let d = Dir::new("sidecar");
    let dem = d.0.join("x.dem");
    // No sidecar.
    assert!(read_sidecar(&dem).is_empty());
    std::fs::write(d.0.join("x.json"), "not json").unwrap();
    assert!(read_sidecar(&dem).is_empty());
    std::fs::write(d.0.join("x.json"), "{}").unwrap();
    assert!(read_sidecar(&dem).is_empty());
    std::fs::write(d.0.join("x.json"), r#"{"events":[{"name":"Bookmark","tick":5},{"name":"Killstreak","value":"3","tick":9}]}"#).unwrap();
    let e = read_sidecar(&dem);
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].value, None);
    assert_eq!(e[1].value.as_deref(), Some("3"));
    // One event missing its tick spoils the sidecar rather than the demo.
    std::fs::write(d.0.join("x.json"), r#"{"events":[{"name":"Bookmark"}]}"#).unwrap();
    assert!(read_sidecar(&dem).is_empty());
}

#[test]
fn headers_with_odd_text_fields() {
    // A field with no NUL at all uses its whole 260 bytes; high bytes are Latin-1.
    let mut b = header("x", "y", 1.0, 66);
    b[536..536 + 260].fill(b'm');
    b[276] = 0xe9;
    b[277] = 0;
    let h = DemoHeader::parse(&b).unwrap();
    assert_eq!(h.map.len(), 260);
    assert_eq!(h.recorder, "\u{e9}");
    // A header with extra bytes after it parses the same.
    b.extend_from_slice(&[1, 2, 3]);
    assert_eq!(DemoHeader::parse(&b).unwrap().map.len(), 260);
}
