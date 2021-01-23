use actix_web::{
  cookie::Cookie, delete, get, post, web, HttpMessage, HttpRequest, HttpResponse,
};
use borsh::{BorshDeserialize, BorshSerialize};
use lettre::{Message, message::{header, MultiPart, SinglePart}};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sled::{transaction::*, IVec, Transactional};
use slug::slugify;
use thiserror::Error;
use time::Duration;

use std::{
  collections::{BTreeMap}
};

use super::{CONF, TEMPLATES};

use crate::{email::EmailStatus, expirable_data::ExpirableData, orchestrator::{Orchestrator, ORC}, responses, utils::{
    is_email_ok,
    is_username_ok,
    is_handle_ok,
    random_string,
    unix_timestamp,
    // FancyBool,
    FancyIVec,
  }};

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

  pub fn create_user(
    &self,
    username: String,
    email: String,
    handle: Option<String>,
  ) -> Option<User> {
    if !is_username_ok(&username) {
      return None;
    }
    if !is_email_ok(&email) {
      return None;
    }

    let user_id = match self.generate_id("usr".as_bytes()) {
      Ok(id) => id,
      Err(_) => return None,
    };

    let mut handle = handle.unwrap_or(slugify(&username));
    if let Some(taken) = self.handle_taken(&handle) {
      if taken {
        let mut num = 1u32;
        let mut new_handle = format!("{}-{}", handle, num);
        while let Some(taken) = self.handle_taken(&handle) {
          if taken {
            if num > 10 {
              return None;
            }
            num = num + 1;
            new_handle = format!("{}-{}", handle, num);
          } else {
            handle = new_handle.clone();
            break;
          }
        }
        if handle != new_handle {
          return None;
        }
      }
    } else {
      return None;
    }

    let usr = User {
      id: user_id,
      username: username.clone(),
      handle,
      reg: unix_timestamp(),
    };

    if (
      &self.users,
      &self.usernames,
      &self.emails,
      &self.user_email_index,
      &self.handles,
    )
      .transaction(|(users, usernames, emails, user_email_index, handles)| {
        if usernames.get(usr.username.as_bytes())?.is_some()
          || handles.get(usr.handle.as_bytes())?.is_some()
          || emails.get(email.as_bytes())?.is_some()
        {
          return Err(sled::transaction::ConflictableTransactionError::Abort(()));
        }

        let uid = IVec::from_u64(user_id);

        users.insert(&uid, usr.try_to_vec().unwrap())?;
        usernames.insert(usr.username.as_bytes(), &uid)?;
        emails.insert(email.as_bytes(), &uid)?;
        user_email_index.insert(&uid, email.as_bytes())?;
        handles.insert(usr.handle.as_bytes(), &uid)?;
        Ok(())
      })
      .is_ok()
    {
      let mut exp_data: BTreeMap<String, Vec<Vec<u8>>> = BTreeMap::new();
      
      exp_data.insert("users".to_string(), vec!(usr.id.to_be_bytes().to_vec()));
      exp_data.insert("usernames".to_string(), vec!(usr.username.as_bytes().to_vec()));
      exp_data.insert("emails".to_string(), vec!(email.as_bytes().to_vec()));
      exp_data.insert("user_email_index".to_string(), vec!(usr.id.to_be_bytes().to_vec()));
      exp_data.insert("handles".to_string(), vec!(usr.handle.as_bytes().to_vec()));

      if !self.expire_data(
        if self.dev_mode {
          // 5 min
          5 * 60
        } else {
          // 8 min
          8 * 60
        },
    ExpirableData::MultiTree(exp_data),
    Some(&usr.id.to_be_bytes())
      ) && self.dev_mode {
        println!("failed to set expiry for unverified user");
      }

      return Some(usr);
    }

    None
  }

  pub fn create_magic_link_email(
    &self,
    first_time: bool,
    usr_id: u64,
    username: String,
    email: String,
  ) -> Option<lettre::Message> {
    let ml = MagicLink::new(usr_id);

    let ml_res: TransactionResult<(), ()> = self.magic_links.transaction(|magic_links| {
      magic_links.insert(ml.code.as_bytes(), ml.try_to_vec().unwrap())?;
      Ok(())
    });

    if ml_res.is_err() {
      if ORC.dev_mode {
        println!("create_magic_link_email: db transaction failed");
      }
      return None;
    }

    let mut ctx = tera::Context::new();
    ctx.insert("magic_code", &ml.code);
    ctx.insert("username", &username);
    ctx.insert("dev_mode", &ORC.dev_mode);
    ctx.insert("domain", &CONF.read().domain);
    if first_time {
      ctx.insert("email_type", "Verification");
    } else {
      ctx.insert("email_type", "Authentication");
    }

    let html_body = match TEMPLATES.read().render("magic-link-email.html", &ctx) {
      Ok(s) => s,
      Err(e) => {
        if ORC.dev_mode {
          println!("magic-link email template had errors: {}", e);
        }
        return None;
      },
    };
    let txt_body = match TEMPLATES.read().render("magic-link-email-text-version.txt", &ctx) {
      Ok(s) => s,
      Err(e) => {
        if ORC.dev_mode {
          println!("magic-link email template had errors: {}", e);
        }
        return None;
      },
    };

    if let Ok(msg) = Message::builder()
      .from("Kurshok Space Auth <admin@kurshok.space>".parse().unwrap())
      .to(format!("{} <{}>", username, email).parse().unwrap())
      .subject("Kurshok Space Auth email")
      .multipart(
        MultiPart::alternative()
          .singlepart(
            SinglePart::builder()
              .header(header::ContentType(
                "text/plain; charset=utf8".parse().unwrap(),
              ))
              .body(txt_body)
          )
          .singlepart(
            SinglePart::builder()
              .header(header::ContentType(
                "text/html; charset=utf8".parse().unwrap(),
              ))
              .body(html_body),
          ),
      ) {
      return Some(msg);
    }

    if ORC.dev_mode {
      println!("sending magic-link email failed");
    }

    return None;
  }

  fn handle_magic_link(&self, code: String) -> Option<User> {
    if code.len() != 20 {
      return None;
    }

    let res: TransactionResult<(User, String, bool), ()> = (
      &self.magic_links,
      &self.users,
      &self.user_verifications,
      &self.user_email_index
    ).transaction(|(
        magic_links,
        users,
        user_verifications,
        user_email_index
    )| {
      if let Some(raw) = magic_links.get(code.as_bytes())? {
        let ml = MagicLink::try_from_slice(&raw).unwrap();
        magic_links.remove(code.as_bytes())?;
        if !ml.has_expired() {
          if let Some(raw) = users.get(&ml.usr_id.to_be_bytes())? {
            let usr = User::try_from_slice(&raw).unwrap();
            let first_time = user_verifications.get(usr.id.to_be_bytes())?.is_none();
            if first_time {
              let v = UserVerification::new();
              user_verifications.insert(IVec::from_u64(usr.id), v.try_to_vec().unwrap())?;
            }
            if let Some(raw_email) = user_email_index.get(usr.id.to_be_bytes())? {
              return Ok((usr, raw_email.to_string(), first_time));
            }
          } else if self.dev_mode {
            println!("could not find user associated with magic-link");
          }
        } else if self.dev_mode {
          println!("someone tried to use an expired magic-link");
        }
      } else if self.dev_mode {
        println!("could not find this {} - magic-link code", &code);
      }
      Err(sled::transaction::ConflictableTransactionError::Abort(()))
    });

    if let Ok((usr, email, first_time)) = res {
      if first_time {
        if self.unexpire_data(&usr.id.to_be_bytes()) && self.dev_mode {
          println!("no need to clean up user: {} anymore, they are verified", &usr.username);
        } else if self.dev_mode {
          println!("we fucked up, a verified user: {} was/will-be deleted", &usr.username);
        }

        if CONF.read().admin_emails.contains(&email) {
          self.make_admin(usr.id, 0, Some("blessed email".to_string()));
        }
      }

      return Some(usr);
    }
    if self.dev_mode {
      println!("handle_magic_link: db transaction failed");
    }
    None
  }

  pub fn create_preauth_token(&self, usr_id: u64) -> Option<String> {
    let res: TransactionResult<String, ()> = self.preauth_tokens.transaction(|preauth_tokens| {
      let token = random_string(22);
      preauth_tokens.insert(token.as_bytes(), &usr_id.to_be_bytes())?;
      Ok(token)
    });
  
    if let Ok(token) = res {
      // 7 minutes
      ORC.expire_data(60 * 7, ExpirableData::Single{
          tree: "preauth_tokens".to_string(),
          key: token.as_bytes().to_vec(),
        },
        Some(token.as_bytes())
      );
      return Some(token);
    }
    None
  }

  pub fn destroy_preauth_token(&self, preauth_token: &str) -> bool {
    let res: TransactionResult<(), ()> = self.preauth_tokens.transaction(|preauth_tokens| {
      preauth_tokens.remove(preauth_token.as_bytes())?;
      Ok(())
    });

    ORC.unexpire_data(preauth_token.as_bytes());

    res.is_ok()
  }

  pub fn setup_session(&self, usr_id: u64) -> Result<String, AuthError> {
    let sess_id = format!("{}:{}", usr_id, random_string(20));
    let timestamp = unix_timestamp();
    let sess_prefix = format!("{}", usr_id);
    if self.sessions.scan_prefix(sess_prefix.as_bytes()).any(|r| {
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
        return Ok(sess_id);
      },
      Err(e) => {
        if self.dev_mode {
          println!("sessions.insert error: {:?}", e);
        }
        return Err(AuthError::DBIssue);
      }
    }
  }

  pub fn get_session(&self, id: &String) -> Option<UserSession> {
    if let Ok(Some(raw)) = self.sessions.get(id.as_bytes()) {
      let session = UserSession::try_from_slice(&raw).unwrap();
      if session.has_expired() {
        if let Err(e) = self.sessions.remove(id.as_bytes()) {
          println!("removing expired session from session tree failed: {}", e);
        }
        return None;
      }
      return Some(session);
    }
    None
  }

  pub fn is_admin(&self, usr_id: u64) -> bool {
    self.admins.contains_key(&usr_id.to_be_bytes()).unwrap_or(false)
  }

  pub fn user_by_session(&self, req: &HttpRequest) -> Option<User> {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value().to_string();
      if let Some(session) = self.get_session(&sess_id) {
        return self.user_by_id(session.usr_id);
      }
    }
    None
  }

  pub fn user_id_by_session(&self, req: &HttpRequest) -> Option<u64> {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value().to_string();
      if let Some(session) = self.get_session(&sess_id) {
        return Some(session.usr_id);
      }
    }
    None
  }
  pub fn user_by_session_renew<'c>(
    &self,
    req: &HttpRequest,
    how_far_to_expiry: Duration,
  ) -> (
    Option<User>,
    Option<Cookie<'c>>,
  ) {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value().to_string();
      if let Some(session) = self.get_session(&sess_id) {
        let mut cookie: Option<Cookie> = None;
        if session.close_to_expiry(how_far_to_expiry) {
          let usr_id = session.usr_id.clone();
          if self.sessions.remove(sess_id.as_bytes()).is_ok() {
            if let Ok(sess_id) = self.setup_session(usr_id) {
              cookie = Some(build_the_usual_cookie("auth", sess_id));
            }
          }
        }
        let o_usr = self.user_by_id(session.usr_id);
        if o_usr.is_some() {
          return (o_usr, cookie);
        }
      }
    }
    (None, None)
  }

  pub fn admin_by_session(&self, req: &HttpRequest) -> Option<User> {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value().to_string();
      if let Some(session) = self.get_session(&sess_id) {
        if self.is_admin(session.usr_id) {
          return self.user_by_id(session.usr_id);
        }
      }
    }
    None
  }

  pub fn is_valid_admin_session(&self, req: &HttpRequest) -> bool {
    if let Some(auth_cookie) = req.cookie("auth") {
      let sess_id = auth_cookie.value().to_string();
      if let Some(session) = self.get_session(&sess_id) {
        return self.is_admin(session.usr_id);
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

  pub fn user_by_id(&self, id: u64) -> Option<User> {
    match self.users.get(&id.to_be_bytes()) {
      Ok(raw) => raw.map(|raw| User::try_from_slice(&raw).unwrap()),
      Err(_) => None,
    }
  }
/*
  pub fn user_by_ivec(&self, id: IVec) -> Option<User> {
    if let Ok(Some(raw)) = self.users.get(id) {
      return Some(User::try_from_slice(&raw).unwrap());
    }
    None
  }

  pub fn admin_by_id(&self, id: u64) -> Option<User> {
    if self.is_admin(id) {
      return self.user_by_id(id);
    }
    None
  }

  pub fn user_by_username(&self, username: &str) -> Option<User> {
    if let Ok(Some(id)) = self.usernames.get(username.as_bytes()) {
      return self.user_by_ivec(id);
    }
    None
  }

  pub fn user_by_handle(&self, handle: &str) -> Option<User> {
    if let Ok(Some(id)) = self.handles.get(handle.as_bytes()) {
      return self.user_by_ivec(id);
    }
    None
  }
  */
  pub fn username_taken(&self, username: &str) -> Option<bool> {
    if let Ok(taken) = self.usernames.contains_key(username.as_bytes()) {
      return Some(taken);
    }
    None
  }

  pub fn handle_taken(&self, handle: &str) -> Option<bool> {
    if let Ok(taken) = self.handles.contains_key(handle.as_bytes()) {
      return Some(taken);
    }
    None
  }

  pub fn change_username(&self, usr: &mut User, new_username: &str) -> Option<UserError> {
    if self.username_taken(new_username).unwrap_or(true) {
      return Some(UserError::UsernameTaken);
    }

    let old_username = usr.username.clone();
    usr.username = new_username.to_string();

    let res: TransactionResult<(), UserError> = (
      &self.users,
      &self.username_changes,
      &self.usernames
    ).transaction(|(users, username_changes, usernames)| {
      let usr_id = IVec::from_u64(usr.id);

      users.insert(&usr_id, usr.try_to_vec().unwrap())?;
      usernames.remove(old_username.as_bytes())?;

      if let Some(raw) = username_changes.get(&usr_id)? {
        let mut old_usernames: BTreeMap<String, i64> = BorshDeserialize::try_from_slice(&raw).unwrap();

        let now = unix_timestamp();
        let mut changes = 0;
        for (_, timestamp) in &old_usernames {
          if let Some(res) = now.checked_sub(*timestamp) {
            let week = 60 * 60 * 24 * 7;
            if res < week {
              changes += 1;
              if changes == 2 {
                break;
              }  
            }
          }
        }
        if changes > 1 {
          return Err(sled::transaction::ConflictableTransactionError::Abort(UserError::ChangedUsernameTooSoon));
        }
         
        old_usernames.insert(old_username.clone(), unix_timestamp());
        let new_raw = old_usernames.try_to_vec().unwrap();
        username_changes.insert(&usr_id, new_raw)?;
      } else {
        let mut old_usernames: BTreeMap<String, i64> = BTreeMap::new();
        old_usernames.insert(old_username.clone(), unix_timestamp());
        let raw = old_usernames.try_to_vec().unwrap();
        username_changes.insert(&usr_id, raw.as_slice())?;
      }

      if let Some(_) = usernames.insert(new_username.as_bytes(), usr_id)? {
        return Err(sled::transaction::ConflictableTransactionError::Abort(UserError::UsernameTaken));
      }
      Ok(())
    });
    
    if let Err(err) = res {
      match err {
        sled::transaction::TransactionError::Abort(ur) => {
          return Some(ur);
        },
        sled::transaction::TransactionError::Storage(e) => {
          println!("storage error: {:?}", e);
          return Some(UserError::DBIssue);
        }
      }
    }

    None
  }

  pub fn change_handle(&self, usr: &mut User, new_handle: &str) -> Option<UserError> {
    if self.handle_taken(new_handle).unwrap_or(true) {
      return Some(UserError::HandleTaken);
    }

    let old_handle = usr.handle.clone();
    usr.handle = new_handle.to_string();
    let res: TransactionResult<(), ()> = (
      &self.users, 
      &self.handle_changes, 
      &self.handles
    ).transaction(|(users, handle_changes, handles)| {
      let usr_id = IVec::from_u64(usr.id);
      
      users.insert(&usr_id, usr.try_to_vec().unwrap())?;
      handles.remove(old_handle.as_bytes())?;

      if let Some(raw) = handle_changes.get(&usr_id)? {
        let mut old_handles: BTreeMap<String, i64> = BorshDeserialize::try_from_slice(&raw).unwrap();
        old_handles.insert(old_handle.clone(), unix_timestamp());
        let new_raw = old_handles.try_to_vec().unwrap();
        handle_changes.insert(&usr_id, new_raw)?;
      } else {
        let mut old_handles = BTreeMap::new();
        old_handles.insert(old_handle.clone(), unix_timestamp());
        let raw = old_handles.try_to_vec().unwrap();
        handle_changes.insert(&usr_id, raw)?;
      }

      if let Some(_) = handles.insert(new_handle.as_bytes(), &usr_id)? {
        return Err(sled::transaction::ConflictableTransactionError::Abort(()));
      }
      Ok(())
    });
    
    if res.is_err() {
      return Some(UserError::DBIssue);
    }
    None
  }

  pub fn change_description(&self, usr: &User, new_desc: &str) -> bool {
    new_desc.len() > 2 && new_desc.len() < 301 && self.user_descriptions.insert(
  usr.id.to_be_bytes(),
new_desc.as_bytes()
    ).is_ok()
  }

  pub fn make_admin(&self, usr_id: u64, level: u8, reason: Option<String>) -> bool {
    let attr = UserAttribute {
      aquired: unix_timestamp(),
      reason,
    };
    let res: TransactionResult<(), ()> = (&self.user_attributes, &self.admins)
      .transaction(|(usr_attrs, admins)| {
        let key = format!("{}:admin", usr_id);
        usr_attrs.insert(key.as_bytes(), attr.try_to_vec().unwrap())?;
        admins.insert(IVec::from_u64(usr_id), &[level])?;
        Ok(())
      });
    res.is_ok()
  }
/*
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
    let res: TransactionResult<(), ()> = (
      &self.user_attributes,
      &self.user_attributes_data
    ).transaction(|(usr_attrs, usr_attrs_data)| {
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
  */
  pub fn user_attributes(&self, usr_id: u64) -> Vec<String> {
    let prefix = format!("{}:", usr_id);
    self
      .user_attributes
      .scan_prefix(prefix.as_bytes())
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
/*
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
*/
  pub fn user_has_some_attrs(&self, usr_id: u64, attrs: &[&str]) -> Option<bool> {
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
}

#[derive(Error, Debug)]
pub enum AuthError {
  #[error("ran into trouble removing bad or expired sessions")]
  SessionRemovalErr,
  #[error("there was a problem interacting with the db")]
  DBIssue, /*
  #[error("id does not match any currently existing user")]
  BadID,
  #[error("id generation failed for some reason, maybe try again later")]
  IDGenErr,
  #[error("too many requests to Auth API, chill for a bit")]
  RateLimit,
  #[error("something fishy going on with session")]
  SessionErr,
  #[error("Unknown auth error")]
  Unknown, */
}
#[derive(Error, Debug)]
pub enum UserError {
  #[error("username is already taken or blacklisted")]
  UsernameTaken,
  #[error("there was a problem interacting with the db")]
  DBIssue, 
  #[error("username was changed too soon after last change")]
  ChangedUsernameTooSoon,
  #[error("handle is already taken or blacklisted")]
  HandleTaken, /*
  #[error("username is invalid or malformed")]
  BadUsername,
  #[error("handle is invalid or malformed")]
  BadHandle,
  #[error("email is already taken or blacklisted")]
  EmailTaken,
  #[error("email is invalid or malformed")]
  BadEmail,
  #[error("Unknown auth error")]
  Unknown, */
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct User {
  pub id: u64,
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
  usr_id: u64,
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

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Debug)]
pub struct UserVerification {
  pub date: i64,
}

impl UserVerification {
  fn new() -> Self {
    UserVerification{
      date: unix_timestamp(),
    }
  }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Debug)]
pub struct MagicLink {
  pub code: String,
  pub usr_id: u64,
  pub expiry: i64,
}

impl MagicLink {
  fn new(usr_id: u64) -> Self {
    MagicLink{
      code: random_string(20),
      usr_id,
      // 5 minutes in the future + 30 seconds for possible delivery time
      expiry: unix_timestamp() + 330
    }
  }

  fn has_expired(&self) -> bool {
    unix_timestamp() > self.expiry
  }
}

#[derive(Serialize, Deserialize)]
pub struct AuthRequest {
  username: String,
  email: String,
  handle: Option<String>,
}

#[delete("/auth")]
pub async fn logout(req: HttpRequest) -> HttpResponse {
  let mut status = "successfully logged out";
  if let Some(auth_cookie) = req.cookie("auth") {
    let sess_id = auth_cookie.value().to_string();
    if ORC.sessions.remove(sess_id.as_bytes()).is_err() {
      status = "login was already bad or expired, no worries";
    }
  }
  let mut res = responses::Accepted(status);
  res.del_cookie("auth");
  res
}
/*
pub fn renew_session_cookie<'c>(
  req: &HttpRequest,
  how_far_to_expiry: Duration,
) -> Option<Cookie<'c>> {
  if let Some(auth_cookie) = req.cookie("auth") {
    let sess_id = auth_cookie.value();
    if let Ok(Some(raw)) = ORC.sessions.get(sess_id.as_bytes()) {
      let session = UserSession::try_from_slice(&raw).unwrap();
      if session.close_to_expiry(how_far_to_expiry) {
        if ORC.sessions.remove(sess_id.as_bytes()).is_ok() {
          if let Ok(sess_id) = ORC.setup_session(session.usr_id) {
            return Some(build_the_usual_cookie("auth", sess_id));
          }
        }
      }
    }
  }
  None
}
*/

#[get("/auth")]
pub async fn check_authentication(req: HttpRequest) -> HttpResponse {
  if ORC.is_valid_session(&req) {
    return responses::Accepted("authenticated");
  }
  responses::Forbidden("not authenticated")
}


#[get("/auth/verification")]
pub async fn indirect_auth_verification(req: HttpRequest) -> HttpResponse {
  if let Some(preauth_cookie) = req.cookie("preauth") {
    let preauth_token = preauth_cookie.value().to_string();
    if preauth_token.len() == 22 {
      if let Ok(res) = ORC.preauth_tokens.get(preauth_token.as_bytes()) {
          if let Some(raw) = res {
            let usr_id = raw.to_u64();
            if let Ok(res) = ORC.users_primed_for_auth.remove(raw) {
              let forbidden = if let Some(raw) = res {
                ORC.destroy_preauth_token(&preauth_token);
                if ORC.dev_mode {
                  let now = unix_timestamp();
                  let expiry = raw.to_i64();
                  println!("/auth/verification - now = {} > expiry = {} == {}", now, expiry, now > expiry);
                }
                // check if auth priming has expired
                unix_timestamp() > raw.to_i64()
              } else {
                true
              };

              if forbidden {
                return responses::Forbidden("Sorry, your auth attempt expired or was invalid, you'll have to try again");
              }

              let token = match ORC.setup_session(usr_id) {
                Ok(t) => t,
                Err(e) => {
                  return responses::Forbidden(format!("trouble setting up session: {}", e));
                }
              };

              if ORC.dev_mode {
                println!("/auth/verification - new token generated");
              }

              return HttpResponse::Accepted()
                .del_cookie(&preauth_cookie)
                .cookie(
                  build_the_usual_cookie("auth", &token)
                )
                .content_type("application/json")
                .json(json!({
                  "ok": true,
                  "status": "Authentication succesful!"
                }));
            } else {
              return responses::InternalServerError("Database error encountered during auth");
            }
          } else {
            return responses::BadRequest("Authentication failed, expired preauth cookie");
          }
      } else {
        return responses::InternalServerError("Failed to read preauth token from database");
      }
    } else {
      return responses::BadRequest("invalid preauth cookie, are you trying to hack your way in?");
    }
  }
  responses::Forbidden("authentication failed, missing preauth cookie")
}

#[get("/auth/email-status")]
pub async fn auth_email_status_check(req: HttpRequest) -> HttpResponse {
  if let Some(preauth_cookie) = req.cookie("preauth") {
    let preauth_token = preauth_cookie.value().to_string();
    if preauth_token.len() == 22 {
      if let Ok(res) = ORC.email_statuses.get(preauth_token.as_bytes()) {
          if let Some(raw) = res {
            let status: EmailStatus = BorshDeserialize::try_from_slice(&raw).unwrap();
            return match status {
              EmailStatus::Failed(reason) => responses::InternalServerError(
                reason.unwrap_or("The email failed to send, reason unknown".to_string())
              ),
              EmailStatus::Sending => responses::NotModified("The email is still sending"),
              EmailStatus::Sent => responses::Accepted("The email was successfully sent"),
            }
          } else {
            return responses::BadRequest("Email status check failed, expired preauth cookie");
          }
      } else {
        return responses::InternalServerError("Failed to read email status from database");
      }
    } else {
      return responses::BadRequest("invalid preauth cookie, are you trying to hack?");
    }
  }
  responses::Forbidden("emails status check failed, missing preauth cookie")
}

#[get("/auth/{code}")]
pub async fn auth_link(req: HttpRequest, code: web::Path<String>) -> HttpResponse {
  if let Some(addr) = req.peer_addr() {
    let hitter = format!("ml{}", addr);
    if let Some(rl) = ORC.ratelimiter.hit(
      hitter.as_bytes(), 3, Duration::minutes(2)
    ) {
      if rl.is_timing_out() {
        return responses::TooManyRequests(format!(
          "Too many requests, timeout has {} minutes left.",
          rl.minutes_left()
        ));
      }
    }
  }

  if let Some(usr) = ORC.handle_magic_link(code.into_inner()) {
    if let Some(addr) = req.peer_addr() {
      let mut hitter = format!("ml{}", addr);
      ORC.ratelimiter.forget(hitter.as_bytes());
      hitter = format!("{}{}", &usr.username, addr);
      ORC.ratelimiter.forget(hitter.as_bytes());
    }

    if let Some(preauth_cookie) = req.cookie("preauth") {
      let preauth_token = preauth_cookie.value().to_string();
      if preauth_token.len() == 22 {
        if let Ok(res) = ORC.preauth_tokens.get(preauth_token.as_bytes()) {
          if let Some(raw) = res {
            let usr_id = raw.to_u64();
            if usr_id == usr.id {
              ORC.destroy_preauth_token(&preauth_token);

              let token = match ORC.setup_session(usr_id) {
                Ok(t) => t,
                Err(e) => {
                  return responses::Forbidden(format!("trouble setting up session: {}", e));
                }
              };

              let mut ctx = tera::Context::new();
              ctx.insert("dev_mode", &ORC.dev_mode);
              return match TEMPLATES.read().render("magic-link-verification-page.html", &ctx) {
                Ok(s) => HttpResponse::Accepted()
                  .del_cookie(&preauth_cookie)
                  .cookie(
                    build_the_usual_cookie("auth", &token)
                  )
                  .content_type("text/html")
                  .body(s),
                Err(err) => {
                    if ORC.dev_mode {
                        HttpResponse::InternalServerError()
                            .content_type("text/plain")
                            .body(&format!("magic-link-verification-page.html template is broken - error : {}", err))
                    } else {
                        HttpResponse::InternalServerError()
                            .content_type("text/plain")
                            .body("The magic-link verification page template is broken! :( We have failed you.")
                    }
                }
              };
            } else {
              return responses::Forbidden("Where did you get this link? It does not match your account.");
            }
          } else {
            return responses::Forbidden("Invalid preauth token");
          }
        } else {
          return responses::InternalServerError("Failed to read preauth token from database");
        }
      }
    } else {
      let priming_expiry_time: i64 = unix_timestamp() + (1000 * 60);

      if ORC.users_primed_for_auth.insert(
        usr.id.to_be_bytes(),
        &priming_expiry_time.to_be_bytes()
      ).is_ok() {
        let mut ctx = tera::Context::new();
        ctx.insert("dev_mode", &ORC.dev_mode);
        ctx.insert("indirect", &true);
        return match TEMPLATES.read().render("magic-link-verification-page.html", &ctx) {
          Ok(s) => HttpResponse::Accepted()
            .content_type("text/html")
            .body(s),
          Err(err) => {
              if ORC.dev_mode {
                  HttpResponse::InternalServerError()
                      .content_type("text/plain")
                      .body(&format!("magic-link-verification-page.html template is broken - error : {}", err))
              } else {
                  HttpResponse::InternalServerError()
                      .content_type("text/plain")
                      .body("The magic-link verification page template is broken! :( We have failed you.")
              }
          }
        };
      }
    }
  }
  responses::Forbidden("authentication failed")
}

#[post("/auth")]
pub async fn auth_attempt(req: HttpRequest, ar: web::Json<AuthRequest>) -> HttpResponse {
  if !is_username_ok(&ar.username) {
    return responses::BadRequest(
      "username is no good, it's either too long, too short, or has weird characters in it, fix it up and try again"
    );
  }

  if !is_email_ok(&ar.email) {
    return responses::BadRequest(
      "email is invalid"
    );
  }

  if let Some(usr) = ORC.user_by_session(&req) {
    return responses::Accepted(format!(
      "Hey {}, you're already authenticated.",
      usr.username
    ));
  }

  if !ORC.dev_mode {
    let hitter = req.peer_addr().map_or(
      ar.username.clone(),
      |a| format!("{}{}", &ar.username, a)
    );

      if let Some(rl) = ORC.ratelimiter.hit(
        hitter.as_bytes(), 3, Duration::minutes(2)
      ) {
      if rl.is_timing_out() {
        return responses::TooManyRequests(format!(
          "Too many requests, timeout has {} minutes left.",
          rl.minutes_left()
        ));
      }
    }
  }

  if let Ok(res) = ORC.usernames.get(ar.username.as_bytes()) {
    if let Some(raw) = res {
      let usr_id = raw.to_u64();
      if let Ok(Some(raw)) = ORC.user_email_index.get(raw) {
        if raw.to_string() != ar.email {
          return responses::Forbidden(
            "Username or email are either mistaken or already taken"
          );
        }
      }

      if let Some(msg) = ORC.create_magic_link_email(
        false,
        usr_id,
        ar.username.clone(),
        ar.email.clone(),
      ) {
        if let Some(preauth_token) = ORC.create_preauth_token(usr_id) {
          crate::email::send_email_with_status_identifier(preauth_token.as_bytes().to_vec(), msg);

          return HttpResponse::Accepted()
            .cookie(
              build_cookie_with_ttl("preauth", &preauth_token, 60 * 10)
            )
            .json(json!({
                "ok": true,
                "status": "Auth email is sending, please check your inbox and also the spam section just in case.",
                "data": {
                  "first_time": false,
                }
            }));
        } else {
          return responses::InternalServerError("Failed to setup pre-auth token");
        }
      }
    }
  } else {
    return responses::InternalServerError(
      "Server had an error when checking the username"
    );
  }

  if let Some(usr) = ORC.create_user(
    ar.username.clone(),
    ar.email.clone(),
    ar.handle.clone()
  ) {
    if ORC.dev_mode {
      println!("registered new user");
    }

    if let Some(msg) = ORC.create_magic_link_email(
      true,
      usr.id,
      ar.username.clone(),
      ar.email.clone(),
    ) {
      if let Some(preauth_token) = ORC.create_preauth_token(usr.id) {
        crate::email::send_email_with_status_identifier(preauth_token.as_bytes().to_vec(), msg);
        return HttpResponse::Accepted()
          .cookie(
            build_cookie_with_ttl("preauth", &preauth_token, 60 * 10)
          )
          .json(json!({
              "ok": true,
              "status": "Auth email is sending, please check your inbox and also the spam section just in case.",
              "data": {
                "first_time": false,
              }
          }));
      /*
        if ORC.send_email(msg) {

        } else {
          if ORC.dev_mode {
            println!("Auth email failed to send for: {}", &ar.email);
          }
          ORC.destroy_preauth_token(&preauth_token);
          return responses::InternalServerError("Auth email failed to send, are you sure your email is in good order?");
        }
      */
      }
    } else {
      return responses::InternalServerError("magic-link creation failed");
    }
  }

  responses::Forbidden("not working, we might be under attack")
}

#[get("/user/{id}/description")]
pub async fn get_user_description(id: web::Path<u64>) -> HttpResponse {
  let usr_id = id.to_be_bytes();

  if let Ok(Some(desc)) = ORC.user_descriptions.get(&usr_id) {
    return responses::Ok(desc.to_string());
  }

  responses::NotFound("No user description found, sorry")
}

#[post("/user/change/{detail}")]
pub async fn change_user_detail(req: HttpRequest, detail: web::Path<String>, value: web::Json<String>) -> HttpResponse {
  match detail.as_str() {
    "username" => {
      if !is_username_ok(value.as_str()) {
        return responses::BadRequest("invalid username");
      }

      if let Some(mut usr) = ORC.user_by_session(&req) {
        if let Some(err) = ORC.change_username(&mut usr, value.as_str()) {
          match err {
            UserError::UsernameTaken => {
              return responses::BadRequest("username already taken or blacklisted")
            },
            UserError::ChangedUsernameTooSoon => {
              return responses::InternalServerError("Username change limit reached, you can only change it twice a week, you may change it again a week after the second change");
            },
            UserError::DBIssue => {
              return responses::InternalServerError("database troubles, failed to change username");
            },
            _ => {},
          }
        } else {
          return responses::AcceptedStatusData(
          "username successfully changed",
            value.to_owned()
          );
        }
      } else {
        return responses::Forbidden("can't change anything if you're not logged in");
      }
    },
    "handle" => {
      if !is_handle_ok(value.as_str()) {
        return responses::BadRequest("invalid user handle");
      }

      if let Some(mut usr) = ORC.user_by_session(&req) {
        if let Some(err) = ORC.change_handle(&mut usr, value.as_str()) {
          match err {
            UserError::HandleTaken => {
              return responses::BadRequest("user handle already taken or blacklisted")
            },
            UserError::DBIssue => {
              return responses::InternalServerError("database troubles, failed to change username");
            },
            _ => {},
          }
        } else {
          return responses::AcceptedStatusData(
          "user handle successfully changed",
            value.to_owned()
          );
        }
      } else {
        return responses::Forbidden("can't change anything if you're not logged in");
      }
    },
    "description" => {
      if let Some(usr) = ORC.user_by_session(&req) {
        if ORC.change_description(&usr, value.as_str()) {
          return responses::Accepted("user description successfully changed");
        }
      } else {
        return responses::Forbidden("can't change anything if you're not logged in");
      }
    },
    _ => {
      return responses::BadRequest("invalid user detail");
    }
  }
  
  responses::InternalServerError("failed to change user details")
}

fn build_the_usual_cookie<'c, N, V>(
  name: N,
  value: V
) -> Cookie<'c> where
  N: Into<std::borrow::Cow<'c, str>>,
  V: Into<std::borrow::Cow<'c, str>>
{
  if !ORC.dev_mode {
    Cookie::build(name, value)
      .domain(CONF.read().domain.clone())
      .max_age(time::Duration::seconds(ORC.expiry_tll))
      .path("/")
      .http_only(true)
      .secure(true)
      .finish()
  } else {
    Cookie::build(name, value)
      .max_age(time::Duration::seconds(ORC.expiry_tll))
      .path("/")
      .http_only(true)
      .finish()
  }
}

fn build_cookie_with_ttl<'c, N, V>(name: N, value: V, seconds: i64) -> Cookie<'c>
where
  N: Into<std::borrow::Cow<'c, str>>,
  V: Into<std::borrow::Cow<'c, str>>,
{
  if !ORC.dev_mode {
    Cookie::build(name, value)
      .domain(CONF.read().domain.clone())
      .max_age(time::Duration::seconds(seconds))
      .path("/")
      .http_only(true)
      .secure(true)
      .finish()
  } else {
    Cookie::build(name, value)
      .max_age(time::Duration::seconds(seconds))
      .path("/")
      .http_only(true)
      .finish()
  }
}
