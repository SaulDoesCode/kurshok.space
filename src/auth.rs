use actix_web::{
  cookie::Cookie, delete, get, post, web, HttpMessage, HttpRequest, HttpResponse, Responder,
};
use argon2::Config;
use borsh::{BorshDeserialize, BorshSerialize};
use dashmap::{DashMap, ElementGuard};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sled::{transaction::*, IVec, Transactional};
use slug::slugify;
use thiserror::Error;
use time::Duration;

use std::sync::{
  atomic::{AtomicU64, Ordering::SeqCst},
  Arc,
};

use super::CONF;
use crate::orchestrator::Orchestrator;
use crate::utils::{
  is_email_ok,
  // datetime_from_unix_timestamp,
  is_password_ok,
  is_username_ok,
  random_string,
  unix_timestamp,
  FancyBool,
  FancyIVec,
};

impl Orchestrator {
  pub fn hash(&self, data: &[u8]) -> Vec<u8> {
    self.hasher.hash(data)
  }

  pub fn generate_id(&self, key: &[u8]) -> TransactionResult<u64, ()> {
    self.id_counter.transaction(|idc| {
      let id = match idc.get(key)? {
        Some(count) => count.to_u64() + 1,
        None => 0,
      };
      idc.insert(key, IVec::from_u64(id))?;
      Ok(id)
    })
  }

  pub fn create_user(&self, ar: AuthRequest) -> Option<User> {
    if !is_username_ok(ar.username.as_str()) {
      return None;
    }
    if !is_password_ok(ar.password.as_str()) {
      return None;
    }

    let pwd = match self.hash_pwd(&ar.password) {
      Some(p) => p,
      None => return None,
    };

    let user_id = match self.generate_id("usr".as_bytes()) {
      Ok(id) => format!("{}", id),
      Err(_) => return None,
    };

    let email = ar
      .email
      .and_then(|email| is_email_ok(&email).qualify(email));

    let usr = User {
      id: user_id.clone(),
      username: ar.username.clone(),
      handle: ar.handle.unwrap_or(slugify(ar.username)),
      reg: unix_timestamp(),
    };

    if (
      &self.users,
      &self.usernames,
      &self.user_secrets,
      &self.user_emails,
      &self.handles,
    )
      .transaction(|(users, usernames, usr_secrets, usr_emails, handles)| {
        if usernames.get(usr.username.as_bytes())?.is_some()
          || handles.get(usr.handle.as_bytes())?.is_some()
        {
          return Err(sled::transaction::ConflictableTransactionError::Abort(()));
        }

        users.insert(user_id.as_bytes(), usr.try_to_vec().unwrap())?;
        usr_secrets.insert(user_id.as_bytes(), pwd.as_bytes())?;
        if let Some(email) = &email {
          usr_emails.insert(user_id.as_bytes(), email.as_bytes())?;
        }
        usernames.insert(usr.username.as_bytes(), user_id.as_bytes())?;
        handles.insert(usr.handle.as_bytes(), user_id.as_bytes())?;
        Ok(())
      })
      .is_ok()
    {
      return Some(usr);
    }

    None
  }

  pub fn authenticate(&self, username: &str, pwd: &str) -> Option<ElementGuard<String, User>> {
    if let Ok(Some(usr_id)) = self.usernames.get(username.as_bytes()) {
      if let Ok(Some(usr_pwd)) = self.user_secrets.get(&usr_id) {
        if verify_password(pwd, usr_pwd.to_str()) {
          return self.user_by_id(usr_id.to_str());
        }
      }
    }
    None
  }

  pub fn setup_session(&self, usr_id: String) -> Result<String, AuthError> {
    let sess_id = format!("{}:{}", &usr_id, random_string(20));
    let timestamp = unix_timestamp();
    if self.sessions.scan_prefix(usr_id.as_bytes()).any(|r| {
      r.map_or(false, |(k, v)| {
        let ses = UserSession::try_from_slice(&v).unwrap();
        if ses.has_expired() {
          let res: TransactionResult<(), ()> =
            (&self.sessions, &self.users).transaction(|(sess, _users)| {
              sess.remove(&k)?;
              Ok(())
            });
          return !res.is_ok();
        }
        false
      })
    }) {
      return Err(AuthError::SessionRemovalErr);
    }

    let session = UserSession {
      usr_id,
      timestamp,
      exp: timestamp + time::Duration::weeks(2).whole_seconds(),
    };

    match self
      .sessions
      .insert(sess_id.as_bytes(), session.try_to_vec().unwrap())
    {
      Ok(_) => {
        self.authcache.cache_session(sess_id.clone(), session);
        return Ok(sess_id);
      }
      Err(e) => {
        if self.dev_mode {
          println!("sessions.insert error: {:?}", e);
        }
        return Err(AuthError::DBIssue);
      }
    }
  }

