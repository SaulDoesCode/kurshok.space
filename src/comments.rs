use actix_web::{delete, post, put, web, HttpRequest, HttpResponse};
use chrono::{offset::Utc, prelude::*, Duration};
use serde::{Deserialize, Serialize};
use sled::{IVec, transaction::*, Transactional};
use std::{cell::Cell, collections::HashMap, sync::Arc};

use itertools::Itertools;
use rayon::prelude::*;

use crate::orchestrator::Orchestrator;
use crate::auth::{User};
use crate::utils::{
  binbe_serialize,
  get_struct,
  i64_is_zero,
  render_md,
  FancyBool,
  FancyIVec,
  IntoBin,
};
use crate::writs::{CommentSettings, Writ};

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct CommentVote {
  pub id: String,
  pub up: bool,
  pub when: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct PublicComment {
  pub id: String,
  pub content: String,
  pub posted: DateTime<Utc>,
  #[serde(skip_serializing_if = "i64_is_zero")]
  pub votes: i64,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub edited: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub you_voted: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub author_only: Option<bool>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Comment {
  pub id: String, // {meta_id}/{author_id}:{unique}/...
  pub author_name: String,
  pub content: String,
  pub posted: DateTime<Utc>,
  pub edited: bool,
  pub public: bool,
  pub author_only: bool,
}

impl Comment {
  pub fn new(id: String, author_name: String, content: String) -> Self {
    Self{
      id,
      author_name,
      content,
      posted: Utc::now(),
      edited: false,
      public: false,
      author_only: false,
    }
  }

  pub fn from_id(orc: &Orchestrator, id: &[u8]) -> Option<Comment> {
    get_struct(&orc.comments, id)
  }

  pub fn key_path(&self, orc: &Orchestrator) -> Option<String> {
    if let Ok(Some(raw)) = orc.comment_key_path_index.get(self.id.as_bytes()) {
      return Some(raw.to_string());
    }
    None
  }

  pub fn new_first_level_id(
    orc: &Orchestrator,
    writ_id: &str,
    usr_id: &str,
  ) -> Option<(String, String)> {
    if let Ok(uid) = orc.generate_id(writ_id.as_bytes()) {
      let own_id = format!("{}:{}", usr_id, uid);
      return Some((
        format!("{}/{}", writ_id, own_id),
        own_id
      ));
    }
    None
  }
  pub fn new_subcomment_id(
    orc: &Orchestrator,
    writ_id: &str,
    parent_id: &str,
    usr_id: &str,
  ) -> Option<(String, String)> {
    if let Ok(uid) = orc.generate_id(writ_id.as_bytes()) {
      let own_id = format!("{}:{}", usr_id, uid);
      return Some((
        format!("{}/{}", parent_id, own_id),
        own_id
      ));
    }
    None
  }

  pub fn writ_id(&self) -> String {
    self.id[..self.id.chars().position(|c| c == '/').unwrap()].to_string()
  }

  pub fn unique_id(&self) -> String {
    self.id[self.id.chars().rev().position(|c| c == '/').unwrap()..].to_string()
  }

  pub fn author_id(&self) -> String {
    Self::get_author_id_from_id(&self.id).to_string()
  }
  
  pub fn get_author_id_from_id(id: &str) -> &str {
    if id.contains('/') {
      return id.split('/').last().unwrap().split(':').next().unwrap();
    }
    return id.split(':').next().unwrap();
  }

  pub fn vote_id(&self, usr_id: &str) -> String {
    format!("{}<{}", self.id, usr_id)
  }

  pub fn public(self, orc: &Orchestrator, usr_id: &Option<String>) -> Option<PublicComment> {
    Some(PublicComment{
      posted: self.posted,
      edited: self.edited.wrap(), 
      author_only: self.author_only.wrap(),
      you_voted: match usr_id {
        Some(id) => get_struct::<CommentVote>(
          &orc.comment_voters,
          self.vote_id(id).as_bytes()
        ).and_then(|cv| cv.up.wrap()),
        None => None,
      },
      votes: if let Ok(Some(raw)) = orc.comment_votes.get(self.id.as_bytes()) {
        raw.to_i64()
      } else {
        return None;
      },
      id: self.id,
      content: self.content,
    })
  }

  pub fn default_deleted(&self) -> Self {
    Self{
      id: self.id.clone(),
      author_name: "_".to_string(),
      content: "[deleted]".to_string(),
      posted: self.posted,
      public: self.public,
      edited: self.edited,
      author_only: self.author_only,
    }
  }

  pub fn delete(&self, orc: &Orchestrator) -> bool {
    let deleted_comment = self.default_deleted();
    let res: TransactionResult<(), ()> = (
      &orc.comment_voters,
      &orc.comments,
      &orc.comment_raw_content,
      &orc.comment_votes
    ).transaction(|(voters, comments, comment_raw_content, votes)| {
      comments.insert(self.id.as_bytes(), deleted_comment.to_bin())?;
      comment_raw_content.remove(self.id.as_bytes())?;
      votes.remove(self.id.as_bytes())?;
      let mut iter = orc.comment_voters.scan_prefix(self.id.as_bytes());
      while let Some(pair) = iter.next() {
        voters.remove(pair?.1)?;
      }
      Ok(())
    });
    res.is_ok()
  }

  pub fn get_root_comment_id(&self, orc: &Orchestrator) -> Option<String> {
    if self.id.matches(":").count() > 1 {
      return Some(self.id.clone())
    }
    if let Ok(Some(raw_key)) = orc.comment_key_path_index.get(self.id.as_bytes()) {
      let full_id = raw_key.to_string();
      let mut parts: Vec<&str> = full_id.split('/').filter(|&c| c != "").collect();
      let root_id = parts.drain(..2).join("/");
      return Some(root_id.to_string());
    }
    None
  }

  pub fn get_root_comment_id_and_path(
    &self,
    orc: &Orchestrator,
  ) -> Option<(String, Vec<String>)> {
    if self.id.matches(":").count() > 1 {
      return Some((self.id.clone(), {
        self.id.split('/').filter(|&c| c != "").map(|part| part.to_string()).collect()
      }));
    }

    if let Ok(Some(raw_key)) = orc.comment_key_path_index.get(self.id.as_bytes()) {
      let full_id = raw_key.to_string();
      let mut parts: Vec<&str> = full_id.split('/').filter(|&c| c != "").collect();
      let path = {
        let mut path = vec!();
        for part in &parts {
          path.push(part.to_string());
        }
        path
      };
      let root_id = parts.drain(..2).join("/");
      return Some((root_id.to_string(), path));
    }
    None
  }

  pub fn get_root_comment(&self, orc: &Orchestrator) -> Option<Comment> {
    if self.id.matches(":").count() > 1 {
      return Some(self.clone())
    }
    if let Ok(Some(raw_key)) = orc.comment_key_path_index.get(self.id.as_bytes()) {
      let full_id = raw_key.to_string();
      let mut parts: Vec<&str> = full_id.split('/').filter(|&c| c != "").collect();
      let root_id = parts.drain(..2).join("/");
      return get_struct(&orc.comments, root_id.as_bytes());
    }
    None
  }

  pub fn remove_in_transaction(
    &self,
    orc: &Orchestrator,
    kpi: &TransactionalTree,
    ctrees: &TransactionalTree,
    comments: &TransactionalTree,
    raw_contents: &TransactionalTree,
    voters: &TransactionalTree,
    votes: &TransactionalTree,
  ) -> ConflictableTransactionResult<(), ()> {
    let (root_id, path) = if self.id.matches(":").count() > 1 {
      let path: Vec<String> = self.id.split('/')
        .filter(|&c| c != "")
        .map(|c| c.to_string())
        .collect();
      (self.id.clone(), path)
    } else if let Some(raw_key) = kpi.get(self.id.as_bytes())? {
      let full_id = raw_key.to_string();
      let mut path: Vec<String> = full_id.split('/')
        .filter(|&c| c != "")
        .map(|c| c.to_string())
        .collect();
      let root_id = path.drain(..2).join("/");
      (root_id, path)
    } else {
      return Err(sled::transaction::ConflictableTransactionError::Abort(()));
    };

    comments.remove(self.id.as_bytes())?;
    raw_contents.remove(self.id.as_bytes())?;
    votes.remove(self.id.as_bytes())?;

    if let Some(raw) = ctrees.get(root_id.as_bytes())? {
      let mut parent_id_tree: CommentIDTree = raw.to_type();
      if let Some(child_cidtree) = parent_id_tree.remove_child(path) {
        for cidtree in child_cidtree.children.values() {
          if let Some(raw_child_comment) = comments.get(cidtree.comment.as_bytes())? {
            let child_comment: Comment = raw_child_comment.to_type();
            child_comment.remove_in_transaction(orc, kpi, ctrees, comments, raw_contents, voters, votes)?;
          } else {
            return Err(sled::transaction::ConflictableTransactionError::Abort(()));
          }
        }
        if let Some(raw_child_comment) = comments.get(child_cidtree.comment.as_bytes())? {
          let child_comment: Comment = raw_child_comment.to_type();
          child_comment.remove_in_transaction(orc, kpi, ctrees, comments, raw_contents, voters, votes)?;
        } else {
          return Err(sled::transaction::ConflictableTransactionError::Abort(()));
        }
        ctrees.insert(root_id.as_bytes(), binbe_serialize(&parent_id_tree))?;
      } else {
        return Err(sled::transaction::ConflictableTransactionError::Abort(()));
      }
      
      let mut iter = orc.comment_voters.scan_prefix(self.id.as_bytes());
      while let Some(pair) = iter.next() {
        voters.remove(pair?.0)?;
      }
  
      kpi.remove(self.id.as_bytes())?;

      return Ok(());
    }
    Err(sled::transaction::ConflictableTransactionError::Abort(()))
  }

  pub fn remove(&self, orc: &Orchestrator) -> bool {
    (
      &orc.comment_key_path_index,
      &orc.comment_trees,
      &orc.comments,
      &orc.comment_raw_content,
      &orc.comment_voters,
      &orc.comment_votes
    ).transaction(|(kpi, ctrees, comments, raw_contents, voters, votes)| {
      self.remove_in_transaction(orc, kpi, ctrees, comments, raw_contents, voters, votes)?;
      Ok(())
    }).is_ok()
  }
}

pub fn comment_on_writ(
  orc: &Orchestrator,
  writ: &Writ,
  usr: &User,
  raw_content: String,
  author_only: bool,
) -> Option<Comment> {
    if let Some(settings) = writ.comment_settings(orc) {
      if let Some(max_len) = settings.max_comment_length {
        if raw_content.len() > max_len as usize {
          return None;
        }
      }
      if let Some(min_len) = settings.min_comment_length {
        if raw_content.len() < min_len as usize {
          return None;
        }
      }

      let content = render_md(&raw_content);

      let (id, _own_id) = match Comment::new_first_level_id(orc, &writ.id, &usr.id) {
        Some(i) => i,
        None => return None,
      };

      let mut comment = Comment::new(id, usr.username.clone(), content);
      comment.author_only = author_only;

      let res: TransactionResult<(), ()> = (
        &orc.comment_trees,
        &orc.comments,
        &orc.comment_raw_content,
        &orc.comment_votes
      ).transaction(|(comment_trees, comments, comment_raw_content, votes)| {
          let cidtree = CommentIDTree{
            comment:  comment.id.clone(),
            children: HashMap::new(),
            level: 0,
          };
          comment_trees.insert(comment.id.as_bytes(), cidtree.to_bin())?;
          comments.insert(comment.id.as_bytes(), comment.to_bin())?;
          comment_raw_content.insert(comment.id.as_bytes(), raw_content.as_bytes())?;
          votes.insert(comment.id.as_bytes(), IVec::from_i64(0))?;
          Ok(())
      });

      if res.is_ok() {
        return Some(comment);
      }
    }
    None
}

pub fn comment_on_comment(
  orc: &Orchestrator,
  settings: &CommentSettings,
  parent_comment: &Comment,
  usr: &User,
  writ_id: String,
  raw_content: String,
  author_only: bool,
) -> Option<Comment> {
  if let Some(max_len) = settings.max_comment_length {
    if raw_content.len() > max_len {
      return None;
    }
  }
  if let Some(min_len) = settings.min_comment_length {
    if raw_content.len() < min_len {
      return None;
    }
  }

  let parent_id = if parent_comment.id.contains('/') {
    parent_comment.id.clone()
  } else {
    match parent_comment.key_path(orc) {
      Some(id) => id,
      None => return None,
    }
  };

  let (tree_id, parts) = get_prefix_and_parts(&parent_id, 2);
  if let Some(max_level) = settings.max_level {
    if parts.len() + 1 == max_level {
      return None;
    }
  }

  let content = render_md(&raw_content);

  let (id, own_id) = match Comment::new_subcomment_id(
    orc,
    &writ_id,
    &parent_id,
    &usr.id
  ) {
    Some(i) => i,
    None => return None,
  };

  let mut comment = Comment::new(own_id, usr.username.clone(), content);
  comment.author_only = author_only;

  if (
    &orc.comment_key_path_index, 
    &orc.comment_trees, 
    &orc.comments, 
    &orc.comment_raw_content, 
    &orc.comment_votes
  ).transaction(|(
    kpi,
    comment_trees,
    comments,
    comment_raw_content,
    votes
  )| {
    if let Some(raw) = comment_trees.get(tree_id.as_bytes())? {
      let mut parent_id_tree: CommentIDTree = raw.to_type();
      let id_tree = CommentIDTree{
        comment: comment.id.clone(),
        children: HashMap::new(),
        level: parent_id_tree.level + 1,
      };
      if parts.len() == 0 {
        parent_id_tree.insert(id_tree);
      } else if !parent_id_tree.insert_child(parts.clone(), id_tree) {
        return Err(sled::transaction::ConflictableTransactionError::Abort(()));
      }
      comment_trees.insert(tree_id.as_bytes(), binbe_serialize(&parent_id_tree))?;
    } else {
      return Err(sled::transaction::ConflictableTransactionError::Abort(()));
    }
    kpi.insert(comment.id.as_bytes(), id.as_bytes())?;
    comments.insert(comment.id.as_bytes(), comment.to_bin())?;
    comment_raw_content.insert(comment.id.as_bytes(), raw_content.as_bytes())?;
    votes.insert(comment.id.as_bytes(), IVec::from_i64(0))?;
    Ok(())
  }).is_ok() {
    return Some(comment);
  }
  None
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct PublicCommentTree {
  comment: PublicComment,
  children: Vec<PublicCommentTree>
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct CommentTree {
  comment: Comment,
  children: Vec<CommentTree>
}

impl CommentTree {
  pub fn public(
    self,
    orc: &Orchestrator,
    usr_id: &Option<String>,
  ) -> Option<PublicCommentTree> {
    Some(PublicCommentTree{
      comment: match self.comment.public(orc, usr_id) {
        Some(pc) => pc,
        None => return None,
      },
      children: self.children.into_par_iter()
        .filter_map(|c| c.public(orc, usr_id))
        .collect()
    })
  }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct CommentIDTree {
  comment: String,
  level: usize,
  children: HashMap<String, CommentIDTree>,
}

impl CommentIDTree {
  pub fn insert(&mut self, id_tree: CommentIDTree) {
    self.children.insert(id_tree.comment.clone(), id_tree);
  }

  pub fn subtree(&self, parts: Vec<String>) -> Option<CommentIDTree> {
    let p_len = parts.len() - 1;
    let mut i = 0;
    let next_layer: Cell<Option<&HashMap<String, CommentIDTree>>> = Cell::new(Some(&self.children));
    while let Some(children) = next_layer.take() {
      if let Some(child) = children.get(&parts[i]) {
        if i == p_len {
          return Some(child.clone());
        } else {
          i += 1;
          next_layer.replace(Some(&child.children));
        }
      } else {
        break;
      }
    }

    None
  }

  pub fn insert_child(&mut self, mut path: Vec<String>, id_tree: CommentIDTree) -> bool {
    if let Some(last) = path.last() {
      if *last == id_tree.comment {
        path.pop();
      }
      if let Some(first) = path.first() {
        if *first == self.comment {
          path.remove(0);
        }
      } else {
        return false;
      }
    } else {
      return false;
    }
    let p_len = path.len() - 1;
    let mut i = 0;
    let next_layer: Cell<Option<&mut HashMap<String, CommentIDTree>>> = Cell::new(Some(&mut self.children));
    while let Some(children) = next_layer.take() {
      if let Some(child) = children.get_mut(&path[i]) {
        if i == p_len {
          child.insert(id_tree);
          return true;
        }
        i += 1;
        next_layer.replace(Some(&mut child.children));
      } else {
        break;
      }
    }
    false
  }

  pub fn remove_child(&mut self, mut path: Vec<String>) -> Option<CommentIDTree> {
    if let Some(first) = path.first() {
      if *first == self.comment {
        path.remove(0);
      }
    } else {
      return None;
    }
    let p_len = path.len() - 1;
    let mut i = 0;
    let next_layer: Cell<Option<&mut HashMap<String, CommentIDTree>>> = Cell::new(Some(&mut self.children));
    while let Some(children) = next_layer.take() {
      if i == p_len && children.contains_key(&path[i]) {
        return children.remove(&path[i]);
      } else if let Some(child) = children.get_mut(&path[i]) {
          i += 1;
          next_layer.replace(Some(&mut child.children));
      } else {
        break;
      }
    }
    None
  }

  fn to_comment_tree(&self, orc: &Orchestrator, query: &CommentQuery) -> Option<CommentTree> {
    if let Some(max_level) = &query.max_level {
      if self.level == *max_level {
        return None;
      }
    }
  
    if let Some(ids) = &query.ids {
      if !ids.contains(&self.comment) {
        return None;
      }
    }
    if let Some(skip_ids) = &query.skip_ids {
      if skip_ids.contains(&self.comment) {
        return None;
      }
    }

    let author_id = Comment::get_author_id_from_id(&self.comment).to_string();

    if let Some(excluded_author_ids) = &query.exluded_author_ids {
      if excluded_author_ids.contains(&author_id) {
        return None;
      }
    }

    if let Some(au_id) = &query.author_id {
      if author_id != *au_id {
        return None;
      }
    } else if let Some(author_ids) = &query.author_ids {
      if !author_ids.contains(&author_id) {
        return None;
      }
    }

    if let Ok(Some(val)) = orc.comments.get(self.comment.as_bytes()) {
      let comment: Comment = val.to_type();
      if check_query_conditions(query, &comment, &author_id) {
        return Some(CommentTree{
          comment,
          children: self.children.par_iter()
            .filter_map(|(_, child)| child.to_comment_tree(orc, query))
            .collect()
        });
      }
    }

    None
  }

  pub fn to_comment_tree_sans_query(&self, orc: &Orchestrator) -> Option<CommentTree> {
    if let Ok(res) = orc.comments.get(self.comment.as_bytes()) {
      if let Some(val) = res {
        let comment: Comment = val.to_type();
        return Some(CommentTree{
          comment,
          children: self.children.par_iter()
            .filter_map(|(_, child)| child.to_comment_tree_sans_query(orc))
            .collect()
        });
      }
    }

    None
  }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct CommentQuery {
  pub ids: Option<Vec<String>>,
  pub skip_ids: Option<Vec<String>>,
  
  pub authors: Option<Vec<String>>,
  pub author_ids: Option<Vec<String>>,
  
  pub public: Option<bool>,
  pub is_admin: Option<bool>,
  pub requestor_id: Option<String>,

  pub author_name: Option<String>,
  pub author_handle: Option<String>,
  pub author_id: Option<String>,
  pub exluded_author_ids: Option<Vec<String>>,
  
  pub posted_before: Option<DateTime<Utc>>,
  pub posted_after: Option<DateTime<Utc>>,
  
  pub year: Option<i32>,
  pub month: Option<u32>,
  pub day: Option<u32>,
  pub hour: Option<u32>,
  
  pub max_level: Option<usize>,

  pub path: String,

  pub amount: Option<u64>,
  pub page: u64,
}

pub fn check_query_conditions(query: &CommentQuery, comment: &Comment, author_id: &str) -> bool {
  let is_admin = query.is_admin.unwrap_or(false);

  if let Some(public_status) = query.public {
    if comment.public == public_status {
      if !comment.public {
        if !is_admin {
          if let Some(requestor_id) = &query.requestor_id {
            if author_id != *requestor_id {
              return false;
            }
          }
        }
      }
    } else {
      return false;
    }
  }

  if comment.author_only {
    if let Some(requestor_id) = &query.requestor_id {
      if author_id != *requestor_id {
        return false;
      }
    } else {
      return false;
    }
  }

  if let Some(posted_before) = &query.posted_before {
    if comment.posted > *posted_before {
      return false;
    }
  }

  if let Some(posted_after) = &query.posted_after {
    if comment.posted < *posted_after {
      return false;
    }
  }

  if let Some(year) = &query.year {
    if comment.posted.year() != *year {
      return false;
    }
  }
  if let Some(month) = &query.month {
    if comment.posted.month() != *month {
      return false;
    }
  }
  if let Some(day) = &query.day {
    if comment.posted.day() != *day {
      return false;
    }
  }
  if let Some(hour) = &query.hour {
    if comment.posted.hour() != *hour {
      return false;
    }
  }

  true
}

pub async fn comment_query(
  o_usr: Option<User>,
  mut query: CommentQuery,
  orc: &Orchestrator,
) -> Option<Vec<CommentTree>> {
  query.path = query.path.trim_end_matches("/").to_string();
  if query.path.is_empty() {
    return None;
  }
  let is_admin = if let Some(usr) = &o_usr {
    query.requestor_id = Some(usr.id.clone());
    orc.is_admin(&usr.id)
  } else { false };

  if !query.path.contains('/') {
    if query.path.matches(':').count() == 1 {
      if let Ok(Some(raw)) = orc.comment_key_path_index.get(query.path.as_bytes()) {
        query.path = raw.to_string();
      } else {
        return None;
      }
    }
  }

  let mut path = query.path.clone();
  let mut parts: Vec<&str> = query.path.split('/').filter(|&c| c != "").collect();
  let depth_path: Option<Vec<String>> = if parts.len() > 2 {
    path = parts.drain(..2).join("/");
    let depth_path: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
    if parts.len() == 0 {
      return None;
    }
    Some(depth_path)
  } else {
    None
  };

  let writ_id = if parts.len() > 0 {
    if let Some(id) = parts.first() {
      id.to_string()
    } else {
      return None;
    }
  } else {
    query.path.clone()
  };

  if let Ok(Some(val)) = orc.comment_settings.get(writ_id.as_bytes()) {
    let settings: CommentSettings = val.to_type();
    if !settings.public {
      if let Some(requestor_id) = &query.requestor_id {
        if writ_id.split(":").collect::<Vec<&str>>()[1] != *requestor_id {
          return None;
        }
      } else if !is_admin {
        return None;
      }
    }
    if let Some(usr) = &o_usr {
      if let Some(visible_to) = &settings.visible_to {
        if !visible_to.contains(&usr.id) {
          return None;
        }
      }
    }
  }

  let amount = if let Some(a) = &query.amount { a.clone() } else { 50 };

  query.is_admin = Some(is_admin);

  if let Some(authors) = &query.authors {
    let mut ids: Vec<String> = authors.iter().filter_map(|a| {
      if let Ok(Some(id)) = orc.usernames.get(a.as_bytes()) {
        return Some(id.to_string());
      }
      None
    }).collect();

    if let Some(author_ids) = &mut query.author_ids {
      author_ids.append(&mut ids);
    } else {
      query.author_ids = Some(ids);
    }
  } else if query.author_id.is_none() {
    if let Some(name) = &query.author_name {
      if let Some(id) = orc.usernames.get(name.as_bytes()).unwrap() {
        query.author_id = Some(id.to_string());
      } else {
        return None;
      }
    } else if let Some(handle) = &query.author_handle {
      if let Some(id) = orc.handles.get(handle.as_bytes()).unwrap() {
        query.author_id = Some(id.to_string());
      } else {
        return None;
      }
    }
  }
  query.author_name = None;
  query.author_handle = None;

  if query.max_level.is_none() {
    query.max_level = Some(6);
  }

  if !is_admin {
    if amount > 50 { return None; }
  } else if amount > 500 { return None; }

  let mut count = 0;
  let page = query.page;

  let mut cidtrees = vec![];

  let mut iter = orc.comment_trees.scan_prefix(path.as_bytes());
  while let Some(Ok((_, value))) = iter.next() {
    if page < 2 {
        if count == amount { break; }
    } else if count != (amount * page) {
        count += 1;
        continue;
    }
    let id_tree: CommentIDTree = value.to_type();

    if let Some(dp) = depth_path {
      if let Some(st) = id_tree.subtree(dp) {
        cidtrees.push(st);        
      }
      break;
    }

    cidtrees.push(id_tree);
    count += 1;
  }

  let comments: Vec<CommentTree> = cidtrees.into_par_iter()
    .filter_map(|cidt| cidt.to_comment_tree(orc, &query))
    .collect();

  (comments.len() != 0).qualify(comments)
}

#[post("/comments")]
pub async fn post_comment_query(
  req: HttpRequest,
  query: web::Json<CommentQuery>,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  let o_usr: Option<User> = orc.user_by_session(&req);
  let usr_id: Option<String> = match &o_usr {
    Some(usr) => Some(usr.id.clone()),
    None => None,
  };

  match comment_query(o_usr, query.into_inner(), orc.as_ref()).await {
    Some(comments) => {
      let public_comments: Vec<PublicCommentTree> = comments
        .into_par_iter()
        .filter_map(|c| c.public(orc.as_ref(), &usr_id))
        .collect();

      crate::responses::Ok(public_comments)
    },
    None => crate::responses::NotFoundEmpty()
  }  
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct RawComment {
  pub parent_id: String,
  pub writ_id: String,
  pub raw_content: String,
  pub author_only: Option<bool>,
}

#[put("/comment")]
pub async fn make_comment(
  req: HttpRequest,
  rc: web::Json<RawComment>,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  let usr = match orc.user_by_session(&req) {
    Some(usr) => usr,
    None => return crate::responses::BadRequest(
      "only authorized users may post comments"
    ),
  };

  let mut rc = rc.into_inner();
  rc.raw_content = rc.raw_content.trim().to_string();

  if !orc.dev_mode {
    // hash contents and ratelimit with it to prevent spam
    let hitter = orc.hash(rc.raw_content.as_bytes());
    let rl = orc.ratelimiter.hit(&hitter, 1, Duration::minutes(60));
    if rl.is_timing_out() {
      return crate::responses::TooManyRequests(
        format!("don't copy existing comments, write your own")
      );
    }

    let hitter = format!("cmnt{}", usr.id);
    let rl = orc.ratelimiter.hit(hitter.as_bytes(), 3, Duration::minutes(2));
    if rl.is_timing_out() {
      return crate::responses::TooManyRequests(
        format!("too many requests, timeout has {} minutes left.", rl.minutes_left())
      );
    }
  }

  if rc.parent_id.matches(':').count() == 1 || rc.parent_id.contains('/') {
    return make_comment_on_comment(usr, rc, orc.as_ref()).await;
  }
  make_comment_on_writ(usr, rc, orc.as_ref()).await
}

#[delete("/comment")]
pub async fn delete_comment(
  req: HttpRequest,
  ctd: web::Json<String>,
  orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
  let usr = match orc.user_by_session(&req) {
    Some(usr) => usr,
    None => return crate::responses::Forbidden(
      "You can't delete your comments if you're not logged in"
    ),
  };

  if let Some(comment) = Comment::from_id(orc.as_ref(), ctd.as_bytes()) {
    if usr.username == comment.author_name {
      if comment.delete(orc.as_ref()) {
        return crate::responses::BadRequest(
          "comment successfully deleted"
        );
      }
    } else {
      return crate::responses::Forbidden(
        "You can't delete someone else's comment"
      );
    }
  } else {
    return crate::responses::BadRequest(
      "can't delete non-existent comment"
    );
  }

  crate::responses::InternalServerError(
    "troubles abound, failed to delete comment :("
  )
}

pub async fn make_comment_on_writ(usr: User, rc: RawComment, orc: &Orchestrator) -> HttpResponse {
  if let Some(writ) = orc.writ_by_id(&rc.writ_id) {
    if let Some(comment) = comment_on_writ(
      orc,
      &writ,
      &usr,
      rc.raw_content,
      rc.author_only.unwrap_or(false)
    ) {
      return crate::responses::AcceptedData(comment);
    }
    return crate::responses::BadRequest(
      "Can't comment on non-existing post"
    );
  }

  crate::responses::InternalServerError(
    "troubles abound, couldn't make subcomment :("
  )
}

pub async fn make_comment_on_comment(
  usr: User,
  rc: RawComment,
  orc: &Orchestrator,
) -> HttpResponse {
  if let Ok(Some(val)) = orc.comment_settings.get(rc.writ_id.as_bytes()) {
    let settings: CommentSettings = val.to_type();
    if let Some(parent_comment) = Comment::from_id(orc, rc.parent_id.as_bytes()) {
      if let Some(comment) = comment_on_comment(
        orc, 
        &settings, 
        &parent_comment,
        &usr,
        rc.writ_id,
        rc.raw_content,
        rc.author_only.unwrap_or(false)
      ) {
        return crate::responses::AcceptedData(comment);
      }
    }
  }

  crate::responses::InternalServerError(
    "troubles abound, couldn't make subcomment :("
  )
}

fn get_prefix_and_parts(id: &str, prefix_parts: usize) -> (String, Vec<String>) {
  let mut parts: Vec<String> = id.split_terminator('/')
    .filter(|s| *s != "")
    .map(|p| p.to_string())
    .collect();

  let prefix = parts.drain(..prefix_parts).join("/");

  (prefix, parts)
}
