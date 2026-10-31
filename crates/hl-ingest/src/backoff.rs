//! Requests that failed lately, so a screen opened again does not ask again
//! straight away (Flashy, auditing the profile's requests: "see if we can
//! cache stuff to lessen the load"). A site that is down, or a lookup that
//! failed, is left alone for a quarter of an hour; what was stored before
//! is shown meanwhile. In memory only: a restart tries again.

use std::collections::HashMap;
use std::sync::Mutex;

/// How long a failed request is left alone.
pub const WAIT_S: i64 = 15 * 60;

static FAILED: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Whether `key` failed less than [`WAIT_S`] ago.
pub fn waiting(key: &str) -> bool {
    let t = now();
    FAILED.lock().ok().and_then(|m| m.as_ref().and_then(|m| m.get(key).copied())).is_some_and(|at| t - at < WAIT_S)
}

/// `key` just failed.
pub fn failed(key: &str) {
    if let Ok(mut m) = FAILED.lock() {
        m.get_or_insert_with(HashMap::new).insert(key.to_string(), now());
    }
}

/// `key` just worked: no wait.
pub fn worked(key: &str) {
    if let Ok(mut m) = FAILED.lock() {
        if let Some(m) = m.as_mut() {
            m.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_waits_until_it_works() {
        assert!(!waiting("test:a"));
        failed("test:a");
        assert!(waiting("test:a"));
        worked("test:a");
        assert!(!waiting("test:a"));
    }
}
