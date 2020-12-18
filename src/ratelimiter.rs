use borsh::{BorshDeserialize, BorshSerialize};
use sled::{Db, Tree};
use std::sync::{
  atomic::{AtomicU64, Ordering::SeqCst},
  Arc,
};
use time::{Duration, OffsetDateTime};

fn now() -> i64 {
  OffsetDateTime::now_utc().unix_timestamp()
}

pub struct RateLimiter {
  pub db: Db,
  pub store: Tree,
  pub limit: u64,
  pub count: Arc<AtomicU64>,
}

impl RateLimiter {
  pub fn setup_default() -> Self {
    let db = sled::open("./storage/rl.db").unwrap();
    let store = db.open_tree("rl").unwrap();
    let count = Arc::new(AtomicU64::new(store.len() as u64));
    RateLimiter {
      db,
      store,
      limit: 100_000,
      count,
    }
  }

  pub fn new(db: Db, store: Tree, limit: u64) -> Self {
    let count = Arc::new(AtomicU64::new(0));
    count.fetch_add(store.len() as u64, SeqCst);
    Self {
      db,
      store,
      limit,
      count,
    }
  }

  pub fn hit(&self, data: &[u8], threshhold: u32, dur: Duration) -> Option<RateLimited> {
    if let Some(mut rl) = self.get(data) {
      if self
        .store
        .insert(data, rl.hit(1, threshhold, dur).to_bin())
        .is_ok()
      {
        return Some(rl);
      }
    }
    let rl = RateLimited::new();
    if self.insert(data, rl.to_bin()) {
      return Some(rl);
    }
    None
  }

  pub fn forget(&self, data: &[u8]) -> bool {
    if let Ok(Some(_)) = self.store.remove(data).is_err() {
      self.count.fetch_sub(1, SeqCst);
      return true;
    }
    false
  }

  pub fn insert(&self, data: &[u8], entry: Vec<u8>) -> bool {
    if self.store.insert(data, entry).is_ok()
      && self.count.fetch_add(1, SeqCst) == self.limit
      && self.store.pop_min().is_ok()
    {
      self.count.fetch_sub(1, SeqCst);
      return true;
    }
    false
  }
  pub fn get(&self, data: &[u8]) -> Option<RateLimited> {
    match self.store.get(data) {
      Ok(rl) => rl.map(|raw| RateLimited::try_from_slice(&raw).unwrap()),
      Err(_) => None,
    }
  }

  pub fn has(&self, data: &str) -> bool {
    self.store.contains_key(data.as_bytes()).unwrap_or(false)
  }
}

#[derive(BorshDeserialize, BorshSerialize, Clone, PartialEq, Debug)]
pub struct RateLimited {
  hits: u32,
  lasthit: i64,
  timeout: i64,
}

impl RateLimited {
  pub fn new() -> Self {
    let now = now();
    Self {
      hits: 1,
      lasthit: now,
      timeout: now,
    }
  }

  pub fn to_bin(&self) -> Vec<u8> {
    self.try_to_vec().unwrap()
  }

  pub fn is_timing_out(&self) -> bool {
    now() < self.timeout
  }

  pub fn minutes_left(&self) -> i64 {
    self.duration_left().whole_minutes()
  }

  pub fn duration_left(&self) -> Duration {
    Duration::seconds(self.timeout - now())
  }

  pub fn hit(&mut self, hits: u32, threshhold: u32, dur: Duration) -> &mut RateLimited {
    let now = now();
    let dur_secs = dur.whole_seconds();
    self.hits += hits;
    if self.hits > 1 && now > self.timeout && (now - self.lasthit) > dur_secs {
      self.hits = 1;
      self.timeout = now;
    } else if self.hits > threshhold {
      self.timeout = now + (dur_secs * (self.hits - threshhold) as i64);
    }
    self.lasthit = now;
    self
  }
}
