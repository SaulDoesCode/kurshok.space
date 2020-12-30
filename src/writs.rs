use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use borsh::{BorshDeserialize, BorshSerialize};
use itertools::Itertools;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sled::{transaction::*, IVec, Transactional};
use thiserror::Error;
use time::OffsetDateTime;

use std::convert::TryInto;

// use super::CONF;
use crate::auth::User;
use crate::comments::Comment;
use crate::orchestrator::{Orchestrator, ORC};
use crate::utils::{datetime_from_unix_timestamp, render_md, unix_timestamp, FancyBool, FancyIVec};

impl Orchestrator {
  pub fn new_writ_id(&self, author_id: u64, kind: &[u8]) -> Option<WritID> {
    if kind.len() == 4 {
      return WritID::new(kind, author_id);
    }
    None
  }

  pub fn index_writ_tags(&self, writ_id: &WritID, tags: &[String]) -> bool {
    let res: TransactionResult<(), ()> =
      (&self.tags_index, &self.tag_counter).transaction(|(tags_index, tag_counter)| {
        self.index_writ_tags_in_transaction(tags_index, tag_counter, writ_id, tags)
      });
    res.is_ok()
  }

  pub fn index_writ_tags_in_transaction(
    &self,
    tags_index: &TransactionalTree,
    tag_counter: &TransactionalTree,
    writ_id: &WritID,
    tags: &[String],
  ) -> ConflictableTransactionResult<(), ()> {
    let mut id = String::new();
    for tag in tags.iter() {
      id = format!("{}:{}", &tag, id);
      let count: u64 = match tag_counter.get(tag.as_bytes())? {
        Some(raw_count) => raw_count.to_u64(),
        None => 0,
      };
      tag_counter.insert(tag.as_bytes(), &(count + 1).to_be_bytes())?;
    }
    id = format!("{}:{}", id, writ_id.to_string());
    tags_index.insert(id.as_bytes(), writ_id.to_bin())?;
    Ok(())
  }

  pub fn remove_indexed_writ_tags(&self, writ_id: &WritID, tags: &[String]) -> bool {
    (&self.tags_index, &self.tag_counter).transaction(|(
      tags_index,
      tag_counter
    )| self.remove_indexed_writ_tags_in_transaction(
      tags_index,
      tag_counter,
      writ_id,
      tags
    )).is_ok()
  }

