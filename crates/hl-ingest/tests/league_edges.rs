//! Edge cases for the league data's parsers and pure helpers: names that
//! change between seasons, pages with parts missing, transfers that do not
//! pair up, empty and malformed answers. No network, no database.

use hl_db::Transfer;
use hl_ingest::catalogue::{mvp_score, shrunk, table_places, MVP_FINAL_WEIGHT};
use hl_ingest::cups::kind_of;
use hl_ingest::etf2l::match_by_time;
use hl_ingest::leagues::{canonical_tier, division_and_stage, is_tournament, parse_name, parse_tournament, season_order, OFF_SEASON};
use hl_ingest::{match_page, news, recruitment, transfers};

// ---- leagues: season, division and stage names --------------------------------

#[test]
fn names_that_are_not_a_highlander_season_are_nothing() {
    for n in ["", "Highlander", "Highlander ", "Highlander Season", "Highlander Season x", "6v6 Season 20: Premiership", "highlander season 3", "Highlander Season 36 (Autumn 2026"] {
        assert_eq!(parse_name(n), None, "{n:?}");
    }
}

#[test]
fn a_season_number_too_big_to_hold_is_nothing_not_a_panic() {
    assert_eq!(parse_name("Highlander Season 99999999999999999999999: Mid"), None);
}

#[test]
fn the_brackets_and_the_colon_are_both_optional() {
    let want = Some((36, "Autumn 2026".to_string(), "Open".to_string(), "regular".to_string()));
    assert_eq!(parse_name("Highlander Season 36 (Autumn 2026): Open"), want);
    assert_eq!(parse_name("Highlander Season 36 (Autumn 2026) Open"), want);
    assert_eq!(parse_name("Highlander Season 36 (Autumn 2026) - Open"), want);
    assert_eq!(parse_name("Highlander Season 36(Autumn 2026): Open"), want);
    assert_eq!(parse_name("Highlander Season 36 (Autumn 2026):   Open  "), want);
}

#[test]
fn a_name_with_a_season_and_nothing_else_has_no_division() {
    assert_eq!(parse_name("Highlander Season 7"), Some((7, "Season 7".into(), "".into(), "regular".into())));
    assert_eq!(parse_name("Highlander Season 7:"), Some((7, "Season 7".into(), "".into(), "regular".into())));
}

#[test]
fn a_division_group_stays_in_the_division_name() {
    // The group letter is dropped later (Catalogue::ungrouped, canonical_tier).
    assert_eq!(parse_name("Highlander Season 20: Division 2A Playoffs"), Some((20, "Season 20".into(), "Division 2A".into(), "Playoffs".into())));
    assert_eq!(parse_name("Highlander Season 20: Open B"), Some((20, "Season 20".into(), "Open B".into(), "regular".into())));
}

#[test]
fn every_stage_suffix_is_read() {
    for (label, stage) in [("Final", "Final"), ("Grand Final", "Grand Final"), ("3rd Place Match", "3rd Place"), ("Playoffs", "Playoffs"), ("Relegation", "Relegation"), ("Qualifiers", "Qualifiers"), ("Signups", "Signups")] {
        let got = parse_name(&format!("Highlander Season 30: Mid {label}")).unwrap();
        assert_eq!((got.2.as_str(), got.3.as_str()), ("Mid", stage), "{label}");
    }
}

#[test]
fn a_preseason_cup_is_a_cup_and_never_a_division() {
    let got = parse_name("Highlander Season 33 (Spring 2025): Preseason Cup").unwrap();
    assert_eq!((got.2.as_str(), got.3.as_str()), ("", "Cup"));
    let got = parse_name("Highlander Season 33 (Spring 2025): Preseason Cup: Mid Playoffs").unwrap();
    assert_eq!((got.2.as_str(), got.3.as_str()), ("Mid", "Cup"));
}

#[test]
fn every_unnumbered_season_has_its_place_between_27_and_32() {
    for (name, number) in [("Winter 2023", 28), ("Spring 2023", 29), ("Autumn 2023", 30), ("Winter 2024", 31)] {
        let got = parse_name(&format!("Highlander {name}: High")).unwrap();
        assert_eq!((got.0, got.1.as_str()), (number, name));
    }
    // An unknown time of year is not guessed at.
    assert_eq!(parse_name("Highlander Summer 2023: High"), None);
}

