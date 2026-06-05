use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use atomicwrites::{AtomicFile, OverwriteBehavior};
use serde::{Deserialize, Serialize};

use thiserror::Error;

use crate::feed;

#[derive(Error, Debug)]
pub enum DataError {
    #[error("io error")]
    Io(#[from] std::io::Error),
    #[error("json error")]
    Json(#[from] serde_json::Error),
}

fn gen_hash<T: Hash>(t: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::default();
    t.hash(&mut hasher);
    hasher.finish()
}

type FeedId = u64;
type SubscriberId = i64;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Feed {
    pub link: String,
    pub title: String,
    pub down_time: Option<SystemTime>,
    pub subscribers: HashSet<SubscriberId, Size64>,
    pub ttl: Option<u32>,
    hash_list: Vec<u64>,
    #[serde(skip)]
    hash_set: Option<HashSet<u64, Size64>>,
}

impl Feed {
    fn ensure_hash_set(&mut self) -> &HashSet<u64, Size64> {
        self.hash_set
            .get_or_insert_with(|| self.hash_list.iter().copied().collect())
    }

    fn replace_hash_list(&mut self, hash_list: Vec<u64>) {
        self.hash_set = Some(hash_list.iter().copied().collect());
        self.hash_list = hash_list;
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hub {
    pub callback: String,
    pub secret: String,
}

#[derive(Debug)]
pub struct Database {
    path: PathBuf,
    feeds: HashMap<FeedId, Feed, Size64>,
    subscribers: HashMap<SubscriberId, HashSet<FeedId, Size64>, Size64>,
    dirty: bool,
    dirty_generation: u64,
}

impl Database {
    pub fn create(path: PathBuf) -> Result<Database, DataError> {
        let result = Database {
            path,
            feeds: HashMap::with_hasher(Size64::default()),
            subscribers: HashMap::with_hasher(Size64::default()),
            dirty: false,
            dirty_generation: 0,
        };

        result.save()?;

        Ok(result)
    }

    pub fn open(path: PathBuf) -> Result<Database, DataError> {
        if path.exists() {
            let f = File::open(&path)?;
            let feeds_list: Vec<Feed> = serde_json::from_reader(&f)?;

            let mut feeds = HashMap::with_capacity_and_hasher(feeds_list.len(), Size64::default());
            let mut subscribers = HashMap::with_hasher(Size64::default());

            for feed in feeds_list {
                let feed_id = gen_hash(&feed.link);
                for subscriber in &feed.subscribers {
                    let subscribed_feeds = subscribers
                        .entry(subscriber.to_owned())
                        .or_insert_with(HashSet::default);
                    subscribed_feeds.insert(feed_id);
                }
                feeds.insert(feed_id, feed);
            }

            Ok(Database {
                path,
                feeds,
                subscribers,
                dirty: false,
                dirty_generation: 0,
            })
        } else {
            Database::create(path)
        }
    }

    pub fn all_feeds(&self) -> Vec<Feed> {
        self.feeds.values().cloned().collect()
    }

    pub fn feed_fetch_info(&self) -> Vec<FeedFetchInfo> {
        self.feeds
            .values()
            .map(|feed| FeedFetchInfo {
                link: feed.link.clone(),
                title: feed.title.clone(),
                ttl: feed.ttl,
                subscribers: feed.subscribers.iter().copied().collect(),
            })
            .collect()
    }

    pub fn all_subscribers(&self) -> Vec<SubscriberId> {
        self.subscribers.keys().copied().collect()
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    pub fn dirty_generation(&self) -> u64 {
        self.dirty_generation
    }

    pub fn clear_dirty_if_generation(&mut self, generation: u64) -> bool {
        if self.dirty_generation == generation {
            self.dirty = false;
            true
        } else {
            false
        }
    }

    /// Mark database as needing to be persisted. Actual save is deferred
    /// to the next flush cycle, reducing I/O from O(n) to O(1) per batch.
    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.dirty_generation = self.dirty_generation.wrapping_add(1);
    }

    /// Persist to disk only if there are pending changes.
    /// Should be called periodically from a background task.
    #[allow(dead_code)]
    pub fn flush_if_dirty(&mut self) -> Result<(), DataError> {
        if self.dirty {
            self.save()?;
            self.clear_dirty();
        }
        Ok(())
    }

    pub fn subscribed_feeds(&self, subscriber: SubscriberId) -> Option<Vec<FeedInfo>> {
        self.subscribers.get(&subscriber).map(|feed_ids| {
            feed_ids
                .iter()
                .filter_map(|feed_id| self.feeds.get(feed_id).map(FeedInfo::from))
                .collect()
        })
    }

    /// Return `None` if feed not found
    pub fn get_or_update_down_time(&mut self, rss_link: &str) -> Option<Duration> {
        let feed_id = gen_hash(&rss_link);
        let feed = self.feeds.get_mut(&feed_id)?;
        let now = SystemTime::now();
        if let Some(t) = feed.down_time {
            Some(now.duration_since(t).unwrap_or_default())
        } else {
            feed.down_time = Some(now);
            self.mark_dirty();
            Some(Duration::default())
        }
    }

    pub fn reset_down_time(&mut self, rss_link: &str) -> bool {
        let feed_id = gen_hash(&rss_link);
        let changed = match self.feeds.get_mut(&feed_id) {
            Some(feed) => feed.down_time.take().is_some(),
            None => return false,
        };
        if changed {
            self.mark_dirty();
        }
        true
    }

    pub fn is_subscribed(&self, subscriber: SubscriberId, rss_link: &str) -> bool {
        self.subscribers
            .get(&subscriber)
            .map(|feeds| feeds.contains(&gen_hash(&rss_link)))
            .unwrap_or(false)
    }

    pub fn subscribe(&mut self, subscriber: SubscriberId, rss_link: &str, rss: &feed::Rss) -> bool {
        let feed_id = gen_hash(&rss_link);
        {
            let subscribed_feeds = self
                .subscribers
                .entry(subscriber)
                .or_default();
            if !subscribed_feeds.insert(feed_id) {
                return false;
            }
        }
        {
            let feed = self.feeds.entry(feed_id).or_insert_with(|| Feed {
                link: rss_link.to_owned(),
                title: rss.title.to_owned(),
                down_time: None,
                ttl: rss.ttl,
                hash_list: rss.items.iter().map(gen_item_hash).collect(),
                subscribers: HashSet::default(),
                hash_set: None,
            });
            feed.subscribers.insert(subscriber);
        }
        self.mark_dirty();
        true
    }

    pub fn unsubscribe(&mut self, subscriber: SubscriberId, rss_link: &str) -> Option<FeedInfo> {
        let feed_id = gen_hash(&rss_link);

        let clear_subscriber;
        if let Some(subscribed_feeds) = self.subscribers.get_mut(&subscriber) {
            if subscribed_feeds.remove(&feed_id) {
                clear_subscriber = subscribed_feeds.is_empty();
            } else {
                return None;
            }
        } else {
            return None;
        }
        if clear_subscriber {
            self.subscribers.remove(&subscriber);
        }

        let result;
        let clear_feed;
        if let Some(feed) = self.feeds.get_mut(&feed_id) {
            if feed.subscribers.remove(&subscriber) {
                clear_feed = feed.subscribers.is_empty();
                result = FeedInfo::from(&*feed);
            } else {
                return None;
            }
        } else {
            return None;
        };
        if clear_feed {
            self.feeds.remove(&feed_id);
        }
        self.mark_dirty();
        Some(result)
    }

    pub fn delete_subscriber(&mut self, subscriber: SubscriberId) -> bool {
        let feed_ids = match self.subscribers.remove(&subscriber) {
            Some(ids) => ids,
            None => return false,
        };
        for feed_id in &feed_ids {
            if let Some(feed) = self.feeds.get_mut(feed_id) {
                feed.subscribers.remove(&subscriber);
                if feed.subscribers.is_empty() {
                    self.feeds.remove(feed_id);
                }
            }
        }
        self.mark_dirty();
        true
    }

    pub fn update_subscriber(&mut self, from: SubscriberId, to: SubscriberId) -> bool {
        self.subscribers
            .remove(&from)
            .map(|feeds| {
                for feed_id in &feeds {
                    if let Some(feed) = self.feeds.get_mut(feed_id) {
                        feed.subscribers.remove(&from);
                        feed.subscribers.insert(to);
                    }
                }
                self.subscribers.insert(to, feeds);
                self.mark_dirty();
            })
            .is_some()
    }

    pub fn update(&mut self, rss_link: &str, new_feed: feed::Rss) -> Vec<FeedUpdate> {
        let feed_id = gen_hash(&rss_link);
        let mut updates = Vec::new();
        let mut changed = false;
        {
            let feed = match self.feeds.get_mut(&feed_id) {
                Some(f) => f,
                None => return Vec::new(),
            };

            let mut new_items = Vec::new();
            let mut new_hash_list = Vec::new();
            let hash_set = feed.ensure_hash_set();
            let items_len = new_feed.items.len();
            for item in new_feed.items {
                let hash = gen_item_hash(&item);
                if !hash_set.contains(&hash) {
                    new_hash_list.push(hash);
                    new_items.push(item);
                }
            }
            if !new_items.is_empty() {
                updates.push(FeedUpdate::Items(new_items));

                let max_size = items_len * 2;
                let mut append: Vec<u64> = feed
                    .hash_list
                    .iter()
                    .take(max_size - new_hash_list.len())
                    .cloned()
                    .collect();
                new_hash_list.append(&mut append);
                feed.replace_hash_list(new_hash_list);
                changed = true;
            }
            if new_feed.title != feed.title {
                updates.push(FeedUpdate::Title(new_feed.title.clone()));
                feed.title = new_feed.title;
                changed = true;
            }
            if feed.ttl != new_feed.ttl {
                feed.ttl = new_feed.ttl;
                changed = true;
            }
            if feed.down_time.take().is_some() {
                changed = true;
            }
        }
        if changed {
            self.mark_dirty();
        }
        updates
    }

    pub fn save(&self) -> Result<(), DataError> {
        let feeds_list: Vec<&Feed> = self.feeds.values().collect();
        let file = AtomicFile::new(&self.path, OverwriteBehavior::AllowOverwrite);
        file.write(|file| serde_json::to_writer(file, &feeds_list))
            .map_err(|e| match e {
                atomicwrites::Error::Internal(e) => DataError::Io(e),
                atomicwrites::Error::User(e) => {
                    assert!(!e.is_io(), "unreachable code");
                    DataError::Io(e.into())
                }
            })?;
        Ok(())
    }

    pub fn serialize(&self) -> Result<Vec<u8>, DataError> {
        let feeds_list: Vec<&Feed> = self.feeds.values().collect();
        Ok(serde_json::to_vec(&feeds_list)?)
    }

    pub fn save_from_serialized(path: PathBuf, data: &[u8]) -> Result<(), DataError> {
        let file = AtomicFile::new(&path, OverwriteBehavior::AllowOverwrite);
        file.write(|file| {
            use std::io::Write;
            file.write_all(data)
        })
        .map_err(|e| match e {
            atomicwrites::Error::Internal(e) => DataError::Io(e),
            atomicwrites::Error::User(e) => DataError::Io(e),
        })?;
        Ok(())
    }
}

pub enum FeedUpdate {
    Items(Vec<feed::Item>),
    Title(String),
}

pub struct FeedFetchInfo {
    pub link: String,
    pub title: String,
    pub ttl: Option<u32>,
    pub subscribers: Vec<SubscriberId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedInfo {
    pub link: String,
    pub title: String,
}

impl From<&Feed> for FeedInfo {
    fn from(feed: &Feed) -> Self {
        FeedInfo {
            link: feed.link.clone(),
            title: feed.title.clone(),
        }
    }
}

fn gen_item_hash(item: &feed::Item) -> u64 {
    item.id.as_ref().map(|id| gen_hash(&id)).unwrap_or_else(|| {
        let title = item.title.as_deref().unwrap_or_default();
        let link = item.link.as_deref().unwrap_or_default();
        gen_hash(&format!("{}{}", title, link))
    })
}

pub type Size64 = BuildHasherDefault<Size64Hasher>;

/// A specialized hasher for u64 and i64
///
/// WARNING: Do not use it for user-controlled input
#[derive(Default)]
pub struct Size64Hasher {
    finished: bool,
    value: u64,
}

impl Hasher for Size64Hasher {
    fn finish(&self) -> u64 {
        self.value
    }

    fn write(&mut self, _bytes: &[u8]) {
        panic!("only support u64 and i64");
    }

    fn write_u64(&mut self, i: u64) {
        assert!(
            !self.finished,
            "this is a special hasher, do not write twice"
        );
        self.value = i;
        self.finished = true;
    }

    fn write_i64(&mut self, i: i64) {
        self.write_u64(i as u64);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn rss_with_item(title: &str, ttl: Option<u32>, id: &str) -> feed::Rss {
        feed::Rss {
            title: title.to_owned(),
            ttl,
            items: vec![feed::Item {
                id: Some(id.to_owned()),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn database_with_feed(rss_link: &str, rss: &feed::Rss) -> Database {
        let feed_id = gen_hash(&rss_link);
        let mut feeds = HashMap::with_hasher(Size64::default());
        feeds.insert(
            feed_id,
            Feed {
                link: rss_link.to_owned(),
                title: rss.title.clone(),
                down_time: None,
                subscribers: HashSet::default(),
                ttl: rss.ttl,
                hash_list: rss.items.iter().map(gen_item_hash).collect(),
                hash_set: None,
            },
        );
        Database {
            path: PathBuf::new(),
            feeds,
            subscribers: HashMap::with_hasher(Size64::default()),
            dirty: false,
            dirty_generation: 0,
        }
    }

    #[test]
    fn serialize_reports_json_errors() {
        let rss_link = "https://example.com/feed.xml";
        let rss = rss_with_item("Example", Some(10), "item-1");
        let mut db = database_with_feed(rss_link, &rss);
        let feed_id = gen_hash(&rss_link);
        db.feeds.get_mut(&feed_id).unwrap().down_time =
            Some(std::time::UNIX_EPOCH - Duration::from_secs(1));

        assert!(db.serialize().is_err());
    }

    #[test]
    fn clear_dirty_if_generation_preserves_newer_mutations() {
        let rss = rss_with_item("Example", Some(10), "item-1");
        let mut db = database_with_feed("https://example.com/feed.xml", &rss);

        db.mark_dirty();
        let first_generation = db.dirty_generation();
        db.mark_dirty();

        assert!(!db.clear_dirty_if_generation(first_generation));
        assert!(db.is_dirty());

        assert!(db.clear_dirty_if_generation(db.dirty_generation()));
        assert!(!db.is_dirty());
    }

    #[test]
    fn update_resets_down_time_without_new_items() {
        let rss_link = "https://example.com/feed.xml";
        let rss = rss_with_item("Example", Some(10), "item-1");
        let mut db = database_with_feed(rss_link, &rss);
        let feed_id = gen_hash(&rss_link);
        db.feeds.get_mut(&feed_id).unwrap().down_time = Some(SystemTime::now());

        let updates = db.update(rss_link, rss);

        assert!(updates.is_empty());
        assert!(db.feeds.get(&feed_id).unwrap().down_time.is_none());
        assert!(db.is_dirty());
    }

    #[test]
    fn get_or_update_down_time_marks_dirty_when_first_set() {
        let rss_link = "https://example.com/feed.xml";
        let rss = rss_with_item("Example", Some(10), "item-1");
        let mut db = database_with_feed(rss_link, &rss);

        assert_eq!(db.get_or_update_down_time(rss_link), Some(Duration::default()));
        assert!(db.is_dirty());
    }

    #[test]
    fn reset_down_time_marks_dirty_when_value_changes() {
        let rss_link = "https://example.com/feed.xml";
        let rss = rss_with_item("Example", Some(10), "item-1");
        let mut db = database_with_feed(rss_link, &rss);
        let feed_id = gen_hash(&rss_link);
        db.feeds.get_mut(&feed_id).unwrap().down_time = Some(SystemTime::now());

        assert!(db.reset_down_time(rss_link));
        assert!(db.feeds.get(&feed_id).unwrap().down_time.is_none());
        assert!(db.is_dirty());
    }

    #[test]
    fn size64hasher() {
        let mut h = Size64Hasher::default();
        h.write_i64(-42);
        assert_eq!(h.finish(), -42i64 as u64);
    }

    #[test]
    #[should_panic(expected = "this is a special hasher, do not write twice")]
    fn size64hasher_write_twice() {
        let mut h = Size64Hasher::default();
        h.write_u64(42);
        h.write_u64(42);
    }

    #[test]
    #[should_panic(expected = "only support u64 and i64")]
    fn size64hasher_other_types() {
        let mut h = Size64Hasher::default();
        h.write_u8(0);
    }

    #[test]
    fn feed_hash_set_is_rebuilt_from_persisted_hash_list() {
        let rss = rss_with_item("Example", Some(10), "item-1");
        let feed = Feed {
            link: "https://example.com/feed.xml".to_string(),
            title: rss.title.clone(),
            down_time: None,
            subscribers: HashSet::default(),
            ttl: rss.ttl,
            hash_list: rss.items.iter().map(gen_item_hash).collect(),
            hash_set: None,
        };

        let json = serde_json::to_string(&feed).unwrap();
        let mut restored: Feed = serde_json::from_str(&json).unwrap();
        let existing_hash = restored.hash_list[0];

        assert!(restored.hash_set.is_none());
        assert!(restored.ensure_hash_set().contains(&existing_hash));
    }

    #[test]
    fn feed_hash_set_is_not_serialized() {
        let rss = rss_with_item("Example", Some(10), "item-1");
        let mut feed = Feed {
            link: "https://example.com/feed.xml".to_string(),
            title: rss.title.clone(),
            down_time: None,
            subscribers: HashSet::default(),
            ttl: rss.ttl,
            hash_list: rss.items.iter().map(gen_item_hash).collect(),
            hash_set: None,
        };

        feed.ensure_hash_set();

        let json = serde_json::to_value(&feed).unwrap();
        assert!(json.get("hash_list").is_some());
        assert!(json.get("hash_set").is_none());
    }

    #[test]
    fn subscribed_feeds_returns_link_title_summaries() {
        let rss_link = "https://example.com/feed.xml";
        let rss = rss_with_item("Example", Some(10), "item-1");
        let mut db = database_with_feed(rss_link, &rss);

        let subscriber: SubscriberId = 12345;
        let feed_id = gen_hash(&rss_link);

        db.subscribers
            .entry(subscriber)
            .or_default()
            .insert(feed_id);
        db.feeds
            .get_mut(&feed_id)
            .unwrap()
            .subscribers
            .insert(subscriber);

        let result = db.subscribed_feeds(subscriber);
        assert!(result.is_some());
        let summaries = result.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(
            summaries[0],
            FeedInfo {
                link: rss_link.to_string(),
                title: "Example".to_string(),
            }
        );
    }

    #[test]
    fn unsubscribe_returns_link_title_summary() {
        let rss_link = "https://example.com/feed.xml";
        let rss = rss_with_item("Example", Some(10), "item-1");
        let mut db = database_with_feed(rss_link, &rss);

        let subscriber: SubscriberId = 12345;
        let feed_id = gen_hash(&rss_link);

        db.subscribers
            .entry(subscriber)
            .or_default()
            .insert(feed_id);
        db.feeds
            .get_mut(&feed_id)
            .unwrap()
            .subscribers
            .insert(subscriber);

        let result = db.unsubscribe(subscriber, rss_link);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap(),
            FeedInfo {
                link: rss_link.to_string(),
                title: "Example".to_string(),
            }
        );
    }
}
