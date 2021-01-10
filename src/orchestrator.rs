use rand::{thread_rng, RngCore};
use sled::{Db, Tree};
use sthash::*;

use super::CONF;
use crate::ratelimiter::RateLimiter;

lazy_static! {
  pub static ref ORC: Orchestrator = Orchestrator::new(60 * 60 * 24 * 7 * 2);
}

pub struct Orchestrator {
  // auth
  pub db: Db,
  pub id_counter: Tree,
  pub users: Tree,
  pub usernames: Tree,
  pub emails: Tree,
  pub username_changes: Tree,
  pub email_changes: Tree,
  pub handle_changes: Tree,
  pub user_email_index: Tree,
  pub user_descriptions: Tree,
  pub user_attributes: Tree,
  pub user_attributes_data: Tree,
  pub user_verifications: Tree,
  pub magic_links: Tree,
  pub preauth_tokens: Tree,
  pub email_statuses: Tree,
  pub expirable_data: Tree,
  pub users_primed_for_auth: Tree,
  pub handles: Tree,
  pub sessions: Tree,
  pub session_data: Tree,
  pub admins: Tree,
  pub ratelimiter: RateLimiter,
  pub expiry_tll: i64,
  pub dev_mode: bool,
  pub hasher: Hasher,

  // writs
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
      .mode(sled::Mode::LowSpace)
      .use_compression(true)
      .compression_factor(20)
      .flush_every_ms(Some(1500))
      .open()
      .expect("failed to open main.db");

    let id_counter = db.open_tree(b"id_counter").unwrap();
    let users = db.open_tree(b"users").unwrap();

    let usernames = db.open_tree(b"usernames").unwrap();
    let emails = db.open_tree(b"emails").unwrap();

    let dev_mode = CONF.read().dev_mode;

    if dev_mode {
      db.drop_tree(b"username_changes").unwrap();
      db.drop_tree(b"handle_changes").unwrap();
    }

    let username_changes = db.open_tree(b"username_changes").unwrap();
    let email_changes = db.open_tree(b"email_changes").unwrap();
    let handle_changes = db.open_tree(b"handle_changes").unwrap();

    let user_descriptions = db.open_tree(b"user_descriptions").unwrap();
    let user_verifications = db.open_tree(b"user_verifications").unwrap();

    let user_email_index = db.open_tree(b"user_email_index").unwrap();

    let user_attributes = db.open_tree(b"user_attributes").unwrap();
    let user_attributes_data = db.open_tree(b"user_attributes_data").unwrap();
    let handles = db.open_tree(b"handles").unwrap();
    let admins = db.open_tree(b"admins").unwrap();
    let magic_links = db.open_tree(b"magic_links").unwrap();
    let preauth_tokens = db.open_tree(b"preauth_tokens").unwrap();
    let email_statuses = db.open_tree(b"email_statuses").unwrap();
    let expirable_data = db.open_tree(b"expirable_data").unwrap();
    let users_primed_for_auth = db.open_tree(b"users_primed_for_auth").unwrap();
    let sessions = db.open_tree(b"sessions").unwrap();
    let session_data = db.open_tree(b"session_data").unwrap();
    
    let secrets = db.open_tree(b"secrets").unwrap();

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

    let ratelimiter = RateLimiter::setup(&db);

    let writs = db.open_tree("writs").unwrap();
    let raw_content = db.open_tree("raw_content").unwrap();
    let content = db.open_tree("content").unwrap();
    let tags_index = db.open_tree("tags_index").unwrap();
    let tag_counter = db.open_tree("tag_counter").unwrap();

    let slugs = db.open_tree("slugs").unwrap();
    let kinds = db.open_tree("kinds").unwrap();

    let titles = db.open_tree("titles").unwrap();
    let comment_trees = db.open_tree("comment_trees").unwrap();
    let comment_key_path_index = db.open_tree("comment_key_path_index").unwrap();
    let comments = db.open_tree("comments").unwrap();
    let comment_raw_content = db.open_tree("comment_raw_content").unwrap();
    let comment_settings = db.open_tree("comment_settings").unwrap();
    let writ_voters = db.open_tree("writ_voters").unwrap();
    let comment_voters = db.open_tree("comment_voters").unwrap();
    let votes = db.open_tree("votes").unwrap();
    let comment_votes = db.open_tree("comment_votes").unwrap();
    let dates = db.open_tree("dates").unwrap();

    Orchestrator {
      db,
      id_counter,
      users,
      usernames,
      emails,
      username_changes,
      email_changes,
      handle_changes,
      user_email_index,
      user_verifications,
      user_descriptions,
      user_attributes,
      user_attributes_data,
      magic_links,
      preauth_tokens,
      email_statuses,
      expirable_data,
      users_primed_for_auth,
      sessions,
      session_data,
      handles,
      admins,
      ratelimiter,
      expiry_tll,
      dev_mode,
      hasher,

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
