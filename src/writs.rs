use chrono::{offset::Utc, prelude::*, Duration};
use serde::{Deserialize, Serialize};
use sled::{IVec, Transactional, transaction::*};
use thiserror::Error;

use std::sync::Arc;

use actix_web::{get, post, put, delete, web, HttpRequest, HttpResponse};

// use super::CONF;
use crate::orchestrator::Orchestrator;
use crate::auth::{User};
use crate::comments::Comment;
use crate::utils::{
  binbe_serialize,
  get_struct,
  render_md,
  FancyIVec,
  IntoBin,
};

impl Orchestrator {
  pub fn new_writ_id(&self, author_id: &str, kind: &str) -> Option<String> {
    if let Ok(id) = self.generate_id("writ".as_bytes()) {
      return Some(format!("{}:{}:{}", kind, author_id, id));
    }
    None
  }

  pub fn index_writ_tags(&self, writ_id: String, tags: Vec<String>) -> bool {
    let res: TransactionResult<(), ()> = (&self.tags_index, &self.tag_counter).transaction(|(tags_index, tag_counter)| {
      for tag in tags.iter() {
        let id = format!("{}:{}", tag.as_str(), writ_id);
        tags_index.insert(id.as_bytes(), binbe_serialize(&tags))?;
        let count: u64 = match tag_counter.get(tag.as_bytes())? {
          Some(raw_count) => raw_count.to_u64(),
          None => 0,
        };
        tag_counter.insert(tag.as_bytes(), &(count + 1).to_be_bytes())?;
      }
      Ok(())
    });
    
    res.is_ok()
  }

  pub fn remove_indexed_writ_tags(&self, writ_id: String, tags: Vec<String>) -> bool {
    (&self.tags_index, &self.tag_counter).transaction(|(tags_index, tag_counter)| {
      for tag in tags.iter() {
        let id = format!("{}:{}", tag, writ_id);
        tags_index.remove(id.as_bytes())?;
        let count: u64 = match tag_counter.get(tag.as_bytes())? {
          Some(raw_count) => raw_count.to_u64(),
          None => return Err(sled::transaction::ConflictableTransactionError::Abort(())),
        };
        if count <= 1 {
          tag_counter.remove(tag.as_bytes())?;
        } else {
          tag_counter.insert(tag.as_bytes(), IVec::from_u64(count - 1))?;
        }
      }
      Ok(())
    }).is_ok()
  }

