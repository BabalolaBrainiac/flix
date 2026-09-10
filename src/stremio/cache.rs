use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 64;
const MAX_AGE: Duration = Duration::from_secs(300);

struct Entry {
    key: String,
    bytes: Arc<[u8]>,
    created: Instant,
}

#[derive(Default)]
pub(super) struct ResponseCache {
    entries: VecDeque<Entry>,
    size: usize,
}

impl ResponseCache {
    pub fn get(&mut self, key: &str) -> Option<Arc<[u8]>> {
        self.expire();
        self.entries
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.bytes.clone())
    }

    pub fn insert(&mut self, key: String, bytes: Vec<u8>) {
        self.expire();
        let size = key.len() + bytes.len();
        if size > MAX_BYTES {
            return;
        }
        if let Some(index) = self.entries.iter().position(|entry| entry.key == key) {
            let old = self.entries.remove(index).unwrap();
            self.size -= old.key.len() + old.bytes.len();
        }
        while self.entries.len() >= MAX_ENTRIES || self.size + size > MAX_BYTES {
            self.remove_first();
        }
        self.size += size;
        self.entries.push_back(Entry {
            key,
            bytes: bytes.into(),
            created: Instant::now(),
        });
    }

    fn expire(&mut self) {
        while self
            .entries
            .front()
            .is_some_and(|entry| entry.created.elapsed() >= MAX_AGE)
        {
            self.remove_first();
        }
    }

    fn remove_first(&mut self) {
        if let Some(entry) = self.entries.pop_front() {
            self.size -= entry.key.len() + entry.bytes.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expires_entries_and_bounds_memory() {
        let mut cache = ResponseCache::default();
        cache.insert("old".into(), vec![1]);
        cache.entries[0].created = Instant::now() - MAX_AGE;
        assert!(cache.get("old").is_none());
        for index in 0..100 {
            cache.insert(index.to_string(), vec![0; 200_000]);
        }
        assert!(cache.size <= MAX_BYTES);
        assert!(cache.entries.len() <= MAX_ENTRIES);
        assert!(cache.get("0").is_none());
        cache.insert("99".into(), vec![7]);
        assert_eq!(&*cache.get("99").unwrap(), &[7]);
        cache.insert("large".into(), vec![0; MAX_BYTES + 1]);
        assert!(cache.get("large").is_none());
    }
}
