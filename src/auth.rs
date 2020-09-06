use argon2::Config;
use chrono::{offset::Utc, prelude::*, Duration};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sled::{Transactional, transaction::*};
use slug::slugify;

use std::sync::Arc;

use actix_web::{get, post, delete, web, HttpMessage, HttpRequest, HttpResponse, Responder, cookie::Cookie};

use super::CONF;
use crate::orchestrator::Orchestrator;
use crate::utils::{
  get_struct,
  is_password_ok,
  is_email_ok,
  is_username_ok,
  random_string,
  IntoBin,
  binbe_serialize,
  binbe_deserialize,
  FancyBool,
  FancyIVec
};

impl Orchestrator {
  pub fn hash(&self, data: &[u8]) -> Vec<u8> {
    self.hasher.hash(data)
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

    let user_id = match self.db.generate_id() {
      Ok(id) => format!("{}", id),
      Err(_) => return None,
    };

    let email = ar.email.and_then(|email| is_email_ok(&email).qualify(email));

    let usr = User{
      id: user_id.clone(),
      username: ar.username.clone(),
      handle: ar.handle.unwrap_or(slugify(ar.username.to_lowercase())),
      registered: Utc::now(),
      email,
    };

    if (&self.users, &self.usernames, &self.user_secrets, &self.handles).transaction(|(users, usernames, user_secrets, handles)| {
      if usernames.get(usr.username.as_bytes())?.is_some() || handles.get(usr.handle.as_bytes())?.is_some() {
        return Err(sled::transaction::ConflictableTransactionError::Abort(()));
      }

      users.insert(user_id.as_bytes(), usr.to_bin())?;
      user_secrets.insert(user_id.as_bytes(), pwd.as_bytes())?;
      usernames.insert(usr.username.as_bytes(), user_id.as_bytes())?;
      handles.insert(usr.handle.as_bytes(), user_id.as_bytes())?;
      Ok(())
    }).is_ok() {
      return Some(usr);
    }

    None
  }

   pub fn authenticate(&self, username: &str, pwd: &str) -> Option<User> {
    if let Ok(Some(usr_id)) = self.usernames.get(username.as_bytes()) {
     if let Ok(Some(usr_pwd)) = self.user_secrets.get(&usr_id) {
       if verify_password(pwd, usr_pwd.to_str()) {
         if let Ok(Some(raw_usr)) = self.users.get(usr_id) {
            return Some(binbe_deserialize(&raw_usr));
         }
       }
      }
    }
    None
  }

  pub fn setup_session(&self, usr_id: String) -> Option<String> {
    let sess_id = format!("{}:{}", usr_id.clone(), random_string(28));
    let timestamp = Utc::now();
    if self.sessions.scan_prefix(usr_id.clone().as_bytes()).any(|r| r.map_or(false, |(k, v)| {
      let ses: UserSession = binbe_deserialize(&v);
      if ses.has_expired() {
        let res: TransactionResult<(), ()> = (&self.sessions, &self.users)
          .transaction(|(sess, _users)| {
            sess.remove(&k)?;
            Ok(())
          });
        res.unwrap();
        return false;
      }
      timestamp - ses.timestamp < Duration::minutes(5)
    })) {
      return None;
    }

    let session = UserSession{
      usr_id,
      timestamp,
      exp: timestamp + Duration::weeks(2),
    };

    if self.sessions.insert(sess_id.clone().as_bytes(), session.to_bin()).is_ok() {
      return Some(sess_id);
    }
    None
  }