  pub fn writ_query(&self, mut query: WritQuery, o_usr: Option<&User>) -> Option<Vec<Writ>> {
    let is_admin = if let Some(usr) = &o_usr {
      self.is_admin(&usr.id)
    } else {
      false
    };

    let amount = if let Some(a) = &query.amount { a.clone() } else { 20 };

    if !is_admin {
      if amount > 50 { return None; }
    } else if amount > 500 { return None; }

    let mut writs: Vec<Writ> = vec!();
    let mut count: u64 = 0;

    let mut author_ids: Option<Vec<sled::IVec>> = None;
    if let Some(authors) = &query.authors {
      let mut ids = vec!();
      for a in authors {
        if let Ok(Some(id)) = self.usernames.get(a.as_bytes()) {
          ids.push(id);
        }
      }
      author_ids = Some(ids);
    } else if query.author_id.is_none() {
      if let Some(name) = &query.author_name {
        if let Ok(Some(id)) = self.usernames.get(name.as_bytes()) {
          query.author_id = Some(id.to_string());
        } else {
          return None;
        }
      } else if let Some(handle) = &query.author_handle {
        if let Ok(Some(id)) = self.handles.get(handle.as_bytes()) {
          query.author_id = Some(id.to_string());
        } else {
          return None;
        }
      }
    }

    let user_attributes = o_usr.as_ref().map_or(None, |usr| Some(self.user_attributes(&usr.id)));

    let check_writ_against_query = |writ: &Writ, date_scan: bool| {
      if let Some(posted_before) = &query.posted_before {
        if writ.posted > *posted_before {
          return false;
        }
      }

      if let Some(posted_after) = &query.posted_after {
        if writ.posted < *posted_after {
          return false;
        }
      }

      if !date_scan {
        if let Some(y) = &query.year {
          if writ.posted.year() != *y {
            return false;
          }
        }
        if let Some(m) = &query.month {
          if writ.posted.month() != *m {
            return false;
          }
        }
        if let Some(d) = &query.day {
          if writ.posted.day() != *d {
            return false;
          }
        }
        if let Some(h) = &query.hour {
          if writ.posted.hour() != *h {
            return false;
          }
        }
      }

      let author_id = writ.author_id();

      if let Some(usr) = &o_usr {
        if author_id == usr.id || is_admin {
          if let Some(public) = &query.public {
            if writ.public != *public {
              return false;
            }
          } else if !writ.public {
            return false;
          }
        }

        if let Some(viewable_by) = &query.viewable_by {
          if let Some(attrs) = &user_attributes {  
            if !viewable_by.iter().all(|a| attrs.contains(a)) {
              return false;
            }
          } else {
            return false;
          };
        }
      } else if !writ.public {
        return false;
      }

      if let Some(ids) = &author_ids {
        let author_id_bytes = author_id.as_bytes();
        if !ids.contains(&author_id_bytes.into()) {
          return false;
        }
      }

      if let Some(tags) = &query.tags {
        for tag in tags {
          if !writ.tags.contains(tag) {
            return false;
          }
        }
      }

      if let Some(omit_tags) = &query.omit_tags {
        for tag in omit_tags {
          if writ.tags.contains(tag) {
            return false;
          }
        }
      }

      // todo handle this upfront because these are unique indexes
      // possibly also allow some kind of fuzzing or partial completeness
      if let Some(title) = &query.title {
        if writ.title != *title {
          return false;
        }
      } else if let Some(slug) = &query.slug {
        if writ.slug != *slug {
          return false;
        }
      }

      true
    };

    if let Some(ids) = &query.ids {
      for id in ids {
        if query.page < 2 {
            if count == amount { break; }
        } else if count != (amount * query.page) {
            count += 1;
            continue;
        }

        if let Some(skip_ids) = &query.skip_ids {
          if skip_ids.contains(id) {
            continue;
          }
        }

        if let Some(author_id) = &query.author_id {
          if !id.contains(&format!(":{}:", author_id)) {
            continue;
          }
        }

        let writ: Writ = if let Some(w) = get_struct(&self.writs, id.as_bytes()) {
          w
        } else {
          continue;
        };

        if check_writ_against_query(&writ, false) {
          count += 1;
          writs.push(writ);
        }
      }
    } else {
      let mut date = String::new();
      let now = Utc::now();
      if let Some(y) = &query.year {
        date.push_str(&format!("{}", y));
      }
      if let Some(m) = &query.month {
        if date.is_empty() {
          date.push_str(&format!("{}", now.year()));
        }
        if *m > 12 || *m == 0 {
          return None;
        }
        date.push_str(&format!("{}", m));
      }
      if let Some(d) = &query.day {
        if date.is_empty() {
          date.push_str(&format!("{}", now.year()));
        }
        if query.month.is_none() {
          date.push_str(&format!("{}", now.month()));
        }
        if *d > 31 || *d == 0 {
          return None;
        }
        date.push_str(&format!("{}", d));
      }
      if let Some(h) = &query.hour {
        if date.is_empty() {
          date.push_str(&format!("{}", now.year()));
        }
        if query.month.is_none() {
          date.push_str(&format!("{}", now.month()));
        }
        if query.day.is_none() {
          date.push_str(&format!("{}", now.day()));
        }
        if *h > 24 {
          return None;
        }
        date.push_str(&format!("{}", h));
      }

      let date_scan = !date.is_empty();

      let mut writ_iter = if date_scan {
        let partial_date_id = format!("{}:{}", query.kind, date);
        self.dates.scan_prefix(partial_date_id.as_bytes())
      } else {
        let mut partial_id = format!("{}:", query.kind);
        if let Some(author_id) = &query.author_id {
          partial_id.push_str(author_id);
        }
        self.writs.scan_prefix(partial_id.as_bytes())
      };

      while let Some(Ok(res)) = writ_iter.next_back() {
        if query.page < 2 {
            if count == amount { break; }
        } else if count != (amount * query.page) {
            count += 1;
            continue;
        }

        let writ: Writ = if date_scan {
          let id = res.1.to_string();
          if let Some(skip_ids) = &query.skip_ids {
            if skip_ids.contains(&id) {
                continue;
            }
          }
          if let Some(author_id) = &query.author_id {
            if !id.contains(&format!(":{}:", author_id)) {
              continue;
            }
          }
          if let Some(w) = get_struct(&self.writs, &res.1) {
            w
          } else {
            continue;
          }
        } else {
          let w: Writ = res.1.to_type();
          if let Some(skip_ids) = &query.skip_ids {
            if skip_ids.contains(&w.id) {
                continue;
            }
          }
          w
        };

        if check_writ_against_query(&writ, date_scan) {
          count += 1;
          writs.push(writ);
        }
      }
    }

    if writs.len() == 0 {
      return None;
    }
    Some(writs)
  }

