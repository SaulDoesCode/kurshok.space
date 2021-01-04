use borsh::{BorshDeserialize, BorshSerialize};
use sled::{Db, Tree};
use time::{Duration, OffsetDateTime};

use crate::orchestrator::ORC;

fn now() -> i64 {
  OffsetDateTime::now_utc().unix_timestamp()
}

pub struct RateLimiter {
  pub store: Tree
}

impl RateLimiter {
  pub fn setup(db: &Db) -> Self {
    let store = db.open_tree("rl").expect("failed to open sled rl tree for ratelimiter");
    RateLimiter {store}
  }

  pub fn hit(&self, data: &[u8], threshhold: u32, dur: Duration) -> Option<RateLimited> {
    if let Some(mut rl) = self.get(data) {
      if self.store.insert(data, rl.hit(1, threshhold, &dur).to_bin()).is_ok() {
        ORC.expire_key(
          rl.seconds_left(),
          "rl".to_string(),
          data
        );
        return Some(rl);
      }
    }
    let rl = RateLimited::new();
    if self.insert(data, rl.to_bin()) {
      ORC.expire_key(
        rl.seconds_left(),
        "rl".to_string(),
        data
      );
      return Some(rl);
    }
    None
  }

  pub fn forget(&self, data: &[u8]) -> bool {
    if let Ok(Some(_)) = self.store.remove(data) {
      ORC.unexpire_key("rl".to_string(), data);
      return true;
    }
    false
  }

  fn insert(&self, data: &[u8], entry: Vec<u8>) -> bool {
    self.store.insert(data, entry).is_ok()
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

  pub fn seconds_left(&self) -> i64 {
    self.duration_left().whole_seconds()
  }

  pub fn duration_left(&self) -> Duration {
    Duration::seconds(self.timeout - now())
  }

  pub fn hit(&mut self, hits: u32, threshhold: u32, dur: &Duration) -> &mut RateLimited {
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
