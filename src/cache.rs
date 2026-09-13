use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(600);

#[derive(Debug)]
struct Entry<V> {
    value: V,
    expires_at: Instant,
}

/// Small process-local TTL cache. Values are cloned out of the lock.
#[derive(Debug)]
pub struct Cache<V> {
    entries: RwLock<HashMap<String, Entry<V>>>,
}

impl<V: Clone> Default for Cache<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone> Cache<V> {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }
    pub fn insert(&self, key: impl Into<String>, value: V, ttl: Duration) {
        let ttl = if ttl.is_zero() {
            DEFAULT_CACHE_TTL
        } else {
            ttl
        };
        self.entries.write().expect("cache lock poisoned").insert(
            key.into(),
            Entry {
                value,
                expires_at: Instant::now() + ttl,
            },
        );
    }
    pub fn get(&self, key: &str) -> Option<V> {
        let mut entries = self.entries.write().expect("cache lock poisoned");
        match entries.get(key) {
            Some(entry) if entry.expires_at > Instant::now() => Some(entry.value.clone()),
            Some(_) => {
                entries.remove(key);
                None
            }
            None => None,
        }
    }
    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    pub fn prune_expired(&self) {
        let now = Instant::now();
        self.entries
            .write()
            .expect("cache lock poisoned")
            .retain(|_, entry| entry.expires_at > now);
    }
    pub fn len(&self) -> usize {
        self.entries.read().expect("cache lock poisoned").len()
    }
}

/// Deterministic FNV-1a hash, unlike `std::collections::hash_map::DefaultHasher`.
pub fn stable_hash(input: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in input.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_expires_and_hash_is_stable() {
        let cache = Cache::new();
        cache.insert("key", 42, Duration::from_millis(1));
        assert_eq!(cache.get("key"), Some(42));
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(cache.get("key"), None);
        assert_eq!(stable_hash("query:42"), "10a54c387d2fc41d");
    }
}