  pub fn public_writ_query(
    &self,
    query: WritQuery,
    o_usr: Option<User>,
  ) -> Option<Vec<PublicWrit>> {
    let usr_id = match &o_usr { Some(usr) => Some(usr.id.clone()), None => None, };
    let with_content = query.with_content.unwrap_or(true);
    if let Some(writs) = self.writ_query(query, o_usr.as_ref()) {
      let mut public_writs = vec!();
      for w in writs {
        // if !w.public {continue;}
        if let Some(pw) = w.public(self, &usr_id, with_content) {
          public_writs.push(pw);
        }
      }

      if public_writs.len() > 0 {
        return Some(public_writs);
      }
    }
    None
  }

  pub fn editable_writ_query(&self, mut query: WritQuery, usr: User) -> Option<Vec<EditableWrit>> {
    query.author_id = Some(usr.id.clone());

    let with_content = query.with_content.unwrap_or(false);
    let with_raw_content = query.with_raw_content.unwrap_or(true);

    if let Some(writs) = self.writ_query(query, Some(&usr)) {
      let mut editable_writs = vec!();
      for w in writs {
        if let Some(pw) = w.editable(self, &usr, with_content, with_raw_content) {
          editable_writs.push(pw);
        }
      }

      if editable_writs.len() > 0 {
        return Some(editable_writs);
      }
    }
    None
  }

  pub fn writ_by_id(&self, id: &str) -> Option<Writ> {
    get_struct(&self.writs, id.as_bytes())
  }

  pub fn writ_by_title(&self, kind: &str, title: &str) -> Option<Writ> {
    let key = format!("{}:{}", kind, title);
    if let Some(w) = self.titles.get(key.as_bytes()).unwrap() {
      return get_struct(&self.writs, &w);
    }
    None
  }

  pub fn writ_by_slug(&self, kind: &str, slug: &str) -> Option<Writ> {
    let key = format!("{}:{}", kind, slug);
    if let Some(w) = self.slugs.get(key.as_bytes()).unwrap() {
      return get_struct(&self.writs, &w);
    }
    None
  }
}

#[serde(deny_unknown_fields)]
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct WritQuery {
    pub title: Option<String>,
    pub slug: Option<String>,
    
    pub tags: Option<Vec<String>>,
    pub omit_tags: Option<Vec<String>>,
    pub viewable_by: Option<Vec<String>>,

    pub ids: Option<Vec<String>>,
    pub skip_ids: Option<Vec<String>>,
    
    pub authors: Option<Vec<String>>,
    
    pub public: Option<bool>,
    pub author_name: Option<String>,
    pub author_handle: Option<String>,
    pub author_id: Option<String>,
    
    pub posted_before: Option<DateTime<Utc>>,
    pub posted_after: Option<DateTime<Utc>>,
    
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub day: Option<u32>,
    pub hour: Option<u32>,
    
    pub amount: Option<u64>,
    pub page: u64,

    pub with_content: Option<bool>,
    pub with_raw_content: Option<bool>,

    pub kind: String,
}