  pub fn get_session(&self, id: &String) -> Option<ElementGuard<String, UserSession>> {
    if let Some(el) = self.authcache.sessions.get(id) {
      return Some(el);
    } else if let Ok(Some(raw)) = self.sessions.get(id.as_bytes()) {
      let session = UserSession::try_from_slice(&raw).unwrap();
      if session.has_expired() {
        if let Err(e) = self.sessions.remove(id.as_bytes()) {
          println!("removing expired session from session tree failed: {}", e);
        }
        return None;
      }
      return Some(self.authcache.cache_session(id.clone(), session));
    }
    None
  }

  pub fn is_admin(&self, usr_id: &str) -> bool {
    self.admins.contains_key(usr_id.as_bytes()).unwrap_or(false)
  }

  pub fn user_by_session(&self, req: &HttpRequest) -> Option<ElementGuard<String, User>> {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value().to_string();
      if let Some(session) = self.get_session(&sess_id) {
        return self.user_by_id(&session.usr_id);
      }
    }
    None
  }

  pub fn user_id_by_session(&self, req: &HttpRequest) -> Option<String> {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value().to_string();
      if let Some(session) = self.get_session(&sess_id) {
        return Some(session.usr_id.clone());
      }
    }
    None
  }
  pub fn user_by_session_renew<'c>(
    &self,
    req: &HttpRequest,
    how_far_to_expiry: Duration,
  ) -> (
    Option<dashmap::ElementGuard<String, User>>,
    Option<Cookie<'c>>,
  ) {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value().to_string();
      if let Some(session) = self.get_session(&sess_id) {
        let mut cookie: Option<Cookie> = None;
        if session.close_to_expiry(how_far_to_expiry) {
          let usr_id = session.usr_id.clone();
          self.authcache.remove_session(&sess_id);
          if self.sessions.remove(sess_id.as_bytes()).is_ok() {
            if let Ok(sess_id) = self.setup_session(usr_id) {
              cookie = Some(if !self.dev_mode {
                Cookie::build("auth", sess_id)
                  .domain(CONF.read().domain.clone())
                  .max_age(time::Duration::seconds(self.expiry_tll))
                  .http_only(true)
                  .secure(true)
                  .finish()
              } else {
                Cookie::build("auth", sess_id)
                  .http_only(true)
                  .max_age(time::Duration::seconds(self.expiry_tll))
                  .finish()
              })
            }
          }
        }
        let o_usr = self.user_by_id(&session.usr_id);
        if o_usr.is_some() {
          return (o_usr, cookie);
        }
      }
    }
    (None, None)
  }

  pub fn admin_by_session(&self, req: &HttpRequest) -> Option<ElementGuard<String, User>> {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value().to_string();
      if let Some(session) = self.get_session(&sess_id) {
        if self.is_admin(&session.usr_id) {
          return self.user_by_id(&session.usr_id);
        }
      }
    }
    None
  }

  pub fn is_valid_admin_session(&self, req: &HttpRequest) -> bool {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value().to_string();
      if let Some(session) = self.get_session(&sess_id) {
        return self.is_admin(&session.usr_id);
      }
    }
    false
  }

  pub fn is_valid_session(&self, req: &HttpRequest) -> bool {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value().to_string();
      return self.get_session(&sess_id).is_some();
    }
    false
  }

  pub fn user_by_id(&self, id: &str) -> Option<ElementGuard<String, User>> {
    if let Some(el) = self.authcache.users.get(&id.to_string()) {
      return Some(el);
    } else if let Ok(Some(raw)) = self.users.get(id.as_bytes()) {
      let el = self
        .authcache
        .cache_user(User::try_from_slice(&raw).unwrap());
      return Some(el);
    }
    None
  }

  pub fn admin_by_id(&self, id: &str) -> Option<ElementGuard<String, User>> {
    if self.is_admin(id) {
      return self.user_by_id(id);
    }
    None
  }

  pub fn user_by_username(&self, username: &str) -> Option<ElementGuard<String, User>> {
    if let Ok(Some(user_id)) = self.usernames.get(username.as_bytes()) {
      return self.user_by_id(user_id.to_str());
    }
    None
  }

  pub fn user_by_handle(&self, handle: &str) -> Option<ElementGuard<String, User>> {
    if let Ok(Some(user_id)) = self.handles.get(handle.as_bytes()) {
      return self.user_by_id(user_id.to_str());
    }
    None
  }

  pub fn username_taken(&self, username: &str) -> bool {
    (&self.usernames).contains_key(username.as_bytes()).unwrap()
  }

  pub fn handle_taken(&self, handle: &str) -> bool {
    (&self.handles).contains_key(handle.as_bytes()).unwrap()
  }

  pub fn change_username(&self, usr: &mut User, new_username: &str) -> bool {
    if self.username_taken(new_username) {
      return false;
    }

    let old_username = usr.username.clone();
    usr.username = new_username.to_string();

    let res: TransactionResult<(), ()> =
      (&self.users, &self.usernames).transaction(|(users, usernames)| {
        users.insert(usr.id.as_bytes(), usr.try_to_vec().unwrap())?;
        usernames.remove(old_username.as_bytes())?;
        usernames.insert(new_username.as_bytes(), usr.id.as_bytes())?;
        Ok(())
      });
    res.is_ok()
  }

  pub fn change_handle(&self, usr: &mut User, new_handle: &str) -> bool {
    if self.handle_taken(new_handle) {
      return false;
    }

    let old_handle = usr.handle.clone();
    usr.handle = new_handle.to_string();
    let res: TransactionResult<(), ()> =
      (&self.users, &self.handles).transaction(|(users, handles)| {
        users.insert(usr.id.as_bytes(), usr.try_to_vec().unwrap())?;
        handles.remove(old_handle.as_bytes())?;
        handles.insert(new_handle.as_bytes(), usr.id.as_bytes())?;
        Ok(())
      });
    res.is_ok()
  }

  pub fn change_description(&self, usr: &mut User, new_desc: &str) -> bool {
    new_desc.len() > 1
      && self
        .user_descriptions
        .insert(usr.id.as_bytes(), new_desc.as_bytes())
        .is_ok()
  }

  pub fn change_password(&self, usr: &mut User, new_pwd: &str) -> bool {
    if !is_password_ok(new_pwd) {
      return false;
    }
    self.hash_pwd(new_pwd).map_or(false, |p| {
      let res: TransactionResult<(), ()> =
        (&self.user_secrets, &self.users).transaction(|(user_secrets, _users)| {
          user_secrets.insert(usr.id.as_bytes(), p.as_bytes())?;
          Ok(())
        });
      res.is_ok()
    })
  }

  pub fn make_admin(&self, usr_id: &str, level: u8, reason: Option<String>) -> bool {
    let attr = UserAttribute {
      aquired: unix_timestamp(),
      reason,
    };
    let res: TransactionResult<(), ()> =
      (&self.user_attributes, &self.admins).transaction(|(usr_attrs, admins)| {
        let key = format!("{}:admin", usr_id);
        usr_attrs.insert(key.as_bytes(), attr.try_to_vec().unwrap())?;
        admins.insert(usr_id.as_bytes(), &[level])?;
        Ok(())
      });
    res.is_ok()
  }

  pub fn change_admin_level(&self, usr_id: &str, level: u8) -> bool {
    let res: TransactionResult<(), ()> =
      (&self.user_attributes, &self.admins).transaction(|(usr_attrs, admins)| {
        let key = format!("{}:admin", usr_id);
        if let Some(_) = usr_attrs.get(key.as_bytes())? {
          admins.insert(usr_id.as_bytes(), &[level])?;
          return Ok(());
        }
        Err(sled::transaction::ConflictableTransactionError::Abort(()))
      });
    res.is_ok()
  }

  pub fn has_admin_level(&self, usr_id: &str, level: u8) -> bool {
    if let Ok(Some(levels)) = self.admins.get(usr_id.as_bytes()) {
      return levels[0] == level;
    }
    false
  }

  pub fn revoke_admin(&self, usr_id: &str) -> bool {
    let res: TransactionResult<(), ()> =
      (&self.user_attributes, &self.admins).transaction(|(usr_attrs, admins)| {
        let key = format!("{}:admin", usr_id);
        usr_attrs.remove(key.as_bytes())?;
        admins.remove(usr_id.as_bytes())?;
        Ok(())
      });
    res.is_ok()
  }

  pub fn bestow_mere_attributes(&self, usr_id: &str, attrs: Vec<String>) -> bool {
    let res: TransactionResult<(), ()> = (&self.user_attributes, &self.user_attributes_data)
      .transaction(|(usr_attrs, usr_attrs_data)| {
        for attr in &attrs {
          let key = format!("{}:{}", usr_id, attr);
          usr_attrs.insert(
            key.as_bytes(),
            UserAttribute::default().try_to_vec().unwrap(),
          )?;
          usr_attrs_data.remove(key.as_bytes())?;
        }
        // return Err(sled::transaction::ConflictableTransactionError::Abort(()));
        Ok(())
      });
    res.is_ok()
  }

  pub fn bestow_attributes(
    &self,
    usr_id: &str,
    attrs: Vec<(String, UserAttribute, Option<Vec<u8>>)>,
  ) -> bool {
    let res: TransactionResult<(), ()> = (&self.user_attributes, &self.user_attributes_data)
      .transaction(|(usr_attrs, usr_attrs_data)| {
        for (name, attr, data) in &attrs {
          let key = format!("{}:{}", usr_id, name);
          usr_attrs.insert(key.as_bytes(), attr.try_to_vec().unwrap())?;
          if let Some(data) = data {
            usr_attrs_data.insert(key.as_bytes(), data.as_slice())?;
          }
        }
        Ok(())
      });
    res.is_ok()
  }

  pub fn bestow_attribute(
    &self,
    usr_id: &str,
    name: String,
    attr: UserAttribute,
    attr_data: Option<Vec<u8>>,
  ) -> bool {
    let res: TransactionResult<(), ()> = (&self.user_attributes, &self.user_attributes_data)
      .transaction(|(usr_attrs, usr_attrs_data)| {
        let key = format!("{}:{}", usr_id, name);
        usr_attrs.insert(key.as_bytes(), attr.try_to_vec().unwrap())?;
        if let Some(data) = &attr_data {
          usr_attrs_data.insert(key.as_bytes(), data.as_slice())?;
        }
        Ok(())
      });
    res.is_ok()
  }

  pub fn strip_attributes(&self, usr_id: &str, attrs: Vec<String>) -> bool {
    let res: TransactionResult<(), ()> = self.user_attributes.transaction(|usr_attrs| {
      for attr in &attrs {
        let key = format!("{}:{}", usr_id, attr);
        usr_attrs.remove(key.as_bytes())?;
      }
      Ok(())
    });
    res.is_ok()
  }

  pub fn user_attributes(&self, usr_id: &str) -> Vec<String> {
    let prefix = format!("{}:", usr_id);
    self
      .user_attributes
      .scan_prefix(usr_id.as_bytes())
      .keys()
      .filter_map(|res| {
        res.map_or(None, |key| {
          let raw_attr = key.to_string();
          let attr = raw_attr.trim_start_matches(&prefix);
          Some(attr.to_string())
        })
      })
      .collect()
  }

  pub fn get_user_attribute(&self, usr_id: &str, attr: &str) -> Option<UserAttribute> {
    let key = format!("{}:{}", usr_id, attr);
    if let Ok(Some(raw)) = self.user_attributes.get(key.as_bytes()) {
      return Some(UserAttribute::try_from_slice(&raw).unwrap());
    }
    None
  }

  pub fn user_has_attrs(&self, usr_id: &str, attrs: &[&str]) -> Option<bool> {
    for attr in attrs {
      let key = format!("{}:{}", usr_id, attr);
      if let Ok(has_attr) = self.user_attributes.contains_key(key.as_bytes()) {
        if !has_attr {
          return Some(false);
        }
      } else {
        return None;
      }
    }
    Some(true)
  }

  pub fn user_has_some_attrs(&self, usr_id: &str, attrs: &[&str]) -> Option<bool> {
    for attr in attrs {
      let key = format!("{}:{}", usr_id, attr);
      if let Ok(has_attr) = self.user_attributes.contains_key(key.as_bytes()) {
        if has_attr {
          return Some(true);
        }
      } else {
        return None;
      }
    }
    Some(false)
  }

  pub fn user_attribute_with_data(
    &self,
    usr_id: &str,
    attr: &str,
  ) -> Option<(UserAttribute, sled::IVec)> {
    let key = format!("{}:{}", usr_id, attr);
    if let Ok(Some(raw_attr)) = self.user_attributes.get(key.as_bytes()) {
      if let Ok(Some(raw_data)) = self.user_attributes_data.get(key.as_bytes()) {
        return Some((UserAttribute::try_from_slice(&raw_attr).unwrap(), raw_data));
      }
    }
    None
  }

  pub fn hash_pwd(&self, pwd: &str) -> Option<String> {
    hash_pwd(&self.pwd_secret, pwd)
  }
}

