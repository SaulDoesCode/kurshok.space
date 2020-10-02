use rand::{thread_rng, RngCore};
use sled::{transaction::*, Db, Tree};
use sthash::*;

use super::CONF;
use crate::ratelimiter::RateLimiter;
use crate::utils::generate_random_bytes;

pub struct Orchestrator {
  // auth
  pub db: Db,
  pub id_counter: Tree,
  pub users: Tree,
  pub usernames: Tree,
  pub user_descriptions: Tree,
  pub user_attributes: Tree,
  pub user_attributes_data: Tree,
  pub user_emails: Tree,
  pub user_secrets: Tree,
  pub handles: Tree,
  pub sessions: Tree,
  pub session_data: Tree,
  pub admins: Tree,
  pub ratelimiter: RateLimiter,
  pub expiry_tll: i64,
  pub dev_mode: bool,
  pub hasher: Hasher,
  pub pwd_secret: Vec<u8>,

  pub authcache: crate::auth::AuthCache,

  // writs
  pub writ_db: Db,
  pub writs: Tree,
  pub raw_content: Tree,
  pub content: Tree,
  pub kinds: Tree,
  pub titles: Tree, // title: writ_id
  pub slugs: Tree,
  pub dates: Tree, // {kind}:yyyy/mm/dd

  pub votes: Tree,       // writ_id: count
  pub writ_voters: Tree, // {user_id}:{writ_id} = {up_or_down, when}

  pub tags_index: Tree,
  pub tag_counter: Tree,
  // comments
  pub comment_settings: Tree,
  pub comment_key_path_index: Tree,
  pub comment_trees: Tree, // master_id-comment_id: {author}
  pub comments: Tree,      // master_id-comment_id: {author}
  pub comment_raw_content: Tree,
  pub comment_voters: Tree, // comment_id_user_id: {up_or_down, when}
  pub comment_votes: Tree,  // comment_id: {up, down, votes, when}
}

impl Orchestrator {
  pub fn new(expiry_tll: i64) -> Self {
    let db_loc = &CONF.read().db_location;
    let db = sled::Config::new()
      .path(&format!("{}main.db", db_loc))
      .use_compression(true)
      .compression_factor(20)
      .mode(sled::Mode::LowSpace)
      .flush_every_ms(Some(1000))
      .open()
      .expect("failed to open main.db");

    let id_counter = db.open_tree(b"id_counter").unwrap();
    let users = db.open_tree(b"users").unwrap();
    let usernames = db.open_tree(b"usernames").unwrap();
    let user_descriptions = db.open_tree(b"user_descriptions").unwrap();
    let user_emails = db.open_tree(b"user_emails").unwrap();
    let user_secrets = db.open_tree(b"user_secrets").unwrap();
    let user_attributes = db.open_tree(b"user_attributes").unwrap();
    let user_attributes_data = db.open_tree(b"user_attributes_data").unwrap();
    let handles = db.open_tree(b"handles").unwrap();
    let admins = db.open_tree(b"admins").unwrap();
    let sessions = db.open_tree(b"sessions").unwrap();
    let session_data = db.open_tree(b"session_data").unwrap();
    let secrets = db.open_tree(b"secrets").unwrap();

    let pwd_secret = {
      let scrt_res: TransactionResult<Vec<u8>, ()> = secrets.transaction(|s| {
        Ok(match s.get(b"pwd_secret")? {
          Some(scrt) => scrt.to_vec(),
          None => {
            let scrt = generate_random_bytes(64);
            s.insert(b"pwd_secret", scrt.clone())?;
            scrt
          }
        })
      });
      scrt_res.unwrap()
    };

    let hasher = if secrets.contains_key(b"hasher_seed").unwrap() {
      let seed = secrets.get(b"hasher_seed").unwrap().unwrap();
      Hasher::new(Key::from_seed(&seed, None), None)
    } else {
      let mut seed = [0; SEED_BYTES];
      thread_rng().fill_bytes(&mut seed);
      secrets.insert(b"hasher_seed", &seed).unwrap();
      db.flush().unwrap();
      Hasher::new(Key::from_seed(&seed, None), None)
    };

    let ratelimiter = RateLimiter::setup_default();

    let dev_mode = CONF.read().dev_mode;

    let writ_db_name = format!("{}writs.db", CONF.read().db_location);
    let writ_db = sled::Config::new()
      .path(&writ_db_name)
      .use_compression(true)
      .compression_factor(20)
      .mode(sled::Mode::LowSpace)
      .flush_every_ms(Some(1000))
      .open()
      .expect("failed to open writs.db");

    let writs = writ_db.open_tree("writs").unwrap();
    let raw_content = writ_db.open_tree("raw_content").unwrap();
    let content = writ_db.open_tree("content").unwrap();
    let tags_index = writ_db.open_tree("tags_index").unwrap();
    let tag_counter = writ_db.open_tree("tag_counter").unwrap();

    let slugs = writ_db.open_tree("slugs").unwrap();
    let kinds = writ_db.open_tree("kinds").unwrap();

    let titles = writ_db.open_tree("titles").unwrap();
    let comment_trees = writ_db.open_tree("comment_trees").unwrap();
    let comment_key_path_index = writ_db.open_tree("comment_key_path_index").unwrap();
    let comments = writ_db.open_tree("comments").unwrap();
    let comment_raw_content = writ_db.open_tree("comment_raw_content").unwrap();
    let comment_settings = writ_db.open_tree("comment_settings").unwrap();
    let writ_voters = writ_db.open_tree("writ_voters").unwrap();
    let comment_voters = writ_db.open_tree("comment_voters").unwrap();
    let votes = writ_db.open_tree("votes").unwrap();
    let comment_votes = writ_db.open_tree("comment_votes").unwrap();
    let dates = writ_db.open_tree("dates").unwrap();

    let authcache = crate::auth::AuthCache::new(5000, 5000);
    Orchestrator {
      db,
      id_counter,
      users,
      usernames,
      user_descriptions,
      user_emails,
      user_secrets,
      user_attributes,
      user_attributes_data,
      sessions,
      session_data,
      handles,
      admins,
      ratelimiter,
      expiry_tll,
      dev_mode,
      hasher,
      pwd_secret,

      authcache,

      writ_db,
      writs,
      raw_content,
      content,
      slugs,
      kinds,
      tags_index,
      tag_counter,
      titles,
      comment_trees,
      comment_key_path_index,
      comments,
      comment_raw_content,
      comment_settings,
      writ_voters,
      votes,
      comment_voters,
      comment_votes,
      dates,
    }
  }
}