impl std::default::Default for WritQuery {
  fn default() -> Self {
    WritQuery{
      title: None,
      slug: None,
      tags: None,
      omit_tags: None,
      viewable_by: None,
      ids: None,
      skip_ids: None,
      authors: None,
      public: None,
      author_name: None,
      author_handle: None,
      author_id: None,
      posted_before: None,
      posted_after: None,
      year: None,
      month: None,
      day: None,
      hour: None,
      with_content: None,
      with_raw_content: None,
      amount: None,
      page: 0,
      kind: "post".to_string(),
    }
  }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct PublicWrit {
  id: String, // {author_id}:{writ_id}
  author_name: String,
  author_handle: String,
  title: String,
  kind: String,
  content: Option<String>,
  tags: Vec<String>,
  posted: DateTime<Utc>,
  you_voted: Option<bool>,
  vote: i64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Writ {
  pub id: String, // {kind}:{author_id}:{writ_id}
  pub title: String,
  pub slug: String,
  pub kind: String,
  pub tags: Vec<String>,
  pub posted: DateTime<Utc>,
  pub public: bool,
  pub viewable_by: Vec<String>,
  pub commentable: bool,
  pub is_md: bool,
}

impl Writ {
  #[inline]
  pub fn author_id(&self) -> &str {
    self.id.split(":").nth(1).unwrap()
  }

  #[inline]
  pub fn unique_id(&self) -> &str {
    self.id.split(":").nth(2).unwrap()
  }

  pub fn content(&self, orc: &Orchestrator) -> Option<String> {
    if let Ok(Some(c)) = orc.content.get(self.id.as_bytes()) {
      return Some(c.to_string());
    }
    None
  }

  pub fn raw_content(&self, orc: &Orchestrator) -> Option<String> {
    if let Ok(Some(c)) = orc.raw_content.get(self.id.as_bytes()) {
      return Some(c.to_string());
    }
    None
  }

  pub fn comment_settings(&self, orc: &Orchestrator) -> Option<CommentSettings> {
    if self.commentable {
      return get_struct(&orc.comment_settings, self.id.as_bytes());
    }
    None
  }

  pub fn public(
    &self,
    orc: &Orchestrator,
    requestor_id: &Option<String>,
    with_content: bool,
  ) -> Option<PublicWrit> {
    let author_id = self.author_id();

    if !self.public {
      if let Some(req_id) = requestor_id {
        if author_id != *req_id {
          return None;
        }
      } else {
        return None;
      }
      if orc.dev_mode {
        println!("writ.public: got past public viewability checking");
      }
    }

    let author = if let Some(author) = orc.user_by_id(author_id) {
      author
    } else {
      if orc.dev_mode {
        println!("writ.public: no such author");
      }
      return None;
    };

    let res: TransactionResult<PublicWrit, ()> = (
      &orc.content,
      &orc.votes,
      &orc.writ_voters,
    ).transaction(|(
      content_tree,
      votes,
      writ_voters,
    )| {
      let vote: i64 = if let Some(res) = votes.get(self.id.as_bytes())? {
        res.to_i64()
      } else {
        0
      };
  
      let content = if with_content {
        if let Some(res) = content_tree.get(self.id.as_bytes())? {
          Some(res.to_string())
        } else {
          if orc.dev_mode {
            println!("writ.public: could not retrieve content");
          }
          return Err(sled::transaction::ConflictableTransactionError::Abort(()));
        }
      } else {
        None
      };
  
      let you_voted = if let Some(req_id) = &requestor_id {
        if let Some(raw) = writ_voters.get(self.vote_id(req_id.as_str()).as_bytes())? {
          Some(raw.to_type::<WritVote>().up)
        } else {
          None
        }
      } else {
        None
      };

      Ok(PublicWrit{
        id: self.id.clone(),
        title: self.title.clone(),
        author_name: author.username.clone(),
        author_handle: author.handle.clone(),
        kind: self.kind.clone(),
        tags: self.tags.clone(),
        posted: self.posted,
        content,
        vote,
        you_voted,
      })
    });

    match res {
      Ok(pw) => Some(pw),
      Err(e) => {
        if orc.dev_mode {
          println!("writ.public crapped out with: {:?}", e);
        }
        None
      }
    }
  }

  pub fn editable(
    &self,
    orc: &Orchestrator,
    author: &User,
    with_content: bool,
    with_raw_content: bool,
  ) -> Option<EditableWrit> {
    let author_id = self.author_id();
    if author_id != author.id {
      return None;
    }

    Some(EditableWrit{
      id: self.id.clone(),
      title: self.title.clone(),
      slug: self.slug.clone(),
      tags: self.tags.clone(),
      posted: self.posted.clone(),
      content: if with_content {
        if let Ok(Some(raw)) = orc.content.get(self.id.as_bytes()) {
          Some(raw.to_string())
        } else {
          return None;
        }
      } else {
        None
      },
      raw_content: if with_raw_content {
        if let Ok(Some(raw)) = orc.raw_content.get(self.id.as_bytes()) {
          Some(raw.to_string())
        } else {
          return None;
        }
      } else {
        None
      },
      kind: self.kind.clone(),
      public: self.public,
      viewable_by: self.viewable_by.clone(),
      commentable: self.commentable,
      is_md: self.is_md,
    })
  }

  pub fn vote(&self, orc: &Orchestrator, usr_id: &str, up: bool) -> bool {
    let res: TransactionResult<(), ()> = (&orc.votes, &orc.writ_voters).transaction(|(votes, writ_voters)| {
      let wv = WritVote{id: self.vote_id(usr_id), when: Utc::now(), up};
      writ_voters.insert(self.vote_id(usr_id).as_bytes(), wv.to_bin())?;
      let count: i64 = votes.get(self.id.as_bytes())?.unwrap().to_i64();
      votes.insert(self.id.as_bytes(), &(count + 1).to_be_bytes())?;
      Ok(())
    });
    res.is_ok()
  }

  pub fn upvote(&self, orc: &Orchestrator, usr_id: &str) -> bool {
    self.vote(orc, usr_id, true)
  }

  pub fn downvote(&self, orc: &Orchestrator, usr_id: &str) -> bool {
    self.vote(orc, usr_id, false)
  }

  pub fn usr_vote(&self, orc: &Orchestrator, usr_id: &str) -> Option<WritVote> {
    get_struct(&orc.writ_voters, self.vote_id(usr_id).as_bytes())
  }

  #[inline]
  pub fn vote_id(&self, usr_id: &str) -> String {
    format!("{}:{}", self.id, usr_id)
  }

  #[inline]
  pub fn title_key(&self) -> String {
    format!("{}:{}", self.kind, self.title)
  }

  #[inline]
  pub fn slug_key(&self) -> String {
    format!("{}:{}", self.kind, self.slug)
  }

  #[inline]
  pub fn date_key(&self) -> String {
    format!("{}:{}{}{}{}:{}",
      self.kind,
      self.posted.year(),
      self.posted.month(),
      self.posted.day(),
      self.posted.hour(),
      self.unique_id()
    )
  }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct EditableWrit {
  pub id: String, // {kind}:{author_id}:{writ_id}
  pub title: String,
  pub slug: String,
  pub kind: String,
  pub tags: Vec<String>,
  pub posted: DateTime<Utc>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub raw_content: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub content: Option<String>,
  pub public: bool,
  pub viewable_by: Vec<String>,
  pub commentable: bool,
  pub is_md: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct RawWrit {
  pub id: Option<String>,
  pub title: String,
  pub raw_content: String,
  pub kind: String,
  pub tags: Vec<String>,
  pub public: bool,
  pub commentable: Option<bool>,
  pub viewable_by: Option<Vec<String>>,
  pub is_md: Option<bool>,
}

impl RawWrit {
  pub fn commit(&self, author: &User, orc: &Orchestrator) -> Result<Writ, WritError> {
    let is_md = self.is_md.unwrap_or(true);
    if is_md && !orc.is_admin(&author.id) {
      return Err(WritError::NoPermNoMD);
    }

    let (writ_id, is_new_writ) = match &self.id {
      Some(wi) => {
        if let Some(author_id) = wi.split(":").nth(1) {
          if author_id != author.id {
            return Err(WritError::InauthenticAuthor);  
          }
        }

        if !orc.writs.contains_key(wi.as_bytes()).unwrap_or(true) {
          return Err(WritError::BadID);
        }

        (wi.clone(), false)
      },
      None => if let Some(writ_id) = orc.new_writ_id(&author.id, &self.kind) {
        (writ_id, true)
      } else {
        return Err(WritError::IDGenErr);
      }
    };

    let writ = Writ{
      id: writ_id,
      slug: slug::slugify(&self.title),
      posted: Utc::now(),
      title: self.title.clone(),
      kind: self.kind.clone(),
      tags: self.tags.clone(),
      public: self.public,
      commentable: self.commentable.unwrap_or(true),
      viewable_by: self.viewable_by.clone().unwrap_or(vec!()),
      is_md,
    };

    let author_attrs = orc.user_attributes(&author.id);
    if !writ.viewable_by.iter().all(|t| author_attrs.contains(t)) {
      return Err(WritError::UsedUnavailableAttributes);
    }

    if is_new_writ && orc.titles.contains_key(writ.title_key().as_bytes()).unwrap() {
      return Err(WritError::TitleTaken);
    }

    let raw_content = self.raw_content.trim();

    // hash contents and ratelimit with it to prevent spam
    let rc_hash = orc.hash(raw_content.as_bytes());
    let mut hitter = Vec::from("wr".as_bytes());
    hitter.extend_from_slice(&rc_hash);
    let rl = orc.ratelimiter.hit(&hitter, 1, Duration::hours(8760));
    if rl.is_timing_out() {
      return Err(WritError::DuplicateWrit);
    }

    let res: TransactionResult<(), ()> = (
      &orc.content,
      &orc.raw_content,
      &orc.titles,
      &orc.slugs,
      &orc.dates,
      &orc.votes,
      &orc.writs,
      &orc.tags_index,
      &orc.tag_counter,
      &orc.comment_settings,
    ).transaction(|(
      ctn,
      raw_ctn,
      titles,
      slugs,
      dates,
      votes,
      writs,
      tags_index,
      tag_counter,
      comment_settings
    )| {
      let writ_id = writ.id.as_bytes();
      
      let mut new_writ = writ.clone();

      if writ.is_md {
        raw_ctn.insert(writ_id, raw_content.as_bytes())?;
        ctn.insert(writ_id, render_md(raw_content).as_bytes())?;
      } else {
        ctn.insert(writ_id, raw_content.as_bytes())?;
      }

      if is_new_writ {
        for tag in new_writ.tags.iter() {
          let id = format!("{}:{}", tag.as_str(), new_writ.id);
          tags_index.insert(id.as_bytes(), binbe_serialize(&tag))?;

          let count: u64 = tag_counter.get(tag.as_bytes())?
            .map_or(0, |raw| raw.to_u64());
          tag_counter.insert(tag.as_bytes(), IVec::from_u64(count + 1))?;
        }

        titles.insert(new_writ.title_key().as_bytes(), writ_id)?;
        slugs.insert(new_writ.slug_key().as_bytes(), writ_id)?;

        dates.insert(new_writ.date_key().as_bytes(), writ_id)?;

        votes.insert(writ_id, &0i64.to_be_bytes())?;

        comment_settings.insert(
          writ_id,
          binbe_serialize(&CommentSettings::default(new_writ.id.clone(), new_writ.public))
        )?;
      } else {
        let old_writ: Writ = writs.get(writ_id)?.unwrap().to_type();
        
        if new_writ.kind != old_writ.kind || new_writ.id != old_writ.id || new_writ.title != old_writ.title  || new_writ.slug != old_writ.slug {
          writs.remove(old_writ.id.as_bytes())?;
          titles.remove(old_writ.title_key().as_bytes())?;
          titles.insert(new_writ.title_key().as_bytes(), writ_id)?;

          slugs.remove(old_writ.slug_key().as_bytes())?;
          slugs.insert(new_writ.slug_key().as_bytes(), writ_id)?;

          let mut settings: CommentSettings = comment_settings.get(old_writ.id.as_bytes())?.unwrap().to_type();
          settings.id = new_writ.id.clone();
          comment_settings.insert(writ_id, binbe_serialize(&settings))?;
        }

        if new_writ.tags != old_writ.tags {
          for tag in old_writ.tags.iter() {
            let id = format!("{}:{}", tag, new_writ.id);
            tags_index.remove(id.as_bytes())?;
            let count: u64 = tag_counter.get(tag.as_bytes())?.unwrap().to_u64();
            if count <= 1 {
              tag_counter.remove(tag.as_bytes())?;
            } else {
              tag_counter.insert(tag.as_bytes(), &(count - 1).to_be_bytes())?;
            }
          }
          for tag in new_writ.tags.iter() {
            let id = format!("{}:{}", tag, new_writ.id);
            tags_index.remove(id.as_bytes())?;
            let count: u64 = tag_counter.get(tag.as_bytes())?.unwrap().to_u64();
            if count <= 1 {
              tag_counter.remove(tag.as_bytes())?;
            } else {
              tag_counter.insert(tag.as_bytes(), &(count - 1).to_be_bytes())?;
            }
          }
        }

        if old_writ.is_md && !new_writ.is_md {
          raw_ctn.remove(writ_id)?;
        }

        if new_writ.posted != old_writ.posted {
          new_writ.posted = old_writ.posted.clone();
          // dates.remove(old_writ.date_key().as_bytes())?;
          // dates.insert(writ.date_key().as_bytes(), writ_id)?;
        }
      }

      writs.insert(writ_id, new_writ.to_bin())?;

      Ok(())
    });

    if res.is_ok() {
      return Ok(writ);
    }

    Err(WritError::DBIssue)
  }

  pub fn remove(&self, orc: &Orchestrator, author: &User) -> Option<Writ> {
    if let Some(writ) = self.writ(orc) {
      if writ.author_id() != author.id {
        return None;
      }

      if (
        &orc.content,
        &orc.raw_content,
        &orc.titles,
        &orc.slugs,
        &orc.dates,
        &orc.votes,
        &orc.writs,
        &orc.tags_index,
        &orc.tag_counter,
        &orc.comment_settings
      ).transaction(|(
        ctn,
        raw_ctn,
        titles,
        slugs,
        dates,
        votes,
        writs,
        tags_index,
        tag_counter,
        comment_settings
      )| {
        let writ_id = writ.id.as_bytes();
        writs.remove(writ_id)?;
        votes.remove(writ_id)?;
        ctn.remove(writ_id)?;
        if raw_ctn.get(writ_id)?.is_some() {
          raw_ctn.remove(writ_id)?;
        }
        comment_settings.remove(writ_id)?;

        for tag in writ.tags.iter() {
          let id = format!("{}:{}", tag, writ.id);
          tags_index.remove(id.as_bytes())?;
          let count: u64 = tag_counter.get(tag.as_bytes())?.unwrap().to_u64();
          if count <= 1 {
            tag_counter.remove(tag.as_bytes())?;
          } else {
            tag_counter.insert(tag.as_bytes(), &(count - 1).to_be_bytes())?;
          }
        }

        titles.remove(writ.title_key().as_bytes())?;
        slugs.remove(writ.slug_key().as_bytes())?;
        dates.remove(writ.date_key().as_bytes())?;

        let mut iter = orc.comments.scan_prefix(writ_id);
        while let Some(res) = iter.next() {
          let comment = res?.1.to_type::<Comment>();
          if !comment.remove(orc.clone()) {
            return Err(sled::transaction::ConflictableTransactionError::Abort(()));
          }
        }

        Ok(())
      }).is_ok() {
        return Some(writ);
      }
    }
    None
  }

  pub fn writ(&self, orc: &Orchestrator) -> Option<Writ> {
    if let Some(id) = &self.id {
      return get_struct(&orc.writs, id.as_bytes());
    }
    None
  }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct WritVote {
  pub id: String,
  pub up: bool,
  pub when: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct CommentSettings {
  pub id: String, // writ_id
  pub public: bool,
  pub visible_to: Option<Vec<String>>,
  pub min_comment_length: Option<usize>,
  pub max_comment_length: Option<usize>,
  pub disqualified_strs: Option<Vec<String>>,
  pub hide_when_vote_below: Option<i64>,
  pub max_level: Option<usize>,
  pub notify_author: bool,
  pub notifying_stops_beyond_level: Option<usize>,
}

impl CommentSettings {
  pub fn default(id: String, public: bool) -> Self {
    Self{
      id,
      public,
      visible_to: None,
      min_comment_length: Some(5),
      max_comment_length: Some(5000),
      disqualified_strs: None,
      hide_when_vote_below: Some(10),
      max_level: Some(32),
      notify_author: true,
      notifying_stops_beyond_level: None,
    }
  }
}

#[derive(Error, Debug)]
pub enum WritError {
    #[error("id does not match any currently existing writ")]
    BadID,
    #[error("id generation failed for some reason, maybe try again later")]
    IDGenErr,
    #[error("author's id mismatches writ's author_id")]
    InauthenticAuthor,
    #[error("duplicate writ, please don't copy")]
    DuplicateWrit,
    #[error("writ made viewable_only with attributes the author user lacks")]
    UsedUnavailableAttributes,
    #[error("writ title is already used, choose a different one")]
    TitleTaken,
    #[error("there was a problem interacting with the db")]
    DBIssue,
    #[error("only authorized users may push non-markdown writs")]
    NoPermNoMD,
    #[error("unknown writ error")]
    Unknown,
}

#[post("/writs")]
pub async fn writ_query(
  req: HttpRequest,
  query: web::Json<WritQuery>,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  let o_usr = orc.user_by_session(&req);
  if let Some(writs) = orc.public_writ_query(query.into_inner(), o_usr) {
    return HttpResponse::Ok().json(writs);
  }

  crate::responses::NotFound(
    "writ query didn't match anything, perhaps reformulate"
  )
}

#[post("/editable-writs")]
pub async fn editable_writ_query(
  req: HttpRequest,
  query: web::Json<WritQuery>,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  if let Some(usr) = orc.user_by_session(&req) {
    if let Some(writs) = orc.editable_writ_query(query.into_inner(), usr) {
      return HttpResponse::Ok().json(writs);
    }
  } else {
    return crate::responses::Forbidden(
      "You can't edit things that aren't yours to edit"
    );
  }

  crate::responses::NotFound(
    "writ query didn't match anything, perhaps reformulate"
  )
}

#[put("/writ")]
pub async fn push_raw_writ(
  req: HttpRequest,
  rw: web::Json<RawWrit>,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  if let Some(usr) = orc.admin_by_session(&req) {
    return match rw.commit(&usr, orc.as_ref()) {
      Ok(w) => crate::responses::Ok(w),
      Err(e) => crate::responses::BadRequest(format!("error: {}", e)),
    };
  }

  crate::responses::Forbidden("only admins may post writs")
}

#[delete("/writ")]
pub async fn delete_writ(
  req: HttpRequest,
  rw: web::Json<RawWrit>,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  if let Some(usr) = orc.admin_by_session(&req) {
    return match rw.remove(orc.as_ref(), &usr) {
      Some(_) => crate::responses::Accepted("writ has been removed"),
      None => crate::responses::BadRequest("invalid data, could not remove writ"),
    };
  }

  crate::responses::Forbidden("only admins may post writs")
}

#[get("/writ/{wrid_id}/upvote")]
pub async fn upvote_writ(
  req: HttpRequest,
  writ_id: web::Path<String>,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  if let Some(usr) = orc.user_by_session(&req) {
    if let Some(writ) = orc.writ_by_id(&writ_id) {
      if writ.upvote(orc.as_ref(), &usr.id) {
        return crate::responses::Accepted("vote went through")
      }
    }
  } else {
    return crate::responses::Forbidden(
      "only users may vote on writs"
    );
  }

  crate::responses::InternalServerError(
    "failed to register vote"
  )
}

#[get("/writ/{wrid_id}/downvote")]
pub async fn downvote_writ(
  req: HttpRequest,
  writ_id: web::Path<String>,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  if let Some(usr) = orc.user_by_session(&req) {
    if let Some(writ) = orc.writ_by_id(&writ_id) {
      if writ.downvote(orc.as_ref(), &usr.id) {
        return crate::responses::Accepted("vote went through");
      }
    }
  } else {
    return crate::responses::Forbidden("only users may vote on writs");
  }

  crate::responses::InternalServerError("failed to register vote")
}