lazy_static! {
  static ref ARGON_CONFIG: argon2::Config<'static> = {
    Config {
      variant: argon2::Variant::Argon2i,
      version: argon2::Version::Version13,
      mem_cost: 65536,
      time_cost: 5,
      lanes: 2,
      thread_mode: argon2::ThreadMode::Sequential,
      secret: &[],
      ad: &[],
      hash_length: 32,
    }
  };
}

pub fn hash_pwd(pwd_secret: &Vec<u8>, pwd: &str) -> Option<String> {
  if let Ok(hash) = argon2::hash_encoded(pwd.as_bytes(), pwd_secret, &ARGON_CONFIG) {
    return Some(hash);
  }
  None
}

pub fn verify_password(pwd: &str, hash: &str) -> bool {
  argon2::verify_encoded(hash, pwd.as_bytes()).unwrap_or(false)
}

pub struct AuthCache {
  user_count: Arc<AtomicU64>,
  max_user_count: u64,
  session_count: Arc<AtomicU64>,
  max_session_count: u64,
  pub users: DashMap<String, crate::auth::User>,
  pub sessions: DashMap<String, crate::auth::UserSession>,
}

impl AuthCache {
  pub fn new(max_user_count: u64, max_session_count: u64) -> Self {
    AuthCache {
      user_count: Arc::new(AtomicU64::new(0)),
      max_user_count,
      session_count: Arc::new(AtomicU64::new(0)),
      max_session_count,
      users: DashMap::new(),
      sessions: DashMap::new(),
    }
  }