  pub fn remove_indexed_writ_tags_in_transaction(
    &self,
    tags_index: &TransactionalTree,
    tag_counter: &TransactionalTree,
    writ_id: &WritID, 
    tags: &[String]
  ) -> ConflictableTransactionResult<(), ()> {
    let mut id = String::new();
    for tag in tags.iter() {
      id = format!("{}:{}", &tag, id);
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
    id = format!("{}:{}", id, writ_id.to_string());
    tags_index.remove(id.as_bytes())?;
    Ok(())
  }

  pub fn remove_writ(&self, author_id: u64, writ_id: &WritID) -> bool {
    if writ_id.author == author_id &&
      !self.is_admin(author_id)
    {
      return false;
    }

    let res: TransactionResult<(), ()> = (
      &self.content,
      &self.raw_content,
      &self.titles,
      &self.slugs,
      &self.dates,
      &self.votes,
      &self.writs,
      &self.tags_index,
      &self.tag_counter,
      &self.comment_settings,
    )
      .transaction(
        |(
          ctn,
          raw_ctn,
          titles,
          slugs,
          dates,
          votes,
          writs,
          tags_index,
          tag_counter,
          comment_settings,
        )| {
          let wid_vec = writ_id.to_bin();
          let wid = wid_vec.as_slice();

          let writ: Writ = match writs.get(wid)? {
            Some(w) => Writ::try_from_slice(&w).unwrap(),
            None => return Err(sled::transaction::ConflictableTransactionError::Abort(())),
          };

          writs.remove(wid)?;
          votes.remove(wid)?;
          ctn.remove(wid)?;
          if raw_ctn.get(wid)?.is_some() {
            raw_ctn.remove(wid)?;
          }
          comment_settings.remove(wid)?;

          self.remove_indexed_writ_tags_in_transaction(
            tags_index,
            tag_counter,
            &writ_id,
            writ.tags.as_slice()
          )?;

          titles.remove(writ.title_key().as_bytes())?;
          slugs.remove(writ.slug_key().as_bytes())?;
          dates.remove(writ.date_key().as_bytes())?;

          Ok(())
        },
      );

    if res.is_ok() {
      let mut iter = self.comments.scan_prefix(writ_id.to_string());
      while let Some(Ok(res)) = iter.next() {
        let comment = Comment::try_from_slice(&res.1).unwrap();
        // TODO: handle this in a safer way
        comment.remove();
      }

      return true;
    }
    false
  }

  pub fn writ_query(&self, mut query: WritQuery, o_usr: Option<&User>) -> Option<Vec<Writ>> {
    let is_admin = o_usr.as_ref().map_or(false, |usr| self.is_admin(usr.id.clone()));

    let amount = *query.amount.as_ref().unwrap_or(&20);

    if (!is_admin && amount > 50) || amount > 500 {
      return None;
    }

    let mut writs: Vec<Writ> = vec![];
    let mut count: u64 = 0;

    let mut author_ids: Option<Vec<sled::IVec>> = None;
    if let Some(authors) = &query.authors {
      author_ids = Some(
        authors.par_iter()
          .filter_map(|a| {
            self.usernames.get(a.as_bytes()).unwrap_or(None)
          })
          .collect()
      );
    } else if query.author_id.is_none() {
      if let Some(name) = &query.author_name {
        let mut found = false;
        if let Some(usr) = &o_usr {
          if usr.username == *name {
            found = true;
            query.author_id = Some(usr.id);
          }
        }
        if !found {
          if let Ok(Some(id)) = self.usernames.get(name.as_bytes()) {
            query.author_id = Some(id.to_u64());
          } else {
            return None;
          }
        }
      } else if let Some(handle) = &query.author_handle {
        let mut found = false;
        if let Some(usr) = &o_usr {
          if usr.handle == *handle {
            found = true;
            query.author_id = Some(usr.id);
          }
        }
        if !found {
          if let Ok(Some(id)) = self.handles.get(handle.as_bytes()) {
            query.author_id = Some(id.to_u64());
          } else {
            return None;
          }
        }
      }
    }

    let user_attributes = o_usr.as_ref().map(|usr| self.user_attributes(usr.id));

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
        let posted = datetime_from_unix_timestamp(writ.posted);
        if let Some(y) = &query.year {
          if posted.year() != *y {
            return false;
          }
        }
        if let Some(m) = &query.month {
          if posted.month() != *m {
            return false;
          }
        }
        if let Some(d) = &query.day {
          if posted.day() != *d {
            return false;
          }
        }
        if let Some(h) = &query.hour {
          if posted.hour() != *h {
            return false;
          }
        }
      }

      let author_id = match writ.author_id() {
        Some(au_id) => au_id,
        None => return false,
      };

      if let Some(usr) = &o_usr {
        if author_id == usr.id || is_admin {
          if let Some(public) = &query.public {
            if writ.public != *public {
              return false;
            }
          }
        } else if let Some(viewable_by) = &query.viewable_by {
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
        if !ids.contains(&IVec::from_u64(author_id)) {
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
      let id_iter = {
        let mut iter = ids.iter();
        if query.page > 0 {
          let skip_n = (query.page * amount) as usize;
          if ids.len() < skip_n || iter.advance_by(skip_n).is_err() {
            return None;
          }
        }
        iter
      };

      for id in id_iter {
        if count == amount {
          break;
        }

        if let Some(skip_ids) = &query.skip_ids {
          if skip_ids.contains(id) {
            continue;
          }
        }

        if let Some(author_id) = &query.author_id {
          if !id.contains(&format!(":{}:", *author_id)) {
            continue;
          }
        }

        let wid= match WritID::from_str(id) {
          Some(wid) => wid,
          None => continue,
        };

        let writ = match self.writs.get(&wid.to_bin()) {
          Ok(raw) => Writ::try_from_slice(&raw.unwrap()).unwrap(),
          Err(_) => continue,
        };

        if check_writ_against_query(&writ, false) {
          count += 1;
          writs.push(writ);
        }
      }
    } else {
      let mut date = String::new();
      let now = OffsetDateTime::now_utc();
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
        let mut partial_id = vec![];
        partial_id.extend_from_slice(query.kind.as_bytes());
        if let Some(author_id) = &query.author_id {
          partial_id.extend_from_slice(&author_id.to_be_bytes());
        }
        self.writs.scan_prefix(&partial_id)
      };

      if query.page > 0 {
        let skip_n = (query.page * amount) as usize;
        if writ_iter.advance_back_by(skip_n).is_err() {
          return None;
        }
      }

      while let Some(Ok(res)) = writ_iter.next_back() {
        if count == amount { break; }

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
          if let Ok(Some(w)) = self.writs.get(res.1) {
            Writ::try_from_slice(&w).unwrap()
          } else {
            continue;
          }
        } else {
          let w = Writ::try_from_slice(&res.1).unwrap();
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
    o_usr: Option<&User>,
  ) -> Option<Vec<PublicWrit>> {
    let usr_id = o_usr.as_ref().map(|usr| usr.id);
    let with_content = query.with_content.unwrap_or(true);
    if let Some(writs) = self.writ_query(query, o_usr) {
      let public_writs = writs
        .into_par_iter()
        .filter_map(|w| w.public(&usr_id, with_content))
        .collect::<Vec<PublicWrit>>();

      if public_writs.len() > 0 {
        return Some(public_writs);
      }
    }
    None
  }

  pub fn editable_writ_query(&self, mut query: WritQuery, usr: &User) -> Option<Vec<EditableWrit>> {
    query.author_id = Some(usr.id.clone());

    let with_content = query.with_content.unwrap_or(false);
    let with_raw_content = query.with_raw_content.unwrap_or(true);

    self.writ_query(query, Some(&usr)).and_then(|writs| {
      let editable_writs = writs
        .into_par_iter()
        .filter_map(|w| w.editable(&usr, with_content, with_raw_content))
        .collect::<Vec<EditableWrit>>();

      (editable_writs.len() > 0).qualify(editable_writs)
    })
  }

  pub fn writ_by_id(&self, id: &str) -> Option<Writ> {
    if let Some(wid) = WritID::from_str(id) {
      return match self.writs.get(&wid.to_bin()) {
        Ok(w) => w.map(|raw| Writ::try_from_slice(&raw).unwrap()),
        Err(_) => None,
      };
    }
    None
  }

  pub fn writ_and_id_from_str(&self, id: &str) -> Option<(Writ, WritID)> {
    if let Some(wid) = WritID::from_str(id) {
      return match self.writs.get(&wid.to_bin()) {
        Ok(w) => w.map(|raw| (Writ::try_from_slice(&raw).unwrap(), wid)),
        Err(_) => None,
      };
    }
    None
  }

  pub fn writ_by_id_bytes(&self, id: &[u8]) -> Option<Writ> {
    match self.writs.get(id) {
      Ok(w) => w.map(|raw| Writ::try_from_slice(&raw).unwrap()),
      Err(_) => None,
    }
  }

  pub fn writ_by_title(&self, kind: &str, title: &str) -> Option<Writ> {
    let key = format!("{}:{}", kind, title);
    if let Ok(Some(wid)) = self.titles.get(key.as_bytes()) {
      return match self.writs.get(wid) {
        Ok(w) => w.map(|raw| Writ::try_from_slice(&raw).unwrap()),
        Err(_) => None,
      };
    }
    None
  }

  pub fn writ_by_slug(&self, kind: &str, slug: &str) -> Option<Writ> {
    let key = format!("{}:{}", kind, slug);
    if let Ok(Some(wid)) = self.slugs.get(key.as_bytes()) {
      return match self.writs.get(wid) {
        Ok(w) => w.map(|raw| Writ::try_from_slice(&raw).unwrap()),
        Err(_) => None,
      };
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
  pub author_id: Option<u64>,

  pub posted_before: Option<i64>,
  pub posted_after: Option<i64>,

  pub year: Option<i32>,
  pub month: Option<u8>,
  pub day: Option<u8>,
  pub hour: Option<u8>,

  pub amount: Option<u64>,
  pub page: u64,

  pub with_content: Option<bool>,
  pub with_raw_content: Option<bool>,

  pub kind: String,
}

impl std::default::Default for WritQuery {
  fn default() -> Self {
    WritQuery {
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

pub struct WritID {
  kind: [u8; 4],
  author: u64,
  id: u64,
}

impl WritID {
  pub fn to_bin(&self) -> Vec<u8> {
    let mut id: Vec<u8> = Vec::with_capacity(20);
    // 4 bytes
    id.extend_from_slice(&self.kind);
    // 8 bytes
    id.extend_from_slice(&self.author.to_be_bytes());
    // 8 bytes
    id.extend_from_slice(&self.id.to_be_bytes());
    // = 20 bytes
    id
  }

  pub fn from_bin(bin: &[u8]) -> Self {
    let kind: [u8; 4] = (&bin[..4]).try_into().unwrap();
    let author = u64::from_be_bytes((&bin[4..8]).try_into().unwrap());
    let id = u64::from_be_bytes((&bin[8..12]).try_into().unwrap());

    Self{kind, author, id}
  }

  pub fn new(kind: &[u8], author: u64) -> Option<Self> {
    if kind.len() == 4 {
      if let Ok(id) = ORC.generate_id(&kind) {
        return Some(Self{
          kind: kind.try_into().unwrap(),
          author,
          id
        });
      }
    }
    None
  }

  pub fn to_string(&self) -> String {
    let kind = String::from_utf8(self.kind.to_vec()).unwrap();
    format!("{}:{}:{}", kind, self.author, self.id)
  }
  
  pub fn from_str(wid: &str) -> Option<Self> {
    if let Some((kind, author, id)) = wid.split(":").collect_tuple() {
      if kind.len() == 4 {
        if let Ok(kind) = kind.as_bytes().try_into() {
          if let Ok(author) = author.parse() {
            if let Ok(id) = id.parse() {
              return Some(Self{kind, author, id});
            }
          }
        }
      }
    }
    None
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
  posted: i64,
  commentable: bool,
  you_voted: Option<bool>,
  vote: i64,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Writ {
  pub id: String, // {kind}:{author_id}:{writ_id}
  pub title: String,
  pub slug: String,
  pub kind: String,
  pub tags: Vec<String>,
  pub posted: i64,
  pub public: bool,
  pub viewable_by: Vec<String>,
  pub commentable: bool,
  pub is_md: bool,
}

impl Writ {
  #[inline]
  pub fn author_id(&self) -> Option<u64> {
    self.id.split(":")
      .nth(1)
      .and_then(|au_id| match au_id.parse::<u64>() {
        Ok(au_id) => Some(au_id),
        Err(_) => {
          if ORC.dev_mode {
            println!("failed to read author_id from writ.id - {}", self.id);
          }
          None
        },
      })
  }

  #[inline]
  pub fn unique_id(&self) -> u64 {
    self.id.split(":").nth(2).unwrap().parse::<u64>().unwrap()
  }

  pub fn writ_id(&self) -> Option<WritID> {
    WritID::from_str(&self.id)
  }

  pub fn content(&self) -> Option<String> {
    if let Some(wid) = self.writ_id() {
      if let Ok(Some(c)) = ORC.content.get(&wid.to_bin()) {
        return Some(c.to_string());
      }
    }
    None
  }

  pub fn raw_content(&self) -> Option<String> {
    if let Some(wid) = self.writ_id() {
      if let Ok(c) = ORC.raw_content.get(&wid.to_bin()) {
        return c.map(|c| c.to_string());
      }
    }
    None
  }

  pub fn comment_settings(&self) -> Option<CommentSettings> {
    if self.commentable {
      if let Some(wid) = self.writ_id() {
        if let Ok(Some(raw)) = ORC.comment_settings.get(&wid.to_bin()) {
          return Some(CommentSettings::try_from_slice(&raw).unwrap());
        }
      }
    }
    None
  }

  pub fn public(
    &self,
    requestor_id: &Option<u64>,
    with_content: bool,
  ) -> Option<PublicWrit> {
    let author_id = match self.author_id() {
      Some(au_id) => au_id,
      None => return None,
    };

    if !self.public {
      if let Some(req_id) = requestor_id {
        if author_id != *req_id {
          return None;
        }
      } else {
        return None;
      }
    }

    let author = if let Some(author) = ORC.user_by_id(author_id) {
      author
    } else {
      if ORC.dev_mode {
        println!("writ.public: no such author");
      }
      return None;
    };

    let writ_id = match WritID::from_str(&self.id) {
      Some(wid) => wid,
      None => return None,
    };
    let wid = writ_id.to_bin();

    let res: TransactionResult<PublicWrit, ()> = (
      &ORC.content,
      &ORC.votes,
      &ORC.writ_voters
    ).transaction(|(content_tree, votes, writ_voters)| {
        let vote: i64 = if let Some(res) = votes.get(&wid)? {
          res.to_i64()
        } else {
          0
        };

        let content = if with_content {
          if let Some(res) = content_tree.get(&wid)? {
            Some(res.to_string())
          } else {
            if ORC.dev_mode {
              println!("writ.public: could not retrieve content");
            }
            return Err(sled::transaction::ConflictableTransactionError::Abort(()));
          }
        } else {
          None
        };

        let you_voted = match &requestor_id {
          Some(req_id) => writ_voters
            .get(self.vote_id(*req_id).as_bytes())?
            .map(|raw| Vote::try_from_slice(&raw).unwrap().up),
          None => None,
        };

        Ok(PublicWrit {
          id: self.id.clone(),
          title: self.title.clone(),
          author_name: author.username.clone(),
          author_handle: author.handle.clone(),
          kind: self.kind.clone(),
          tags: self.tags.clone(),
          posted: self.posted,
          content,
          commentable: self.commentable,
          vote,
          you_voted,
        })
      });

    match res {
      Ok(pw) => Some(pw),
      Err(e) => {
        if ORC.dev_mode {
          println!("writ.public crapped out with: {:?}", e);
        }
        None
      }
    }
  }

  pub fn editable(
    &self,
    author: &User,
    with_content: bool,
    with_raw_content: bool,
  ) -> Option<EditableWrit> {
    let author_id = match self.author_id() {
      Some(au_id) => au_id,
      None => return None,
    };

    if author_id != author.id {
      return None;
    }

    let writ_id = match self.writ_id() {
      Some(wid) => wid,
      None => return None,
    };
    let wid_vec = writ_id.to_bin();
    let wid = wid_vec.as_slice();

    let content = if with_content {
      match ORC.content.get(wid) {
        Ok(Some(raw)) => Some(raw.to_string()),
        Ok(None) | Err(_) => None,
      }
    } else {
      None
    };
    let raw_content = if with_raw_content {
      match ORC.raw_content.get(wid) {
        Ok(Some(raw)) => Some(raw.to_string()),
        Ok(None) | Err(_) => None,
      }
    } else {
      None
    };

    Some(EditableWrit {
      id: self.id.clone(),
      title: self.title.clone(),
      slug: self.slug.clone(),
      tags: self.tags.clone(),
      posted: self.posted,
      content,
      raw_content,
      kind: self.kind.clone(),
      public: self.public,
      viewable_by: self.viewable_by.clone(),
      commentable: self.commentable,
      is_md: self.is_md,
    })
  }

  pub fn vote(&self, usr_id: u64, up: Option<bool>) -> Option<i64> {
    let writ_id = match self.writ_id() {
      Some(wid) => wid,
      None => return None,
    };
    let res: TransactionResult<i64, ()> =
    (&ORC.votes, &ORC.writ_voters).transaction(|(votes, writ_voters)| {
        let vote_id = self.vote_id(usr_id);
        let wid_vec = writ_id.to_bin();
        let wid = wid_vec.as_slice();

        let mut count = votes.get(wid)?.unwrap().to_i64();
  
        if let Some(raw) = writ_voters.get(vote_id.as_bytes())? {
          let rw = Vote::try_from_slice(&raw).unwrap();
          if let Some(up) = &up {
            // prevent double voting
            if rw.up == *up {
              return Err(sled::transaction::ConflictableTransactionError::Abort(()));
            }
            // handle when they alreay voted and now vote the oposite way
            if *up {
              count += 2;
            } else {
              count -= 2;
            }
            votes.insert(wid, &count.to_be_bytes())?;
          } else {
            // unvote
            writ_voters.remove(vote_id.as_bytes())?;

            if rw.up {
              count -= 1;
            } else {
              count += 1;
            }

            votes.insert(wid, &count.to_be_bytes())?;

            return Ok(count);
          }
        } else if up.is_none() {
          return Err(sled::transaction::ConflictableTransactionError::Abort(()));
        } else {
          if up.clone().unwrap() {
            count += 1;
          } else {
            count -= 1;
          }
          votes.insert(wid, &count.to_be_bytes())?;
        }

        let wv = Vote {
          id: vote_id,
          when: unix_timestamp(),
          up: up.unwrap(),
        };
        writ_voters.insert(wv.id.as_bytes(), wv.try_to_vec().unwrap())?;

        Ok(count)
      });

    match res {
      Ok(count) => Some(count),
      Err(e) => {
        if ORC.dev_mode {
          println!("Something bad went down with voting - {:?}", e);
        }
        None
      }
    }
  }

  pub fn upvote(&self, usr_id: u64) -> Option<i64> {
    self.vote(usr_id, Some(true))
  }

  pub fn downvote(&self, usr_id: u64) -> Option<i64> {
    self.vote(usr_id, Some(false))
  }

  pub fn unvote(&self, usr_id: u64) -> Option<i64> {
    self.vote(usr_id, None)
  }

  pub fn usr_vote(&self, usr_id: u64) -> Option<Vote> {
    match ORC.writ_voters.get(self.vote_id(usr_id).as_bytes()) {
      Ok(wv) => wv.map(|raw| Vote::try_from_slice(&raw).unwrap()),
      Err(_) => None,
    }
  }

  #[inline]
  pub fn vote_id(&self, usr_id: u64) -> String {
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
    let posted = datetime_from_unix_timestamp(self.posted);
    format!(
      "{}:{}{}{}{}:{}",
      self.kind,
      posted.year(),
      posted.month(),
      posted.day(),
      posted.hour(),
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
  pub posted: i64,
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
  pub fn commit(&self, author_id: u64) -> Result<Writ, WritError> {
    let is_md = self.is_md.unwrap_or(true);
    if !is_md && 
      !ORC.user_has_some_attrs(author_id, &["writer", "admin"])
        .unwrap_or(false)
    {
      return Err(WritError::NoPermNoMD);
    }

    let tags: Vec<String> = self.tags.iter()
      .map(|t| t.trim().replace("  ", "-").replace(" ", "-"))
      .collect();

    if !RawWrit::are_tags_valid(&tags) {
      return Err(WritError::InvalidTags);
    }

    let (writ_id, is_new_writ) = match &self.id {
      Some(wid) => {
        let wid = match WritID::from_str(wid) {
          Some(wid) => wid,
          None => return Err(WritError::BadID),
        };

        if wid.author != author_id {
          return Err(WritError::InauthenticAuthor);
        }

        if let Ok(has_id) = ORC.writs.contains_key(wid.to_bin()) {
          if !has_id {
            return Err(WritError::NonExistentID);
          }
        } else {
          return Err(WritError::DBIssue);
        }

        (wid, false)
      }
      None => {
        if let Some(writ_id) = ORC.new_writ_id(author_id, self.kind.as_bytes()) {
          (writ_id, true)
        } else {
          return Err(WritError::IDGenErr);
        }
      }
    };

    let writ = Writ {
      id: writ_id.to_string(),
      slug: slug::slugify(&self.title),
      posted: unix_timestamp(),
      title: self.title.clone(),
      kind: self.kind.clone(),
      tags,
      public: self.public,
      commentable: self.commentable.unwrap_or(true),
      viewable_by: self.viewable_by.clone().unwrap_or(vec![]),
      is_md,
    };

    if is_new_writ && ORC.titles.contains_key(writ.title_key().as_bytes()).unwrap() {
      return Err(WritError::TitleTaken);
    }

    let author_attrs = ORC.user_attributes(author_id);
    if !writ.viewable_by.iter().all(|t| author_attrs.contains(t)) {
      return Err(WritError::UsedUnavailableAttributes);
    }

    let raw_content = self.raw_content.trim();

    if !ORC.dev_mode && is_new_writ {
      // hash contents and ratelimit with it to prevent spam
      let rc_hash = ORC.hash(raw_content.as_bytes());
      let mut hitter = Vec::from("wr".as_bytes());
      hitter.extend_from_slice(&rc_hash);
      if let Some(rl) = ORC.ratelimiter.hit(&hitter, 1, time::Duration::minutes(360)) {
        if rl.is_timing_out() {
          return Err(WritError::DuplicateWrit);
        }
      } else {
        return Err(WritError::DBIssue);
      }
    }

    let res: TransactionResult<(), ()> = (
      &ORC.content,
      &ORC.raw_content,
      &ORC.titles,
      &ORC.slugs,
      &ORC.dates,
      &ORC.votes,
      &ORC.writs,
      &ORC.tags_index,
      &ORC.tag_counter,
      &ORC.comment_settings,
    )
      .transaction(
        |(
          ctn,
          raw_ctn,
          titles,
          slugs,
          dates,
          votes,
          writs,
          tags_index,
          tag_counter,
          comment_settings,
        )| {
          let wid_vec = writ_id.to_bin();
          let wid = wid_vec.as_slice();

          let mut new_writ = writ.clone();

          if writ.is_md {
            raw_ctn.insert(wid, raw_content.as_bytes())?;
            ctn.insert(wid, render_md(raw_content).as_bytes())?;
          } else {
            ctn.insert(wid, raw_content.as_bytes())?;
          }

          if is_new_writ {
            ORC.index_writ_tags_in_transaction(
              tags_index,
              tag_counter,
              &writ_id,
              new_writ.tags.as_slice()
            )?;

            titles.insert(new_writ.title_key().as_bytes(), wid)?;
            slugs.insert(new_writ.slug_key().as_bytes(), wid)?;

            dates.insert(new_writ.date_key().as_bytes(), wid)?;

            votes.insert(wid, &0i64.to_be_bytes())?;

            comment_settings.insert(
              wid,
              CommentSettings::default(new_writ.public)
                .try_to_vec()
                .unwrap(),
            )?;
          } else {
            let old_writ = Writ::try_from_slice(&writs.get(wid)?.unwrap()).unwrap();

            if new_writ.kind != old_writ.kind
              || new_writ.title != old_writ.title
              || new_writ.slug != old_writ.slug
            {
              writs.remove(wid)?;
              titles.remove(old_writ.title_key().as_bytes())?;
              titles.insert(new_writ.title_key().as_bytes(), wid)?;

              slugs.remove(old_writ.slug_key().as_bytes())?;
              slugs.insert(new_writ.slug_key().as_bytes(), wid)?;

              let settings = CommentSettings::try_from_slice(
                &comment_settings.get(wid)?.unwrap()
              ).unwrap();
              comment_settings.insert(wid, settings.try_to_vec().unwrap())?;
            }

            if new_writ.tags != old_writ.tags {
              ORC.remove_indexed_writ_tags_in_transaction(
                tags_index,
                tag_counter,
                &writ_id,
                old_writ.tags.as_slice()
              )?;

              ORC.index_writ_tags_in_transaction(
                tags_index,
                tag_counter,
                &writ_id,
                new_writ.tags.as_slice()
              )?;
            }

            if old_writ.is_md && !new_writ.is_md {
              raw_ctn.remove(wid)?;
            }

            if new_writ.posted != old_writ.posted {
              new_writ.posted = old_writ.posted;
              // dates.remove(old_writ.date_key().as_bytes())?;
              // dates.insert(writ.date_key().as_bytes(), writ_id)?;
            }
          }

          writs.insert(wid, new_writ.try_to_vec().unwrap())?;

          Ok(())
        },
      );

    match res {
      Ok(_) => Ok(writ),
      Err(_) => {
        /* if self.dev_mode {
          println!("writ creation pooped out: {}", err);
        } */
        Err(WritError::DBIssue)
      }
    }
  }

  pub fn are_tags_valid(tags: &Vec<String>) -> bool {
    tags.par_iter().all(|t| {
      t.len() >= 1 && t.len() <= 22 && 
      t.chars()
          .all(|c| c.is_alphanumeric() || c.is_whitespace() || c == '-')
    })
  }

  pub fn writ(&self) -> Option<Writ> {
    if let Some(id) = &self.id {
      return ORC.writ_by_id(id.as_str());
    }
    None
  }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Debug)]
pub struct Vote {
  pub id: String,
  pub up: bool,
  pub when: i64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, PartialEq, Debug)]
pub struct CommentSettings {
  pub public: bool,
  pub visible_to: Option<Vec<u64>>,
  pub min_comment_length: Option<u64>,
  pub max_comment_length: Option<u64>,
  pub disqualified_strs: Option<Vec<String>>,
  pub hide_when_vote_below: Option<i64>,
  pub max_level: Option<u64>,
  pub notify_author: bool,
  pub notifying_stops_beyond_level: Option<u64>,
}

impl CommentSettings {
  pub fn default(public: bool) -> Self {
    Self {
      public,
      visible_to: None,
      min_comment_length: Some(5),
      max_comment_length: Some(8000),
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
  #[error("no such id in database, cannot update writ")]
  NonExistentID,
  #[error("id generation failed for some reason, maybe try again later")]
  IDGenErr,
  #[error("author's id mismatches writ's author_id")]
  InauthenticAuthor,
  #[error("please see to it that all writ tags are alphanumeric and no longer than 20 chars")]
  InvalidTags,
  #[error("duplicate writ, please don't copy")]
  DuplicateWrit,
  #[error("writ made viewable_only with attributes the author user lacks")]
  UsedUnavailableAttributes,
  #[error("writ title is already used, choose a different one")]
  TitleTaken,
  #[error("there was a problem interacting with the db")]
  DBIssue,
  #[error("only authorized users may push non-markdown writs")]
  RateLimit,
  #[error("too many requests to writ api, chill for a bit")]
  NoPermNoMD,
  #[error("unknown writ error")]
  Unknown,
}

#[get("/writ-raw-content/{id}")]
pub async fn writ_raw_content(
  req: HttpRequest,
  wid: web::Path<String>,
) -> HttpResponse {
  // TODO: ratelimiting
  if let Some(wid) = WritID::from_str(wid.as_str()) {
    if let Some(usr) = ORC.user_by_session(&req) {
      if wid.author == usr.id {
        if let Ok(Some(raw_rw)) = ORC.raw_content.get(&wid.to_bin()) {
          return crate::responses::Ok(raw_rw.to_string());
        } else {
          return crate::responses::NotFound("writ id didn't match anything of yours");
        }
      }
    }
  }

  crate::responses::Forbidden(
    "You can't load the raw_contents of writs if you aren't logged in or if the contents in question aren't yours"
  )
}

#[get("/post-content/{id}")]
pub async fn post_content(
  req: HttpRequest,
  pid: web::Path<String>,
) -> HttpResponse {
  // TODO: ratelimiting
  if let Some((writ, wid)) = ORC.writ_and_id_from_str(&pid) {
    if !writ.public {
      if let Some(usr) = ORC.user_by_session(&req) {
        if usr.id != wid.author {
          return crate::responses::Forbidden(
            "You can't load the contents of private writs that aren't yours",
          );
        }
      } else {
        return crate::responses::Forbidden("You can't load the contents of private writs");
      }
    }

    if let Ok(Some(raw_c)) = ORC.content.get(&wid.to_bin()) {
      return crate::responses::Ok(raw_c.to_string());
    }
  }

  crate::responses::NotFound("post ID is either malformed or didn't match anything of yours")
}

#[post("/writs")]
pub async fn writ_query(
  req: HttpRequest,
  query: web::Json<WritQuery>,
) -> HttpResponse {
  let o_usr = ORC.user_by_session(&req);
  if let Some(writs) =
    ORC.public_writ_query(query.into_inner(), o_usr.as_ref())
  {
    return HttpResponse::Ok().json(writs);
  }

  crate::responses::NotFound("writ query didn't match anything, perhaps reformulate")
}

#[post("/editable-writs")]
pub async fn editable_writ_query(
  req: HttpRequest,
  query: web::Json<WritQuery>,
) -> HttpResponse {
  if let Some(usr) = ORC.user_by_session(&req) {
    if let Some(writs) = ORC.editable_writ_query(query.into_inner(), &usr) {
      return HttpResponse::Ok().json(writs);
    }
  } else {
    return crate::responses::Forbidden("You can't edit things that aren't yours to edit");
  }

  crate::responses::NotFound("writ query didn't match anything, perhaps reformulate")
}

#[put("/writ")]
pub async fn push_raw_writ(
  req: HttpRequest,
  rw: web::Json<RawWrit>,
) -> HttpResponse {
  if rw.raw_content.len() > 200_000 {
    return crate::responses::BadRequest(
      "Your writ is too long, it has to be less than 200k characters",
    );
  }

  if let Some(usr_id) = ORC.user_id_by_session(&req) {
    if ORC
      .user_has_some_attrs(usr_id, &["writer", "admin"])
      .unwrap_or(false)
    {
      return match rw.commit(usr_id) {
        Ok(w) => crate::responses::Ok(w),
        Err(e) => crate::responses::BadRequest(format!("error: {}", e)),
      };
    }
  }

  crate::responses::Forbidden("only authorized may post writs")
}

#[delete("/writ")]
pub async fn delete_writ(
  req: HttpRequest,
  body: web::Bytes,
) -> HttpResponse {
  if let Ok(writ_id) = String::from_utf8(body.to_vec()) {
    if let Some(writ_id) = WritID::from_str(&writ_id) {
      if let Some(usr_id) = ORC.user_id_by_session(&req) {
        return match ORC.remove_writ(usr_id, &writ_id) {
          true => crate::responses::Accepted("writ has been removed"),
          false => crate::responses::BadRequest("invalid data, could not remove writ"),
        };
      }
    }
  }

  crate::responses::Forbidden("only authorized users may remove writs")
}

#[get("/writ/{wrid_id}/upvote")]
pub async fn upvote_writ(
  req: HttpRequest,
  writ_id: web::Path<String>,
) -> HttpResponse {
  if let Some(usr_id) = ORC.user_id_by_session(&req) {   
    if let Some(writ) = ORC.writ_by_id(&writ_id) {
      if let Some(count) = writ.upvote(usr_id) {
        return crate::responses::AcceptedStatusData("vote went through", count);
      }
    }
  } else {
    return crate::responses::Forbidden("only users may vote on writs");
  }

  crate::responses::InternalServerError("failed to register vote")
}

#[get("/writ/{wrid_id}/downvote")]
pub async fn downvote_writ(
  req: HttpRequest,
  writ_id: web::Path<String>,
) -> HttpResponse {
  if let Some(usr_id) = ORC.user_id_by_session(&req) {
    if let Some(writ) = ORC.writ_by_id(&writ_id) {
      if let Some(count) = writ.downvote(usr_id) {
        return crate::responses::AcceptedStatusData("vote went through", count);
      }
    }
  } else {
    return crate::responses::Forbidden("only users may vote on writs");
  }

  crate::responses::InternalServerError("failed to register vote")
}

#[get("/writ/{wrid_id}/unvote")]
pub async fn unvote_writ(
  req: HttpRequest,
  writ_id: web::Path<String>,
) -> HttpResponse {
  if let Some(usr_id) = ORC.user_id_by_session(&req) {
    if let Some(writ) = ORC.writ_by_id(&writ_id) {
      if let Some(count) = writ.unvote(usr_id) {
        return crate::responses::AcceptedStatusData("vote went through", count);
      }
    }
  } else {
    return crate::responses::Forbidden("only users may vote on writs");
  }

  crate::responses::InternalServerError("failed to register vote")
}

// TODO: prevent malformed ids from even getting to sled .get/.insert/.delete
#[inline]
fn is_valid_writ_id(id: &str) -> bool {
  let len = id.len();
  len > 2 && len < 30 && id.matches(":").count() > 0
}