  pub fn is_authenticated(&self, req: &HttpRequest) -> bool {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value();
      if let Some(session) = get_struct::<UserSession>(&self.sessions, sess_id.as_bytes()) {
        return session.has_expired()
      }
    }
    false  
  }

  pub fn is_valid_session(&self, req: &HttpRequest) -> bool {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value();
      if let Some(session) = get_struct::<UserSession>(&self.sessions, sess_id.as_bytes()) {
        if session.has_expired() {
          if let Err(e) = self.sessions.remove(sess_id.as_bytes()) {
            println!("removing expired session from session tree failed: {}", e);
          }
        } else {
          return true;
        }
      }
    }
    false
  }

  pub fn is_admin(&self, usr_id: &str) -> bool {
    if let Ok(is_admin) = self.admins.contains_key(usr_id.as_bytes()) {
      return is_admin;
    }
    false
  }

  pub fn user_by_session(&self, req: &HttpRequest) -> Option<User> {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value();
      if let Some(session) = get_struct::<UserSession>(&self.sessions, sess_id.as_bytes()) {
        if session.has_expired() {
          if let Err(e) = self.sessions.remove(sess_id.as_bytes()) {
            println!("removing expired session from session tree failed: {}", e);
          }
        } else if let Some(usr) = self.user_by_id(&session.usr_id) {
          return Some(usr);
        }
      }
    }
    None
  }
  
  pub fn user_by_session_renew<'c>(
    &self,
    req: &HttpRequest,
    how_far_to_expiry: Duration,
  ) -> (Option<User>, Option<Cookie<'c>>) {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value();
      if let Some(session) = get_struct::<UserSession>(&self.sessions, sess_id.as_bytes()) {
        if session.has_expired() {
          if let Err(e) = self.sessions.remove(sess_id.as_bytes()) {
            println!("removing expired session from session tree failed: {}", e);
          }
        } else {
          let mut cookie: Option<Cookie> = None;
            if session.close_to_expiry(how_far_to_expiry) {
              if self.sessions.remove(sess_id.as_bytes()).is_ok() {
                if let Some(sess_id) = self.setup_session(session.usr_id.clone()) {
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
                  });
              }
            }
          }
          let o_usr = self.user_by_id(&session.usr_id);
          if o_usr.is_some() {
            return (o_usr, cookie);
          }
        }
      }
    }
    (None, None)
  }

  pub fn username_by_session_renew<'c>(
    &self,
    req: &HttpRequest,
    how_far_to_expiry: Duration,
  ) -> (Option<User>, Option<Cookie<'c>>) {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value();
      if let Some(session) = get_struct::<UserSession>(&self.sessions, sess_id.as_bytes()) {
        if session.has_expired() {
          if let Err(e) = self.sessions.remove(sess_id.as_bytes()) {
            println!("removing expired session from session tree failed: {}", e);
          }
        } else {
          let mut cookie: Option<Cookie> = None;
            if session.close_to_expiry(how_far_to_expiry) {
            if self.sessions.remove(sess_id.as_bytes()).is_ok() {
              if let Some(sess_id) = self.setup_session(session.usr_id.clone()) {
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
                });
              }
            }
          }
          if let Some(usr) = self.user_by_id(&session.usr_id) {
            return (Some(usr), cookie);
          }
        }
      }
    }
    (None, None)
  }

  pub fn admin_by_session(&self, req: &HttpRequest) -> Option<User> {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value();
      if let Some(session) = get_struct::<UserSession>(&self.sessions, sess_id.as_bytes()) {
        if session.has_expired() {
          if let Err(e) = self.sessions.remove(sess_id.as_bytes()) {
            println!("removing expired session from session tree failed: {}", e);
          }
        } else if self.is_admin(&session.usr_id) {
          if let Some(usr) = self.user_by_id(&session.usr_id) {
            return Some(usr);
          }
        }
      }
    }
    None
  }

  pub fn is_valid_admin_session(&self, req: &HttpRequest) -> bool {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value();
      if let Some(session) = get_struct::<UserSession>(&self.sessions, sess_id.as_bytes()) {
        if session.has_expired() {
          if let Err(e) = self.sessions.remove(sess_id.as_bytes()) {
            println!("removing expired session from session tree failed: {}", e);
          }
        } else if let Ok(is_admin) = self.admins.contains_key(session.usr_id) {
          return is_admin;
        }
      }
    }
    false
  }

  pub fn user_by_id(&self, id: &str) -> Option<User> {
    get_struct(&self.users, id.as_bytes())
  }

  pub fn admin_by_id(&self, id: &str) -> Option<User> {
    if self.is_admin(id) {
      if let Some(usr) = get_struct::<User>(&self.users, id.as_bytes()) {
        return Some(usr);
      }
    }
    None
  }

  pub fn user_by_username(&self, username: &str) -> Option<User> {
    if let Some(user_id) = self.usernames.get(username.as_bytes()).unwrap() {
      return get_struct(&self.users, &user_id);
    }
    None
  }

  pub fn user_by_handle(&self, handle: &str) -> Option<User> {
    if let Ok(Some(user_id)) = self.handles.get(handle.as_bytes()) {
      return get_struct(&self.users, &user_id);
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
    if self.username_taken(new_username) { return false; }

    let old_username = usr.username.clone();
    usr.username = new_username.to_string();

    let res: TransactionResult<(), ()> = (&self.users, &self.usernames).transaction(|(users, usernames)| {
      users.insert(usr.id.as_bytes(), usr.to_bin())?;
      usernames.remove(old_username.as_bytes())?;
      usernames.insert(new_username.as_bytes(), usr.id.as_bytes())?;
      Ok(())
    });
    res.is_ok()
  }

  pub fn change_handle(&self, usr: &mut User, new_handle: &str) -> bool {
    if self.handle_taken(new_handle) { return false; }

    let old_handle = usr.handle.clone();
    usr.handle = new_handle.to_string();
    let res: TransactionResult<(), ()> = (&self.users, &self.handles)
      .transaction(|(users, handles)| {
        users.insert(usr.id.as_bytes(), usr.to_bin())?;
        handles.remove(old_handle.as_bytes())?;
        handles.insert(new_handle.as_bytes(), usr.id.as_bytes())?;
        Ok(())
      });
    res.is_ok()
  }

  pub fn change_description(&self, usr: &mut User, new_desc: &str) -> bool {
    new_desc.len() > 1 &&
    self.user_descriptions.insert(usr.id.as_bytes(), new_desc.as_bytes()).is_ok()
  }

  pub fn change_password(&self, usr: &mut User, new_pwd: &str) -> bool {
    if !is_password_ok(new_pwd) { return false; }
    self.hash_pwd(new_pwd).map_or(false, |p| {
      let res: TransactionResult<(), ()> = (&self.user_secrets, &self.users)
        .transaction(|(user_secrets, _users)| {
          user_secrets.insert(usr.id.as_bytes(), p.as_bytes())?;
          Ok(())
        });
      res.is_ok()
    })
  }

  pub fn make_admin(&self, usr_id: &str, level: u8) -> bool {
    let res: TransactionResult<(), ()> = (&self.user_attributes, &self.admins)
      .transaction(|(usr_attrs, admins)| {
        let attributes = if let Some(raw_attrs) = usr_attrs.get(usr_id.as_bytes())? {
          let mut attributes: Vec<String> = binbe_deserialize(&raw_attrs);
          attributes.push("admin".to_string());
          attributes.dedup();
          attributes
        } else {
          vec!("admin".to_string())
        };
        usr_attrs.insert(usr_id.as_bytes(), binbe_serialize(&attributes))?;
        admins.insert(usr_id.as_bytes(), &[level])?;
        Ok(())
      });
    res.is_ok()
  }

  pub fn change_admin_level(&self, usr_id: &str, level: u8) -> bool {
    let res: TransactionResult<(), ()> = (&self.user_attributes, &self.admins)
      .transaction(|(usr_attrs, admins)| {
        if let Some(raw_attrs) = usr_attrs.get(usr_id.as_bytes())? {
          let attributes: Vec<String> = binbe_deserialize(&raw_attrs);
          if attributes.contains(&"admin".to_string()) {
            admins.insert(usr_id.as_bytes(), &[level])?;
          }
        } else {
          return Err(sled::transaction::ConflictableTransactionError::Abort(()));
        }
        Ok(())
      });
    res.is_ok()
  }

  pub fn has_admin_level(&self, usr_id: &str, level: u8) -> bool {
    if let Some(levels) = self.admins.get(usr_id.as_bytes()).unwrap() {
      return levels[0] == level;
    }
    false
  }

  pub fn revoke_admin(&self, usr_id: &str) -> bool {
    let res: TransactionResult<(), ()> =
      (&self.user_attributes, &self.admins).transaction(|(usr_attrs, admins)| {
        if let Some(raw_attrs) = usr_attrs.get(usr_id.as_bytes())? {
          let attributes: Vec<String> = binbe_deserialize::<Vec<String>>(&raw_attrs)
          .iter().filter(|t| *t != "admin")
          .cloned().collect();
          
          usr_attrs.insert(usr_id.as_bytes(), binbe_serialize(&attributes))?;
        } else {
          return Err(sled::transaction::ConflictableTransactionError::Abort(()));
        }
        admins.remove(usr_id.as_bytes())?;
        Ok(())
      });
    res.is_ok()
  }

  pub fn user_attributes(&self, usr_id: &str) -> Vec<String> {
    get_struct(&self.user_attributes, usr_id.as_bytes()).unwrap_or(vec!())
  }

  pub fn bestow_attributes(&self, usr_id: &str, attrs: Vec<String>) -> bool {
    let res: TransactionResult<(), ()> = self.user_attributes.transaction(|usr_attrs| {
      if let Some(raw_attrs) = usr_attrs.get(usr_id.as_bytes())? {
        let mut attributes = binbe_deserialize::<Vec<String>>(&raw_attrs);
        for attr in &attrs {
          attributes.push(attr.clone());
        }
        attributes.dedup();

        usr_attrs.insert(
          usr_id.as_bytes(),
          binbe_serialize(&attributes)
        )?;
      } else {
        return Err(sled::transaction::ConflictableTransactionError::Abort(()));
      }
      Ok(())
    });
    res.is_ok()
  }

  pub fn strip_attributes(&self, usr_id: &str, attrs: Vec<String>) -> bool {
    let res: TransactionResult<(), ()> = self.user_attributes.transaction(|usr_attrs| {
      if let Some(raw_attrs) = usr_attrs.get(usr_id.as_bytes())? {
        let attributes: Vec<String> = binbe_deserialize::<Vec<String>>(&raw_attrs)
          .iter().filter(|t| !attrs.contains(t))
          .cloned().collect();
        usr_attrs.insert(
          usr_id.as_bytes(),
          binbe_serialize(&attributes)
        )?;
      } else {
        return Err(sled::transaction::ConflictableTransactionError::Abort(()));
      }
      Ok(())
    });
    res.is_ok()
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
      time_cost: 10,
      lanes: 4,
      thread_mode: argon2::ThreadMode::Parallel,
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

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct User {
  pub id: String,
  pub username: String,
  pub handle: String,
  pub registered: DateTime<Utc>,
  pub email: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct UserSession {
  usr_id: String,
  timestamp: DateTime<Utc>,
  exp: DateTime<Utc>,
}

impl UserSession {
  pub fn has_expired(&self) -> bool {
    Utc::now() > self.exp
  }

  pub fn close_to_expiry(&self, with_duration: Duration) -> bool {
    Utc::now() + with_duration > self.exp
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
    if !orc.sessions.remove(auth_cookie.value().as_bytes()).is_ok() {
      status = "login was already bad or expired, no worries";
    }
  }
  let mut res = HttpResponse::Accepted().json(json!({
    "ok": true,
    "status": status
  }));
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
    if let Some(session) = get_struct::<UserSession>(&orc.sessions, sess_id.as_bytes()) {
      if session.close_to_expiry(how_far_to_expiry) {
        if orc.sessions.remove(sess_id.as_bytes()).is_ok() {
          if let Some(sess_id) = orc.setup_session(session.usr_id) {
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

pub async fn login(usr: User, first_time: bool, orc: web::Data<Arc<Orchestrator>>) -> HttpResponse {
  let token = if let Some(t) = orc.setup_session(usr.id) { t } else {
    return HttpResponse::Forbidden().json(json!({
      "ok": false,
      "status": "trouble setting up session"
    }));
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
    "staus": "successfully logged in",
    "first_time": first_time,
  }))
}

#[get("/auth")]
pub async fn check_authentication(req: HttpRequest, orc: web::Data<Arc<Orchestrator>>) -> HttpResponse {
  if orc.is_valid_session(&req) {
    return HttpResponse::Accepted().json(json!({
      "ok": true,
      "data": "authenticated"
    }));
  }
  HttpResponse::Forbidden().json(json!({
    "ok": false,
    "data": "not authenticated"
  }))
}

#[post("/auth")]
pub async fn auth_attempt(
  req: HttpRequest,
  ar: web::Json<AuthRequest>,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  let (username, pwd) = (ar.username.as_str(), ar.password.as_str());

  if !is_username_ok(username) {
    return HttpResponse::BadRequest().json(json!({
      "ok": false,
      "status": "username is no good, it's either too long, too short, or has weird characters in it, fix it up and try again."
    }));
  }
  if !is_password_ok(pwd) {
    return HttpResponse::BadRequest().json(json!({
      "ok": false,
      "status": "malformed password"
    }));
  }

  if let Some(usr) = orc.user_by_session(&req) {
    return HttpResponse::Accepted().json(json!({
      "ok": true,
      "status": &format!("Hey {}, you're already authenticated.", usr.username)
    }));
  }

  let hitter = req.peer_addr().map_or(username.to_string(), |a| format!("{}{}", username, a));
  let rl = orc.ratelimiter.hit(hitter.as_bytes(), 3, Duration::minutes(2));
  if rl.is_timing_out() {
    return HttpResponse::TooManyRequests().json(json!({
      "ok": false,
      "status": &format!("Too many requests, timeout has {} minutes left.", rl.minutes_left())
    }));
  }

  if orc.username_taken(username) {
    if let Some(usr) = orc.authenticate(username, pwd) {
      orc.ratelimiter.forget(hitter.as_bytes());
      return login(usr, false, orc.clone()).await;
    }
    return HttpResponse::BadRequest().json(json!({
      "ok": false,
      "status": "either your password is wrong or the username is already taken."
    }));
  }

  if let Some(usr) = orc.create_user(ar.into_inner()) {
    orc.ratelimiter.forget(hitter.as_bytes());
    return login(usr, true, orc).await;
  }

  HttpResponse::Forbidden().json(json!({
    "ok": false,
    "status": "not working, we might be under attack"
  }))
}

#[derive(Serialize, Deserialize)]
pub struct AdministralityRequest {key: String}

#[post("/administrality")]
pub async fn administer_administrality(
  req: HttpRequest,
  ar: web::Json<AdministralityRequest>,
  orc: web::Data<Arc<Orchestrator>>,
) -> impl Responder {
  if CONF.read().admin_key == ar.key {
    if let Some(ref mut usr) = orc.user_by_session(&req) {
      if orc.make_admin(&usr.id, 0) {
        return HttpResponse::Accepted().json(json!({
          "ok": true,
          "status": "Congratulations, you are now an admin."
        }));
      }
      return HttpResponse::InternalServerError().json(json!({
        "ok": false,
        "status": format!(
          "Sorry {}, you got it right, but there was a database error. Try again later. ;D",
          usr.username
        ),
      }));
    } else if let Some(remote_addr) = req.connection_info().remote_addr() {
      let hitter = format!("aA{}", remote_addr);
      let rl = orc.ratelimiter.hit(hitter.as_bytes(), 2, Duration::minutes(60));
      if rl.is_timing_out() {
        return HttpResponse::TooManyRequests().json(json!({
          "ok": false,
          "status": &format!("too many requests, timeout has {} minutes left.", rl.minutes_left())
        }));
      }
    }
  } else if let Some(auth_cookie) = req.cookie("auth") {
    let hitter = format!("aA{}", auth_cookie.value());
    let rl = orc.ratelimiter.hit(hitter.as_bytes(), 2, Duration::minutes(60));
    if rl.is_timing_out() {
      return HttpResponse::TooManyRequests().json(json!({
        "ok": false,
        "status": &format!("too many requests, timeout has {} minutes left.", rl.minutes_left())
      }));
    }
  } else if let Some(remote_addr) = req.connection_info().remote_addr() {
    let hitter = format!("aA{}", remote_addr);
    let rl = orc.ratelimiter.hit(hitter.as_bytes(), 2, Duration::minutes(60));
    if rl.is_timing_out() {
      return HttpResponse::TooManyRequests().json(json!({
        "ok": false,
        "status": &format!("too many requests, timeout has {} minutes left.", rl.minutes_left())
      }));
    }
  }

  HttpResponse::Forbidden().json(json!({
    "ok": false,
    "status": "Sorry, no administrality for you."
  }))
}

#[delete("/administrality")]
pub async fn remove_administrality(
  req: HttpRequest,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  if let Some(ref mut usr) = orc.admin_by_session(&req) {
    if orc.revoke_admin(&usr.id) {
      return HttpResponse::Accepted().json(json!({
        "ok": true,
        "data": format!("Sorry {}, you've lost your adminstrality.", usr.username).as_str()
      }));
    }
  }
  HttpResponse::Forbidden().json(json!({
    "ok": false,
    "data": "To lose your administrality you need to have some in the first place!"
  }))
}

#[get("/administrality")]
pub async fn check_administrality(
  req: HttpRequest,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  if orc.is_valid_admin_session(&req) {
    return HttpResponse::Accepted().json(json!({
      "ok": true,
      "data": "genuine admin"
    }));
  }
  HttpResponse::Forbidden().json(json!({
    "ok": false,
    "data": "silly impostor, you are not admin"
  }))
}