  pub fn cache_user(&self, user: User) -> ElementGuard<String, User> {
    let el = self.users.insert_and_get(user.id.clone(), user);
    let count = self.user_count.fetch_add(1, SeqCst) + 1;
    if count > self.max_user_count {
      self.pop_user();
    }
    el
  }

  pub fn remove_user(&self, id: &String) -> bool {
    if self.users.remove(id) {
      self.user_count.fetch_sub(1, SeqCst);
      return true;
    }
    false
  }

  pub fn pop_user(&self) -> bool {
    if let Some(el) = self.users.iter().last() {
      return self.remove_user(el.key());
    }
    false
  }

  pub fn cache_session(
    &self,
    id: String,
    session: UserSession,
  ) -> ElementGuard<String, UserSession> {
    let el = self.sessions.insert_and_get(id, session);
    let count = self.session_count.fetch_add(1, SeqCst) + 1;
    if count > self.max_session_count {
      self.pop_session();
    }
    el
  }

  pub fn remove_session(&self, id: &String) -> bool {
    if self.sessions.remove(id) {
      self.session_count.fetch_sub(1, SeqCst);
      return true;
    }
    false
  }

  pub fn pop_session(&self) -> bool {
    if let Some(el) = self.sessions.iter().last() {
      return self.remove_session(el.key());
    }
    false
  }
}

