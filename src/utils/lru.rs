use std::{
    borrow::Borrow,
    collections::{HashMap, VecDeque},
    hash::Hash,
};

pub(crate) struct LruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    capacity: usize,
    inner: HashMap<K, V>,
    usage_queue: VecDeque<K>,
}

impl<K, V> std::fmt::Debug for LruCache<K, V>
where
    K: std::fmt::Debug + Eq + Hash + Clone,
    V: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LruCache")
            .field("capacity", &self.capacity)
            .field("inner", &self.inner)
            .field("usage_queue", &self.usage_queue)
            .finish()
    }
}

impl<K, V> Default for LruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    fn default() -> Self {
        Self {
            capacity: 10,
            inner: HashMap::default(),
            usage_queue: VecDeque::default(),
        }
    }
}

impl<K, V> LruCache<K, V>
where
    K: Eq + Hash + Clone,
{
    pub(crate) fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.inner.contains_key(key)
    }

    pub(crate) fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.inner.contains_key(&key) {
            self.usage_queue.retain(|k| k != &key);
        } else if self.inner.len().saturating_add(1) > self.capacity {
            if let Some(lru_key) = self.usage_queue.pop_front() {
                self.inner.remove(&lru_key);
            }
        }

        self.usage_queue.push_back(key.clone());
        self.inner.insert(key, value)
    }

    pub(crate) fn get(&mut self, key: &K) -> Option<&V> {
        if self.inner.contains_key(key) {
            self.usage_queue.retain(|k| k != key);
            self.usage_queue.push_back(key.clone());
        }
        self.inner.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_cache_basic() {
        let mut cache = LruCache::<String, i32>::default();

        // Insert some items
        assert_eq!(cache.insert("a".to_string(), 1), None);
        assert_eq!(cache.insert("b".to_string(), 2), None);
        assert_eq!(cache.insert("c".to_string(), 3), None);

        // Check if keys exist
        assert!(cache.contains_key("a"));
        assert!(cache.contains_key("b"));
        assert!(cache.contains_key("c"));

        // Overwrite existing key
        assert_eq!(cache.insert("b".to_string(), 22), Some(2));
        assert!(cache.contains_key("b"));
        assert_eq!(cache.get(&"b".to_string()), Some(&22));
    }

    #[test]
    fn test_lru_cache_eviction() {
        let mut cache = LruCache::<String, i32>::default();
        cache.capacity = 3;

        // Fill the cache
        cache.insert("a".to_string(), 1);
        cache.insert("b".to_string(), 2);
        cache.insert("c".to_string(), 3);

        // All three items should be in the cache
        assert!(cache.contains_key("a"));
        assert!(cache.contains_key("b"));
        assert!(cache.contains_key("c"));

        // Adding a new item should evict the least recently used item (a)
        cache.insert("d".to_string(), 4);
        assert!(!cache.contains_key("a"));
        assert!(cache.contains_key("b"));
        assert!(cache.contains_key("c"));
        assert!(cache.contains_key("d"));

        // Using an item moves it to the back of the queue
        // Access b (now b is most recently used)
        cache.insert("b".to_string(), 22);

        // Adding another item should evict c now
        cache.insert("e".to_string(), 5);
        assert!(!cache.contains_key("a"));
        assert!(cache.contains_key("b"));
        assert!(!cache.contains_key("c"));
        assert!(cache.contains_key("d"));
        assert!(cache.contains_key("e"));
    }
}