#[test]
fn seasons_sort_in_time_order_with_the_off_season_between() {
    let mut s = vec![35, 134, 1, 34, 36, 2, OFF_SEASON + 12, 12, 13];
    s.sort_by_key(|n| season_order(*n));
    assert_eq!(s, vec![1, 2, 12, OFF_SEASON + 12, 13, 34, 134, 35, 36]);
    // Distinct seasons never share a place.
    let mut seen = std::collections::HashSet::new();
    for n in (0..60).chain(OFF_SEASON..OFF_SEASON + 60) {
        assert!(seen.insert(season_order(n)), "{n}");
    }
}

#[test]
fn a_division_lands_on_the_ladder_whatever_its_spelling_or_case() {
    for (name, tier) in [
        ("  mid  ", Some(2)),
        ("MID", Some(2)),
        ("Mid A", Some(2)),
        ("Mid a", Some(2)),
        ("Div 2", Some(1)),
        ("division 5B", Some(3)),
        ("Division 5 B", Some(3)),
        ("Division 7", Some(5)),
        ("Division 8", None),
        ("Division 0", None),
        ("Division", None),
        ("Division A", None),
        ("Prem", Some(0)),
        ("Fresh Meat", Some(5)),
        ("Premiership B", Some(0)),
        ("", None),
        ("A", None),
        ("Top Tiers", None),
        ("Open Playoffs", None),
        ("Mid\u{e9}", None),
    ] {
        assert_eq!(canonical_tier(name), tier, "{name:?}");
    }
}

#[test]
fn a_tournament_is_told_from_a_fun_team_bracket() {
    assert!(is_tournament("The Highlander Open", "Highlander", "The Highlander Open #4: Playoffs"));
    assert!(is_tournament("1 Day Cup", "Highlander", "Highlander One-Night-Cup"));
    assert!(is_tournament("Fun Cup", "Highlander", "HIGHLANDER EXPERIMENTAL CUP #3"));
    assert!(!is_tournament("Fun Cup", "Highlander", "Experimental Cup"), "no 'highlander' in the name");
    assert!(!is_tournament("Season", "Highlander", "Highlander Season 36"), "a season is not a tournament");
    assert!(!is_tournament("Fun Cup", "Highlander", "Highlander Fun Team Cup"));
    assert!(!is_tournament("Fun Cup", "HIGHLANDER FUN TEAM", "Highlander Cup"));
    assert!(!is_tournament("", "", ""));
}

#[test]
fn a_tournament_name_splits_at_its_first_colon_or_dash() {
    assert_eq!(parse_tournament(""), ("".into(), "".into()));
    assert_eq!(parse_tournament("Highlander Cup"), ("Highlander Cup".into(), "".into()));
    assert_eq!(parse_tournament("Highlander Cup: Mid"), ("Highlander Cup".into(), "Mid".into()));
    assert_eq!(parse_tournament("Highlander Cup: Final"), ("Highlander Cup".into(), "".into()));
    assert_eq!(parse_tournament("Highlander Cup: Mid Grand Final"), ("Highlander Cup".into(), "Mid".into()));
    assert_eq!(parse_tournament("Highlander Cup: Mid Third Place Match"), ("Highlander Cup".into(), "Mid".into()));
    // A colon wins over a dash, and a dash in the name itself is kept.
    assert_eq!(parse_tournament("High-Tide Cup: Open"), ("High-Tide Cup".into(), "Open".into()));
}

#[test]
fn a_division_and_stage_for_a_season_a_cup_or_neither() {
    assert_eq!(division_and_stage("Highlander Season 36 (Autumn 2026): Mid Playoffs"), Some(("Mid".into(), "Playoffs".into())));
    assert_eq!(division_and_stage("Highlander Experimental Cup #2: Open"), Some(("Open".into(), "Cup".into())));
    assert_eq!(division_and_stage(""), None);
}