#[derive(Error, Debug)]
pub enum AuthError {
  #[error("id does not match any currently existing user")]
  BadID,
  #[error("id generation failed for some reason, maybe try again later")]
  IDGenErr,
  #[error("there was a problem interacting with the db")]
  DBIssue,
  #[error("too many requests to Auth API, chill for a bit")]
  RateLimit,
  #[error("something fishy going on with session")]
  SessionErr,
  #[error("ran into trouble removing bad or expired sessions")]
  SessionRemovalErr,
  #[error("unknown auth error")]
  Unknown,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Debug)]
pub struct User {
  pub id: String,
  pub username: String,
  pub handle: String,
  pub reg: i64, // registration date
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Debug)]
pub struct UserAttribute {
  pub aquired: i64,
  pub reason: Option<String>,
}

impl Default for UserAttribute {
  fn default() -> Self {
    Self {
      aquired: unix_timestamp(),
      reason: None,
    }
  }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Debug)]
pub struct UserSession {
  usr_id: String,
  timestamp: i64,
  exp: i64,
}

impl UserSession {
  pub fn has_expired(&self) -> bool {
    unix_timestamp() > self.exp
  }

  pub fn close_to_expiry(&self, with_duration: time::Duration) -> bool {
    unix_timestamp() + with_duration.whole_seconds() > self.exp
  }
}

