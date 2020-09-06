use bincode::{Options};
use chrono::{prelude::*, Duration, offset::{Utc}};
use serde::{Serialize, Deserialize};
use sled::{Tree};
use std::sync::{atomic::{AtomicU64, Ordering::{SeqCst}}, Arc};

lazy_static!{
  pub static ref BINCODE_BIGENDIAN: bincode::config::WithOtherEndian<bincode::DefaultOptions, bincode::config::BigEndian> = bincode::DefaultOptions::new().with_big_endian();
}

pub struct RateLimiter {
  store: Tree,
  limit: u64,
  count: Arc<AtomicU64>,
}

impl RateLimiter {
  pub fn setup_default() -> Self {
    let rl_db = sled::open("./storage/rl.db").unwrap();
    let store = rl_db.open_tree("rl").unwrap();
    let count = Arc::new(AtomicU64::new(0));
    count.fetch_add(store.len() as u64, SeqCst);
    RateLimiter{store, limit: 100_000, count}
  }

  pub fn new(store: Tree, limit: u64) -> Self {
    let count = Arc::new(AtomicU64::new(0));
    count.fetch_add(store.len() as u64, SeqCst);
    Self{store, limit, count}
  }

  pub fn hit(&self, data: &[u8], threshhold: u64, dur: Duration) -> RateLimited {
    if let Some(mut rl) = self.get(data) {
      rl.hit(1, threshhold, dur);
      self.store.insert(data, rl.to_bin()).unwrap();
      return rl;
    }
    let rl = RateLimited::new();
    self.insert(data, rl.to_bin());
    return rl;
  }

  pub fn forget(&self, data: &[u8]) {
    if let Some(_) = self.store.remove(data).unwrap() {
      self.count.fetch_sub(1, SeqCst);
    }
  }
  
  pub fn insert(&self, data: &[u8], entry: Vec<u8>) {
    self.store.insert(data, entry).unwrap();
    if self.count.fetch_add(1, SeqCst) == self.limit {
      if self.store.pop_min().is_ok() {
        self.count.fetch_sub(1, SeqCst);
      }
    }
  }
  
  pub fn get(&self, data: &[u8]) -> Option<RateLimited> {
    if let Some(v) = self.store.get(data).unwrap() {
      return Some(BINCODE_BIGENDIAN.deserialize(&v).unwrap());
    }
    None
  }

  pub fn has(&self, data: &str) -> bool {
    self.store.contains_key(data.clone().as_bytes()).unwrap()
  }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct RateLimited {
  hits: u64,
  lasthit: DateTime<Utc>,
  timeout: DateTime<Utc>,
}

impl RateLimited {
  pub fn new() -> Self {
    let now = Utc::now();
    Self{hits: 1, lasthit: now, timeout: now}
  }

  pub fn to_bin(&self) -> Vec<u8> {
    BINCODE_BIGENDIAN.serialize(self).unwrap()
  }

  pub fn is_timing_out(&self) -> bool {
    Utc::now() < self.timeout
  }

  pub fn minutes_left(&self) -> i64 {
    self.duration_left().num_minutes()
  }

  pub fn duration_left(&self) -> Duration {
    self.timeout - Utc::now()
  }

  pub fn hit(&mut self, hits: u64, threshhold: u64, dur: Duration) -> &mut RateLimited {
    let now = Utc::now();
    self.hits += hits;
    if self.hits > 1 && now > self.timeout && (now - self.lasthit) > dur {
      self.hits = 1;
      self.timeout = now;
    } else if self.hits > threshhold {
      self.timeout = now + (dur * ((self.hits - threshhold) as i32));
    }
    self.lasthit = now;
    self
  }
}