#[test]
fn a_tournaments_kind() {
    for (name, kind) in [
        ("Highlander Experimental Cup #10", "experimental"),
        ("Highlander Nations Cup #9", "nations"),
        ("Highlander Season 36 Preseason Cup", "preseason"),
        ("High-Tide Highlander One-Night-Cup", "one-night"),
        ("Highlander 1 Day Cup", "one-night"),
        ("The Highlander Open #4", "open"),
        ("Something Else", "other"),
        ("", "other"),
        // Two words in one name: the first rule listed wins, and that is the preseason.
        ("Nations Preseason", "preseason"),
    ] {
        assert_eq!(kind_of(name), kind, "{name}");
    }
}

// ---- catalogue: a league table's placings, MVP arithmetic ----------------------

#[test]
fn a_league_tables_placings_are_its_first_three() {
    let json = r#"{"tables":{"Mid":[{"id":1,"ach":"1"},{"id":2,"ach":2},{"id":3,"ach":"3"},{"id":4,"ach":4},{"id":5,"ach":""},{"id":6},{"ach":1}],
                   "High":[{"id":9,"ach":1}]}}"#;
    let mut got = table_places(json);
    got.sort();
    assert_eq!(got, vec![("High".to_string(), 1, 9), ("Mid".to_string(), 1, 1), ("Mid".to_string(), 2, 2), ("Mid".to_string(), 3, 3)]);
}

#[test]
fn a_league_table_that_is_not_one_gives_no_placings() {
    for body in ["", "not json", "[]", "{}", r#"{"tables":[]}"#, r#"{"tables":{"Mid":"x"}}"#, r#"{"tables":{"Mid":{}}}"#, r#"{"tables":null}"#] {
        assert!(table_places(body).is_empty(), "{body:?}");
    }
}