#[derive(Serialize, Deserialize)]
pub struct AuthRequest {
  username: String,
  password: String,
  handle: Option<String>,
  email: Option<String>,
}

#[delete("/auth")]
pub async fn logout(req: HttpRequest, orc: web::Data<Arc<Orchestrator>>) -> HttpResponse {
  let mut status = "successfully logged out";
  if let Some(auth_cookie) = req.cookie("auth") {
    let sess_id = auth_cookie.value().to_string();
    if orc.sessions.remove(sess_id.as_bytes()).is_err() || !orc.authcache.remove_session(&sess_id) {
      status = "login was already bad or expired, no worries";
    }
  }
  let mut res = crate::responses::Accepted(status);
  res.del_cookie("auth");
  res
}

pub fn renew_session_cookie<'c>(
  req: &HttpRequest,
  how_far_to_expiry: Duration,
  orc: web::Data<Arc<Orchestrator>>,
) -> Option<Cookie<'c>> {
  if let Some(auth_cookie) = req.cookie("auth") {
    let sess_id = auth_cookie.value();
    if let Ok(Some(raw)) = orc.sessions.get(sess_id.as_bytes()) {
      let session = UserSession::try_from_slice(&raw).unwrap();
      if session.close_to_expiry(how_far_to_expiry) {
        if orc.sessions.remove(sess_id.as_bytes()).is_ok() {
          if let Ok(sess_id) = orc.setup_session(session.usr_id) {
            return Some(if !orc.dev_mode {
              Cookie::build("auth", sess_id.clone())
                .domain(CONF.read().domain.clone())
                .max_age(time::Duration::seconds(orc.expiry_tll))
                .http_only(true)
                .secure(true)
                .finish()
            } else {
              Cookie::build("auth", sess_id.clone())
                .http_only(true)
                .max_age(time::Duration::seconds(orc.expiry_tll))
                .finish()
            });
          }
        }
      }
    }
  }
  None
}

pub async fn login(
  usr_id: String,
  first_time: bool,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  let token = match orc.setup_session(usr_id) {
    Ok(t) => t,
    Err(e) => {
      return crate::responses::Forbidden(format!("trouble setting up session: {}", e));
    }
  };

  let cookie = if !orc.dev_mode {
    Cookie::build("auth", token)
      .domain(CONF.read().domain.clone())
      .max_age(time::Duration::seconds(orc.expiry_tll))
      .http_only(true)
      .secure(true)
      .finish()
  } else {
    Cookie::build("auth", token)
      .http_only(true)
      .max_age(time::Duration::seconds(orc.expiry_tll))
      .finish()
  };

  HttpResponse::Accepted().cookie(cookie).json(json!({
    "ok": true,
    "data": "successfully logged in",
    "status": {
      "first_time": first_time
    }
  }))
}

#[get("/auth")]
pub async fn check_authentication(
  req: HttpRequest,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  if orc.is_valid_session(&req) {
    return crate::responses::Accepted("authenticated");
  }
  crate::responses::Forbidden("not authenticated")
}

