//! Collapsing combined logs and their per-round parts into one match.
//!
//! trends.tf's `duplicate_of` sits on the **combined** log and lists the
//! per-round **parts** it was built from:
//!
//! ```text
//! 4109131  2816s  duplicate_of [4109086, 4109098, 4109125]   <- keep
//! 4109086   778s                                               <- superseded
//! 4109098   916s                                               <- superseded
//! 4109125  1122s                                               <- superseded
//! ```
//!
//! Real data also has overlapping combines (49 parts on this account are
//! claimed by more than one combined log) — someone combines rounds 1-2 and
//! someone else combines 1-3. So this groups every log connected through a
//! shared part and keeps exactly one per group.
//!
//! trends.tf only knows the parts of logs it indexed. A combined log that
//! only logs.tf has (the 2018 `gullywash + badwater` semi-final) lists none,
//! so its parts are found another way: logs.tf copies rounds verbatim when
//! combining, so every round of a part starts at the same second as one of
//! the combined log's rounds. See [`contained_parts`].

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub log_id: i64,
    pub duration_s: i64,
    pub duplicate_of: Vec<i64>,
}

/// Returns `superseded log -> the log that replaces it`. Logs absent from the
/// map are kept.
pub fn supersessions(entries: &[IndexEntry]) -> HashMap<i64, i64> {
    let mut uf = UnionFind::default();
    for e in entries {
        uf.add(e.log_id);
        for &part in &e.duplicate_of {
            uf.union(e.log_id, part);
        }
    }

    // Only logs we actually have index rows for can be kept or superseded.
    // A missing part can still bridge two combined logs, which is why it was
    // added to the union-find above.
    let mut groups: HashMap<i64, Vec<&IndexEntry>> = HashMap::new();
    for e in entries {
        groups.entry(uf.find(e.log_id)).or_default().push(e);
    }

    let mut out = HashMap::new();
    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        // The whole match beats any slice of it: longest first, then the one
        // built from the most parts, then the newest upload.
        let keep = members
            .iter()
            .max_by_key(|e| (e.duration_s, e.duplicate_of.len(), e.log_id))
            .expect("group is non-empty")
            .log_id;
        for e in members {
            if e.log_id != keep {
                out.insert(e.log_id, keep);
            }
        }
    }
    out
}

/// Logs whose every round also appears, start time for start time, in a
/// log with more rounds: `(whole, part)`. Round start times are to the second
/// and a combined log copies them from its parts, so a match is not chance.
/// On this account: five logs, all genuine.
pub fn contained_parts(round_starts: &HashMap<i64, Vec<i64>>) -> Vec<(i64, i64)> {
    let mut owners: HashMap<i64, Vec<i64>> = HashMap::new();
    for (&log, starts) in round_starts {
        for &s in starts {
            owners.entry(s).or_default().push(log);
        }
    }
    let mut out = Vec::new();
    for (&part, starts) in round_starts {
        if starts.is_empty() {
            continue;
        }
        // Logs holding every one of this log's round starts.
        let mut holders: Option<Vec<i64>> = None;
        for s in starts {
            let here: Vec<i64> = owners[s].iter().copied().filter(|l| *l != part).collect();
            holders = Some(match holders {
                None => here,
                Some(h) => h.into_iter().filter(|l| here.contains(l)).collect(),
            });
        }
        for whole in holders.unwrap_or_default() {
            if round_starts[&whole].len() > starts.len() {
                out.push((whole, part));
            }
        }
    }
    out.sort_unstable();
    out
}

#[derive(Default)]
struct UnionFind {
    parent: HashMap<i64, i64>,
}

impl UnionFind {
    fn add(&mut self, x: i64) {
        self.parent.entry(x).or_insert(x);
    }

    fn find(&mut self, x: i64) -> i64 {
        self.add(x);
        let mut root = x;
        while self.parent[&root] != root {
            root = self.parent[&root];
        }
        // Path compression.
        let mut cur = x;
        while self.parent[&cur] != root {
            let next = self.parent[&cur];
            self.parent.insert(cur, root);
            cur = next;
        }
        root
    }

    fn union(&mut self, a: i64, b: i64) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent.insert(ra, rb);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_inside_another_is_its_part() {
        // 2111181 combines 2111140 (gullywash) and 2111157 (badwater).
        let rounds: HashMap<i64, Vec<i64>> = [
            (2111181, vec![100, 200, 300, 400]),
            (2111140, vec![100, 200]),
            (2111157, vec![300, 400]),
            (9, vec![500]),
            // The same match uploaded twice is not a part of itself.
            (10, vec![600, 700]),
            (11, vec![600, 700]),
        ]
        .into_iter()
        .collect();
        assert_eq!(contained_parts(&rounds), vec![(2111181, 2111140), (2111181, 2111157)]);
    }

    fn e(log_id: i64, duration_s: i64, duplicate_of: &[i64]) -> IndexEntry {
        IndexEntry { log_id, duration_s, duplicate_of: duplicate_of.to_vec() }
    }

    /// The real group from the account: one combined log, three parts.
    #[test]
    fn combined_log_supersedes_its_parts() {
        let s = supersessions(&[
            e(4109131, 2816, &[4109086, 4109098, 4109125]),
            e(4109086, 778, &[]),
            e(4109098, 916, &[]),
            e(4109125, 1122, &[]),
        ]);
        assert_eq!(s.len(), 3);
        assert!(s.values().all(|&keep| keep == 4109131));
        assert!(!s.contains_key(&4109131));
    }

    #[test]
    fn overlapping_combines_keep_only_the_fullest() {
        // Rounds 1-2 combined by one person, rounds 1-3 by another.
        let s = supersessions(&[
            e(10, 1600, &[1, 2]),
            e(11, 2400, &[1, 2, 3]),
            e(1, 800, &[]),
            e(2, 800, &[]),
            e(3, 800, &[]),
        ]);
        assert!(!s.contains_key(&11));
        assert_eq!(s[&10], 11);
        assert_eq!(s[&1], 11);
        assert_eq!(s[&3], 11);
    }

    #[test]
    fn a_missing_part_still_links_the_group() {
        // Part 99 was never indexed, but both combines reference it.
        let s = supersessions(&[e(20, 1000, &[99]), e(21, 2000, &[99, 98])]);
        assert_eq!(s[&20], 21);
    }

    #[test]
    fn standalone_logs_are_untouched() {
        let s = supersessions(&[e(1, 1800, &[]), e(2, 1800, &[])]);
        assert!(s.is_empty());
    }
}