#[test]
fn a_tied_table_can_give_two_teams_the_same_place() {
    let got = table_places(r#"{"tables":{"Mid":[{"id":1,"ach":1},{"id":2,"ach":1}]}}"#);
    assert_eq!(got.len(), 2);
}

#[test]
fn an_mvp_score_blends_the_final_and_the_playoff_run_by_how_much_of_it_there_is() {
    assert_eq!(mvp_score(1.2, None, 5), 1.2);
    assert_eq!(mvp_score(1.2, Some(0.8), 0), 1.2, "no playoff maps: the final alone");
    // A full run of maps gives the playoffs their whole share.
    let full = mvp_score(1.2, Some(0.8), 4);
    assert!((full - ((MVP_FINAL_WEIGHT * 1.2) + (1.0 - MVP_FINAL_WEIGHT) * 0.8)).abs() < 1e-12);
    // More maps than the full run do not count extra.
    assert_eq!(mvp_score(1.2, Some(0.8), 40), full);
    // In between, in between.
    let half = mvp_score(1.2, Some(0.8), 2);
    assert!(full < half && half < 1.2);
    // The same final and run give the same score: no hidden order.
    assert_eq!(mvp_score(1.0, Some(1.0), 3), 1.0);
}

#[test]
fn a_shrunk_average_with_no_games_is_neutral_and_with_many_is_the_average() {
    assert_eq!(shrunk(0.0, 0.0), 1.0);
    assert!((shrunk(1.5e6, 1e6) - 1.5).abs() < 1e-4);
    assert!(shrunk(2.0, 1.0) < 2.0 && shrunk(2.0, 1.0) > 1.0, "one game of 2.0 is pulled towards 1.0");
}

// ---- etf2l: which official a log belongs to, by time ---------------------------

#[test]
fn a_log_is_inside_an_officials_window_from_its_start_to_three_hours_after() {
    let m = [(7, 1000)];
    let at = |t: i64| match_by_time(&[(1, t)], &m);
    assert_eq!(at(1000), vec![(1, 7)], "at the scheduled time");
    assert_eq!(at(1000 + 3 * 3600), vec![(1, 7)], "exactly three hours after");
    assert!(at(1000 + 3 * 3600 + 1).is_empty());
    assert!(at(999).is_empty(), "never before it");
}

#[test]
fn a_log_in_two_windows_goes_to_the_match_that_started_closest_before_it() {
    let m = [(1, 0), (2, 3600), (3, 7200)];
    assert_eq!(match_by_time(&[(10, 7300)], &m), vec![(10, 3)]);
    assert_eq!(match_by_time(&[(10, 7100)], &m), vec![(10, 2)]);
}

#[test]
fn several_logs_of_one_official_all_belong_to_it() {
    let got = match_by_time(&[(10, 100), (11, 2000), (12, 5000), (13, 20_000)], &[(1, 0)]);
    assert_eq!(got, vec![(10, 1), (11, 1), (12, 1)]);
}

#[test]
fn no_logs_or_no_matches_pairs_nothing() {
    assert!(match_by_time(&[], &[(1, 0)]).is_empty());
    assert!(match_by_time(&[(1, 0)], &[]).is_empty());
    assert!(match_by_time(&[], &[]).is_empty());
}

#[test]
fn two_matches_at_the_same_minute_still_give_one_answer_per_log() {
    let got = match_by_time(&[(10, 100)], &[(1, 0), (2, 0)]);
    assert_eq!(got.len(), 1);
}

#[test]
fn extreme_times_do_not_overflow() {
    assert!(match_by_time(&[(1, i64::MAX)], &[(1, 0)]).is_empty());
    assert!(match_by_time(&[(1, i64::MIN)], &[(1, i64::MAX)]).is_empty());
}

// ---- transfers ------------------------------------------------------------------

fn tr(team: i64, player: i64, kind: &str, time: i64) -> Transfer {
    Transfer {
        team_id: team,
        team_name: format!("T{team}"),
        team_type: None,
        player_id: player,
        account_id: Some(player as u32),
        player_name: format!("p{player}"),
        kind: kind.into(),
        time,
        by_id: None,
        by_name: None,
    }
}

#[test]
fn a_transfer_page_that_is_not_one_is_nothing() {
    for body in ["", "not json", "[]", "{}", r#"{"data":{}}"#, r#"{"meta":{}}"#, r#"{"data":null}"#] {
        assert!(transfers::parse_page(body, None).is_none(), "{body:?}");
    }
}

#[test]
fn an_empty_transfer_page_has_no_rows_and_one_page_when_it_says_nothing_else() {
    let (rows, last) = transfers::parse_page(r#"{"data":[]}"#, None).unwrap();
    assert!(rows.is_empty());
    assert_eq!(last, 1);
    let (_, last) = transfers::parse_page(r#"{"data":[],"meta":{"last_page":0}}"#, None).unwrap();
    assert_eq!(last, 0);
}

#[test]
fn transfer_rows_missing_a_part_are_skipped_and_the_rest_read() {
    let body = r#"{"data":[
        {"who":{"id":1,"name":"a","steam":{"id64":"76561198099396919"}},"team":{"id":5,"name":"T"},"time":10,"type":"joined"},
        {"who":{"id":2,"name":"b"},"team":{"name":"no id"},"time":10,"type":"joined"},
        {"who":{"id":3,"name":"c"},"team":{"id":5},"type":"joined"},
        {"who":{"id":4,"name":"d"},"team":{"id":5},"time":10},
        {"who":{"name":"no id"},"team":{"id":5},"time":10,"type":"joined"},
        {"team":{"id":5},"time":10,"type":"joined"},
        {"who":{"id":6,"name":"f","steam":{"id64":12345}},"team":{"id":5,"name":"T"},"time":11,"type":"left"}]}"#;
    let (rows, _) = transfers::parse_page(body, None).unwrap();
    assert_eq!(rows.iter().map(|r| r.player_id).collect::<Vec<_>>(), vec![1, 6]);
    assert_eq!(rows[0].account_id, Some(139131191));
    // A number that is not a Steam ID64 is an account number: kept as read.
    assert_eq!(rows[1].account_id, Some(12345));
}

#[test]
fn a_player_without_a_name_or_steam_account_is_still_kept() {
    let body = r#"{"data":[{"who":{"id":9,"steam":{}},"team":{"id":5},"time":10,"type":"joined"}]}"#;
    let (rows, _) = transfers::parse_page(body, None).unwrap();
    assert_eq!((rows[0].player_name.as_str(), rows[0].account_id, rows[0].team_name.as_str()), ("", None, ""));
}

#[test]
fn a_player_page_takes_its_subject_for_rows_that_do_not_name_them() {
    let body = r#"{"data":[{"team":{"id":5,"name":"T"},"time":10,"type":"joined"},{"who":null,"team":{"id":6,"name":"U"},"time":20,"type":"left"}]}"#;
    let (rows, _) = transfers::parse_page(body, Some((1, Some(77), "Me"))).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.player_id == 1 && r.account_id == Some(77) && r.player_name == "Me"));
    assert!(transfers::parse_page(body, None).unwrap().0.is_empty(), "a team page's row must name its player");
}

