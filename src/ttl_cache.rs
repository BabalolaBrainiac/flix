use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Entry<V> {
    stored: Instant,
    value: Arc<V>,
}

/// A small in-memory cache. An entry expires after `ttl`. The oldest entry
/// leaves when the cache is full. Two uses: the page list of a manga chapter
/// (one upstream lookup per chapter, not per page) and the candidate pool of
/// a Discover query (one fetch for all its pages).
pub struct TtlCache<V> {
    entries: Mutex<HashMap<String, Entry<V>>>,
    ttl: Duration,
    max_entries: usize,
}

impl<V> TtlCache<V> {
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
            max_entries,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry<V>>> {
        self.entries.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The stored value, or `None` when there is none or it is too old.
    pub fn get(&self, key: &str) -> Option<Arc<V>> {
        self.lock()
            .get(key)
            .filter(|entry| entry.stored.elapsed() < self.ttl)
            .map(|entry| Arc::clone(&entry.value))
    }

    /// Stores a value and returns a shared handle to it.
    pub fn insert(&self, key: &str, value: V) -> Arc<V> {
        let value = Arc::new(value);
        let mut entries = self.lock();
        if entries.len() >= self.max_entries && !entries.contains_key(key) {
            let oldest = entries
                .iter()
                .min_by_key(|(_, entry)| entry.stored)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                entries.remove(&oldest);
            }
        }
        entries.insert(
            key.to_string(),
            Entry {
                stored: Instant::now(),
                value: Arc::clone(&value),
            },
        );
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_a_stored_value_until_it_expires() {
        let cache = TtlCache::new(Duration::from_millis(30), 4);
        cache.insert("c1", vec!["u1".to_string()]);
        assert_eq!(cache.get("c1").unwrap().as_slice(), ["u1".to_string()]);
        std::thread::sleep(Duration::from_millis(40));
        assert!(cache.get("c1").is_none());
    }

    #[test]
    fn evicts_the_oldest_entry_when_full() {
        let cache = TtlCache::new(Duration::from_secs(60), 2);
        cache.insert("a", 1);
        std::thread::sleep(Duration::from_millis(2));
        cache.insert("b", 2);
        std::thread::sleep(Duration::from_millis(2));
        cache.insert("c", 3);
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some() && cache.get("c").is_some());
    }

    #[test]
    fn replacing_a_key_does_not_evict_another_entry() {
        let cache = TtlCache::new(Duration::from_secs(60), 2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("a", 3);
        assert_eq!(*cache.get("a").unwrap(), 3);
        assert!(cache.get("b").is_some());
    }
}