#[post("/auth")]
pub async fn auth_attempt(
  req: HttpRequest,
  ar: web::Json<AuthRequest>,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  let (username, pwd) = (ar.username.as_str(), ar.password.as_str());

  if !is_username_ok(username) {
    return crate::responses::BadRequest(
      "username is no good, it's either too long, too short, or has weird characters in it, fix it up and try again."
    );
  }
  if !is_password_ok(pwd) {
    return crate::responses::BadRequest("malformed password");
  }

  if let Some(usr) = orc.user_by_session(&req) {
    return crate::responses::Accepted(format!(
      "Hey {}, you're already authenticated.",
      usr.username
    ));
  }

  let hitter = req
    .peer_addr()
    .map_or(username.to_string(), |a| format!("{}{}", username, a));
  if let Some(rl) = orc
    .ratelimiter
    .hit(hitter.as_bytes(), 3, Duration::minutes(2))
  {
    if rl.is_timing_out() {
      return crate::responses::TooManyRequests(format!(
        "Too many requests, timeout has {} minutes left.",
        rl.minutes_left()
      ));
    }
  }

  if orc.username_taken(username) {
    if let Some(usr) = orc.authenticate(username, pwd) {
      orc.ratelimiter.forget(hitter.as_bytes());
      return login(usr.id.clone(), false, orc.clone()).await;
    }
    return crate::responses::BadRequest(
      "either your password is wrong or the username is already taken.",
    );
  }

  if let Some(usr) = orc.create_user(ar.into_inner()) {
    orc.ratelimiter.forget(hitter.as_bytes());
    return login(usr.id.clone(), true, orc).await;
  }

  crate::responses::Forbidden("not working, we might be under attack")
}

#[derive(Serialize, Deserialize)]
pub struct AdministralityRequest {
  key: String,
}

#[post("/administrality")]
pub async fn administer_administrality(
  req: HttpRequest,
  ar: web::Json<AdministralityRequest>,
  orc: web::Data<Arc<Orchestrator>>,
) -> impl Responder {
  if CONF.read().admin_key == ar.key {
    if let Some(ref mut usr) = orc.user_by_session(&req) {
      if orc.make_admin(&usr.id, 0, Some("passed administrality".to_string())) {
        return crate::responses::Accepted("Congratulations, you are now an admin.");
      }
      return crate::responses::InternalServerError(format!(
        "Sorry {}, you got it right, but there was a database error. Try again later. ;D",
        usr.username
      ));
    } else if let Some(remote_addr) = req.connection_info().remote_addr() {
      let hitter = format!("aA{}", remote_addr);
      if let Some(rl) = orc
        .ratelimiter
        .hit(hitter.as_bytes(), 2, Duration::minutes(60))
      {
        if rl.is_timing_out() {
          return crate::responses::TooManyRequests(format!(
            "too many requests, timeout has {} minutes left.",
            rl.minutes_left()
          ));
        }
      }
    }
  } else if let Some(auth_cookie) = req.cookie("auth") {
    let hitter = format!("aA{}", auth_cookie.value());
    if let Some(rl) = orc
      .ratelimiter
      .hit(hitter.as_bytes(), 2, Duration::minutes(60))
    {
      if rl.is_timing_out() {
        return crate::responses::TooManyRequests(format!(
          "too many requests, timeout has {} minutes left.",
          rl.minutes_left()
        ));
      }
    }
  } else if let Some(remote_addr) = req.connection_info().remote_addr() {
    let hitter = format!("aA{}", remote_addr);
    if let Some(rl) = orc
      .ratelimiter
      .hit(hitter.as_bytes(), 2, Duration::minutes(60))
    {
      if rl.is_timing_out() {
        return crate::responses::TooManyRequests(format!(
          "too many requests, timeout has {} minutes left.",
          rl.minutes_left()
        ));
      }
    }
  }

  crate::responses::Forbidden("Sorry, no administrality for you.")
}

#[delete("/administrality")]
pub async fn remove_administrality(
  req: HttpRequest,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  if let Some(ref mut usr) = orc.admin_by_session(&req) {
    if orc.revoke_admin(&usr.id) {
      return crate::responses::Accepted(format!(
        "Sorry {}, you've lost your adminstrality.",
        usr.username
      ));
    }
  }
  crate::responses::Forbidden("To lose administrality one needs to have some in the first place!")
}

#[get("/administrality")]
pub async fn check_administrality(
  req: HttpRequest,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  if orc.is_valid_admin_session(&req) {
    return crate::responses::Accepted("Genuine admin");
  }
  crate::responses::Forbidden("Silly impostor, you are not admin")
}