#[test]
fn stays_of_nothing_are_nothing() {
    assert!(transfers::stays(&[]).is_empty());
}

#[test]
fn a_join_and_leave_pair_into_one_stay() {
    let s = transfers::stays(&[tr(1, 1, "joined", 10), tr(1, 1, "left", 20)]);
    assert_eq!(s.len(), 1);
    assert_eq!((s[0].from, s[0].to), (Some(10), Some(20)));
}

#[test]
fn a_join_and_leave_in_the_same_second_is_a_stay_of_no_length() {
    let s = transfers::stays(&[tr(1, 1, "joined", 10), tr(1, 1, "left", 10)]);
    assert_eq!(s.len(), 1);
    assert_eq!((s[0].from, s[0].to), (Some(10), Some(10)));
}

#[test]
fn two_leaves_in_a_row_are_two_stays_the_first_of_unknown_start() {
    let s = transfers::stays(&[tr(1, 1, "left", 10), tr(1, 1, "left", 20)]);
    assert_eq!(s.len(), 2);
    assert!(s.iter().all(|x| x.from.is_none()));
}

#[test]
fn the_same_player_on_two_teams_at_once_has_two_open_stays() {
    let s = transfers::stays(&[tr(1, 1, "joined", 10), tr(2, 1, "joined", 11), tr(1, 1, "left", 20)]);
    assert_eq!(s.len(), 2);
    let on_two = s.iter().find(|x| x.team_id == 2).unwrap();
    assert_eq!((on_two.from, on_two.to), (Some(11), None));
}

#[test]
fn a_rejoin_without_a_leave_ends_the_first_stay_at_the_rejoin() {
    let s = transfers::stays(&[tr(1, 1, "joined", 10), tr(1, 1, "joined", 30)]);
    assert_eq!(s.len(), 2);
    let ended = s.iter().find(|x| x.to.is_some()).unwrap();
    assert_eq!((ended.from, ended.to), (Some(10), Some(30)));
}

#[test]
fn a_leave_takes_the_name_the_player_had_then() {
    let mut leave = tr(1, 1, "left", 20);
    leave.player_name = "new name".into();
    let s = transfers::stays(&[tr(1, 1, "joined", 10), leave]);
    assert_eq!(s[0].name, "new name");
}

// ---- recruitment ----------------------------------------------------------------

#[test]
fn a_recruitment_answer_that_is_not_one_is_nothing() {
    for body in ["", "x", "[]", "{}", r#"{"recruitment":null}"#, r#"{"recruitment":{}}"#, r#"{"recruitment":{"data":{}}}"#, r#"{"recruitment":5}"#] {
        assert!(recruitment::parse_page(body).is_none(), "{body:?}");
    }
    assert_eq!(recruitment::parse_page(r#"{"recruitment":{"data":[]}}"#).unwrap().len(), 0);
}

#[test]
fn the_old_plain_list_and_the_paged_one_read_alike() {
    let row = r#"{"id":1,"name":"x","type":"Highlander","classes":[],"comments":{},"steam":{},"urls":{}}"#;
    let paged = recruitment::parse_page(&format!(r#"{{"recruitment":{{"data":[{row}]}}}}"#)).unwrap();
    let plain = recruitment::parse_page(&format!(r#"{{"recruitment":[{row}]}}"#)).unwrap();
    assert_eq!(paged, plain);
    assert_eq!(paged[0].link, "https://etf2l.org/recruitment/", "a post with no link points at the list");
}

#[test]
fn a_post_with_no_id_or_another_format_is_left_out_and_a_blank_name_stays_blank() {
    let body = r#"{"recruitment":{"data":[
        {"name":"no id","type":"Highlander"},
        {"id":2,"name":"   ","type":"Highlander"},
        {"id":3,"name":"six","type":"6v6"},
        {"id":4,"type":"Highlander"}]}}"#;
    let got = recruitment::parse_page(body).unwrap();
    assert_eq!(got.iter().map(|p| (p.id, p.name.as_str())).collect::<Vec<_>>(), vec![(2, ""), (4, "")]);
    assert_eq!((got[0].comments, got[0].last_comment, got[0].account_id, got[0].team_id), (0, None, None, None));
}

#[test]
fn a_player_post_with_an_odd_steam_id_has_no_account_not_a_wrong_one() {
    let body = |id3: &str| format!(r#"{{"recruitment":{{"data":[{{"id":1,"name":"x","type":"Highlander","steam":{{"id3":"{id3}"}},"urls":{{"player":"u"}}}}]}}}}"#);
    assert_eq!(recruitment::parse_page(&body("[U:1:5]")).unwrap()[0].account_id, Some(5));
    assert_eq!(recruitment::parse_page(&body("U:1:5")).unwrap()[0].account_id, Some(5));
    assert_eq!(recruitment::parse_page(&body("nonsense")).unwrap()[0].account_id, None);
    assert_eq!(recruitment::parse_page(&body("[U:1:99999999999]")).unwrap()[0].account_id, None, "more than an account number holds");
    assert_eq!(recruitment::parse_page(&body("")).unwrap()[0].account_id, None);
}

#[test]
fn a_comment_by_someone_whose_name_has_a_colon_keeps_the_whole_name() {
    let page = r##"<div class="rec"><h2>x</h2><small>Posted: <time datetime="2026-10-17T02:09:43+02:00">now</time></small><p>hi</p>
    <a href="u" class="small">p</a></div>
    <h3 id="comments">2 Comments</h3>
    <ol class="commentlist">
      <li class="alt" id="comment-1"><cite><a href="https://etf2l.org/forum/user/1">:pregnant_man:</a>: <a href="https://etf2l.org/teams/2">MNHT</a>, <a href="https://etf2l.org/teams/3">Other</a></cite> said:<br>
        <p class="postmeta"> <a href="#c"><time datetime="2026-10-16T19:28:40+02:00">x</time></a> </p><p>one</p></li>
      <li class="alt" id="comment-2"><cite><a href="http://www.etf2l.org" class="url">ETF2L Staff</a></cite> said:<br>
        <p class="postmeta"> <a href="#c"><time datetime="2026-10-16T19:29:40+02:00">x</time></a> </p><p>two</p></li>
    </ol>"##;
    let d = recruitment::parse_detail(page, "x").unwrap();
    assert_eq!(d.comments.len(), 2);
    assert_eq!((d.comments[0].author.as_str(), d.comments[0].teams.as_str(), d.comments[0].staff), (":pregnant_man:", "MNHT, Other", false));
    assert_eq!((d.comments[1].author.as_str(), d.comments[1].teams.as_str(), d.comments[1].staff), ("ETF2L Staff", "", true));
}

#[test]
fn a_post_page_cut_short_gives_what_it_has() {
    let d = recruitment::parse_detail(r#"<div class="rec">"#, "x").unwrap();
    assert_eq!((d.posted, d.last_online, d.posted_by, d.html.as_str()), (None, None, None, ""));
    assert!(d.history.is_empty() && d.comments.is_empty());
    // A history row with too few cells, and a time that is not one.
    let page = r#"<div class="rec"><small>Posted: <time datetime="garbage">x</time></small></div>
        <h2>Team History</h2><table><tr class="small "><td>Joined</td><td>T</td></tr></table>"#;
    let d = recruitment::parse_detail(page, "x").unwrap();
    assert_eq!(d.posted, None);
    assert!(d.history.is_empty());
}

// ---- match page -----------------------------------------------------------------

#[test]
fn a_match_page_without_its_content_is_nothing_and_an_empty_one_is_empty() {
    assert!(match_page::parse("", "l").is_none());
    assert!(match_page::parse("<div id=\"sidebars\"></div>", "l").is_none());
    let p = match_page::parse("<div id=\"content\">", "l").unwrap();
    assert_eq!(p, match_page::MatchPage { link: "l".into(), ..Default::default() });
}

#[test]
fn a_match_page_cut_in_the_middle_of_a_tag_does_not_panic() {
    let full = r##"<div id="content"><h1 class="c">Highlander Season 36 (Autumn 2026): Hiégh</h1><h3 class="c">Round 6</h3>
    <h4 class="c">Round commencing: <time datetime="2026-09-17T00:00:00+02:00">17 Sep</time></h4>
    <h4 class="c">Scheduled: <time datetime="2026-09-17T21:15:00+02:00">17 Sep</time><br>Results submitted: <time datetime="2026-09-17T21:59:03+02:00">17</time><br>by éé</h4>
    <div class="maps"><div class="map"><h2>pl_vigil_rc10</h2><img src="https://etf2l.org/images/maps/pl_vigil_rc10.jpg"/><p>1 - 0</p><p class="s">[Logs: <a href="https://logs.tf/4122215">A</a>]</p></div><div style="clear: both"></div></div>
    <p class="result"><a href="https://etf2l.org/teams/1">é</a><b>vs</b><a href="https://etf2l.org/teams/2">B</a><br>3 - 1</p>
    <h2>Demos</h2><ul><li><a href="https://etf2l.org/d/1">x</a></li></ul><h3 id="comments">0</h3><ol class="commentlist"><li class="alt"><cite>a</cite></li></ol></div>"##;
    // Every prefix, including the ones that end inside a multi-byte letter.
    for end in 0..=full.len() {
        if full.is_char_boundary(end) {
            let _ = match_page::parse(&full[..end], "l");
        }
    }
}

#[test]
fn a_forfeit_has_no_map_scores_and_a_score_for_the_team_that_took_it() {
    let page = r#"<div id="content"><h1 class="c">Highlander Season 36 (Autumn 2026): Mid</h1><h3 class="c">Round 2</h3>
    <div class="maps"><div class="map"><h2>TBD</h2><p>[XX] - [XX]</p></div><div class="map"><h2>pl_x</h2><p>0 - 0</p></div><div style="clear: both"></div></div>
    <p class="result"><a href="https://etf2l.org/teams/1">A</a><b>vs</b><a href="https://etf2l.org/teams/2">B</a><br>0 - 6</p>"#;
    let p = match_page::parse(page, "l").unwrap();
    assert_eq!(p.score, Some((0, 6)));
    assert_eq!(p.maps.len(), 2);
    assert_eq!(p.maps[0].map, None);
    assert_eq!(p.maps[1].score, Some((0, 0)));
}

#[test]
fn a_match_not_yet_played_has_teams_but_no_score() {
    let page = r#"<div id="content"><p class="result"><a href="https://etf2l.org/teams/1">A - 1</a><b>vs</b><a href="https://etf2l.org/teams/2">B</a></p>"#;
    let p = match_page::parse(page, "l").unwrap();
    assert_eq!(p.teams.len(), 2);
    assert_eq!(p.score, None, "a dash in a team's name is not a score");
}

#[test]
fn a_demo_link_that_is_not_on_etf2l_is_left_out() {
    let page = r#"<div id="content"><h2>Demos</h2><ul><li><a href="https://evil.example/d">x</a></li><li><a href="https://etf2l.org/d/2">ok</a></li></ul>"#;
    let p = match_page::parse(page, "l").unwrap();
    assert_eq!(p.demos.len(), 1);
    assert_eq!(p.demos[0].link, "https://etf2l.org/d/2");
}

#[test]
fn a_map_image_must_be_etf2ls_own() {
    let page = r#"<div id="content"><div class="maps"><div class="map"><h2>pl_x</h2><img src="https://elsewhere.example/a.jpg"/><p>1 - 0</p></div><div style="clear: both"></div></div>"#;
    assert_eq!(match_page::parse(page, "l").unwrap().maps[0].image, None);
}

#[test]
fn logs_in_a_maps_block_are_read_whatever_follows_the_number() {
    let page = r#"<div id="content"><div class="maps"><div class="map"><h2>pl_x</h2><p>1 - 0</p>
      <p class="s"><a href="https://logs.tf/123#x">a</a> <a href="http://logs.tf/456?y=1">b</a> <a href="https://logs.tf/">none</a> logs.tf/789</p></div><div style="clear: both"></div></div>"#;
    assert_eq!(match_page::parse(page, "l").unwrap().maps[0].logs, vec![123, 456, 789]);
}

// ---- news -----------------------------------------------------------------------

#[test]
fn news_that_is_not_a_list_is_nothing_and_an_empty_list_is_empty() {
    for body in ["", "x", "{}", "null", r#"{"id":1}"#] {
        assert!(news::parse(body).is_none(), "{body:?}");
    }
    assert_eq!(news::parse("[]").unwrap().len(), 0);
}

#[test]
fn a_post_without_an_id_or_a_readable_date_is_skipped() {
    let body = r#"[
      {"date_gmt":"2026-10-12T01:14:47","title":{"rendered":"no id"}},
      {"id":2,"title":{"rendered":"no date"}},
      {"id":3,"date_gmt":"2026-10-12T01:14:47.123","title":{"rendered":"fractional"}},
      {"id":4,"date_gmt":"2026-10-12 01:14:47","title":{"rendered":"space"}},
      {"id":5,"date_gmt":"2026-10-12T01:14:47","title":{"rendered":"ok"}}]"#;
    let got = news::parse(body).unwrap();
    assert_eq!(got.iter().map(|(n, _)| n.id).collect::<Vec<_>>(), vec![5]);
}

#[test]
fn a_post_with_only_an_id_and_a_date_still_reads() {
    let got = news::parse(r#"[{"id":1,"date_gmt":"1970-01-01T00:00:00"}]"#).unwrap();
    let (n, html) = &got[0];
    assert_eq!((n.time, n.title.as_str(), n.link.as_str(), n.excerpt.as_str(), n.image.as_deref(), html.as_str()), (0, "", "", "", None, ""));
}

#[test]
fn posts_come_newest_first_and_equal_times_keep_their_order() {
    let body = r#"[
      {"id":1,"date_gmt":"2026-10-01T00:00:00"},{"id":2,"date_gmt":"2026-10-03T00:00:00"},
      {"id":3,"date_gmt":"2026-10-03T00:00:00"},{"id":4,"date_gmt":"2026-10-02T00:00:00"}]"#;
    let ids: Vec<i64> = news::parse(body).unwrap().iter().map(|(n, _)| n.id).collect();
    assert_eq!(ids, vec![2, 3, 4, 1]);
}

#[test]
fn a_banner_credit_is_cut_from_the_excerpt_only_at_its_start() {
    let one = |excerpt: &str| {
        let body = format!(r#"[{{"id":1,"date_gmt":"2026-10-01T00:00:00","excerpt":{{"rendered":{}}}}}]"#, serde_json::to_string(excerpt).unwrap());
        news::parse(&body).unwrap().remove(0).0.excerpt
    };
    assert_eq!(one("<p>BANNER BY Someone</p>"), "Someone");
    assert_eq!(one("<p>banner by </p>"), "banner by", "nothing after the credit: nothing to cut");
    assert_eq!(one("<p>Our banner by someone</p>"), "Our banner by someone");
    assert_eq!(one("<p>Banner</p>"), "Banner");
    // A multi-byte letter across the tenth byte is not cut in half.
    assert_eq!(one("<p>\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}</p>"), "\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}");
    assert_eq!(one(""), "");
}

#[test]
fn only_a_secure_picture_is_a_banner() {
    let one = |content: &str| {
        let body = format!(r#"[{{"id":1,"date_gmt":"2026-10-01T00:00:00","content":{{"rendered":{}}}}}]"#, serde_json::to_string(content).unwrap());
        news::parse(&body).unwrap().remove(0).0.image
    };
    assert_eq!(one(r#"<img src="https://a/b.png">"#).as_deref(), Some("https://a/b.png"));
    assert_eq!(one(r#"<img class="x" src="https://a/b.png" alt="">"#).as_deref(), Some("https://a/b.png"));
    assert_eq!(one(r#"<img src="http://a/b.png">"#), None);
    assert_eq!(one(r#"<img data-src="https://a/b.png">"#), None);
    assert_eq!(one(r#"<img src="https://a/b.png"#), None, "a tag cut short");
    assert_eq!(one(r#"<p>no picture</p>"#), None);
    assert_eq!(one(r#"<img src="https://a/b.png?x=1&amp;y=2">"#).as_deref(), Some("https://a/b.png?x=1&y=2"));
}
