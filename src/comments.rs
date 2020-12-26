use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use borsh::{BorshDeserialize, BorshSerialize};
use itertools::Itertools;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sled::{transaction::*, IVec, Transactional};
use std::{cell::Cell, collections::HashMap};
use time::Duration;

use crate::auth::User;
use crate::orchestrator::ORC;
use crate::utils::{
  datetime_from_unix_timestamp, i64_is_zero, render_md, unix_timestamp, FancyBool, FancyIVec,
};
use crate::writs::{CommentSettings, Vote, Writ};

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct PublicComment {
  pub id: String,
  pub content: String,
  pub author_name: String,
  pub posted: i64,
  #[serde(skip_serializing_if = "i64_is_zero")]
  pub vote: i64,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub edited: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub you_voted: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub author_only: Option<bool>,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Comment {
  pub id: String, // {meta_id}/{author_id}:{unique}/...
  pub author_name: String,
  pub content: String,
  pub posted: i64,
  pub edited: Option<i64>,
  pub public: bool,
  pub author_only: bool,
}

impl Comment {
  pub fn new(id: String, author_name: String, content: String) -> Self {
    Self {
      id,
      author_name,
      content,
      posted: unix_timestamp(),
      edited: None,
      public: false,
      author_only: false,
    }
  }

  pub fn from_id(id: &[u8]) -> Option<Comment> {
    match ORC.comments.get(id) {
      Ok(c) => c.map(|raw| Comment::try_from_slice(&raw).unwrap()),
      Err(_) => None,
    }
  }

  pub fn key_path(&self) -> Option<String> {
    if let Ok(Some(raw)) = ORC.comment_key_path_index.get(self.id.as_bytes()) {
      return Some(raw.to_string());
    }
    None
  }

  pub fn new_first_level_id(writ_id: &str, usr_id: &str) -> Option<(String, String)> {
    if let Ok(uid) = ORC.generate_id(writ_id.as_bytes()) {
      let own_id = format!("{}:{}", usr_id, uid);
      return Some((format!("{}/{}", writ_id, own_id), own_id));
    }
    None
  }
  pub fn new_subcomment_id(writ_id: &str, parent_id: &str, usr_id: &str) -> Option<(String, String)> {
    if let Ok(uid) = ORC.generate_id(writ_id.as_bytes()) {
      let own_id = format!("{}:{}", usr_id, uid);
      return Some((format!("{}/{}", parent_id, own_id), own_id));
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

  pub fn get_author_id_from_uncertain_id(id: &str) -> Option<&str> {
    if id.contains('/') {
      if let Some(cid) = id.split('/').last() {
        return cid.split(':').next();
      }
    }
    id.split(':').next()
  }

  pub fn vote_id(&self, usr_id: &str) -> String {
    format!("{}<{}", self.id, usr_id)
  }

  pub fn public(self, usr_id: &Option<String>) -> Option<PublicComment> {
    Some(PublicComment {
      posted: self.posted,
      edited: self.edited,
      author_only: self.author_only.wrap(),
      you_voted: match usr_id {
        Some(id) => match ORC.comment_voters.get(self.vote_id(id).as_bytes()) {
          Ok(cv) => cv.map(|raw| Vote::try_from_slice(&raw).unwrap().up),
          Err(_) => None,
        },
        None => None,
      },
      vote: if let Ok(Some(raw)) = ORC.comment_votes.get(self.id.as_bytes()) {
        raw.to_i64()
      } else {
        return None;
      },
      id: self.id,
      author_name: self.author_name,
      content: self.content,
    })
  }

  pub fn default_deleted(&self) -> Self {
    Self {
      id: self.id.clone(),
      author_name: "_".to_string(),
      content: "[deleted]".to_string(),
      posted: self.posted,
      public: self.public,
      edited: None,
      author_only: self.author_only,
    }
  }

  pub fn delete(&self) -> bool {
    let deleted_comment = self.default_deleted();

    let vlist = ORC.comment_voters.scan_prefix(self.id.as_bytes())
      .keys()
      .filter_map(|res| match res {
        Ok(key) => Some(key),
        Err(_) => None,
      })
      .collect::<Vec<IVec>>();

    let res: TransactionResult<(), ()> = (
      &ORC.comment_voters,
      &ORC.comments,
      &ORC.comment_raw_content,
      &ORC.comment_votes,
    )
      .transaction(|(voters, comments, comment_raw_content, votes)| {
        comments.insert(self.id.as_bytes(), deleted_comment.try_to_vec().unwrap())?;
        comment_raw_content.remove(self.id.as_bytes())?;
        votes.remove(self.id.as_bytes())?;

        for voter in vlist.iter() {
          voters.remove(voter)?;
        }
        Ok(())
      });

    res.is_ok()
  }

  pub fn is_root_comment(&self) -> bool {
    self.id.matches(":").count() > 1
  }

  pub fn get_root_comment_id(&self) -> Option<String> {
    if self.id.matches(":").count() > 1 {
      return Some(self.id.clone());
    }
    if let Ok(Some(raw_key)) = ORC.comment_key_path_index.get(self.id.as_bytes()) {
      let full_id = raw_key.to_string();
      let mut parts: Vec<&str> = full_id.split('/').filter(|&c| c != "").collect();
      let root_id = parts.drain(..2).join("/");
      return Some(root_id.to_string());
    }
    None
  }

  pub fn get_root_comment_id_and_path(&self) -> Option<(String, Vec<String>)> {
    if self.id.matches(":").count() > 1 {
      return Some((
        self.id.clone(),
        self.id.split('/')
          .filter(|&c| c != "")
          .map(|c| c.to_string())
          .collect()
      ));
    }

    if let Ok(Some(raw_key)) = ORC.comment_key_path_index.get(self.id.as_bytes()) {
      let full_id = raw_key.to_string();
      let mut parts: Vec<&str> = full_id.split('/').filter(|&c| c != "").collect();
      let path = parts.iter().map(|p| p.to_string()).collect::<Vec<String>>();
      let root_id = parts.drain(..2).join("/");
      return Some((root_id.to_string(), path));
    }
    None
  }

  pub fn get_root_comment(&self) -> Option<Comment> {
    if self.id.matches(":").count() > 1 {
      return Some(self.clone());
    }
    if let Ok(Some(raw_key)) = ORC.comment_key_path_index.get(self.id.as_bytes()) {
      let full_id = raw_key.to_string();
      let mut parts: Vec<&str> = full_id.split('/').filter(|&c| c != "").collect();
      let root_id = parts.drain(..2).join("/");
      if let Ok(c) = ORC.comments.get(root_id.as_bytes()) {
        return c.map(|raw| Comment::try_from_slice(&raw).unwrap());
      }
    }
    None
  }

  pub fn remove_in_transaction(
    &self,
    kpi: &TransactionalTree,
    ctrees: &TransactionalTree,
    comments: &TransactionalTree,
    raw_contents: &TransactionalTree,
    voters: &TransactionalTree,
    votes: &TransactionalTree,
  ) -> ConflictableTransactionResult<(), ()> {
    let is_root_comment = self.is_root_comment();
    let (root_id, path) = if is_root_comment {
      let path: Vec<String> = self.id
        .split('/')
        .filter(|&c| c != "")
        .map(|c| c.to_string())
        .collect();

      (self.id.clone(), path)
    } else if let Some(raw_key) = kpi.get(self.id.as_bytes())? {
      let full_id = raw_key.to_string();
      let mut path: Vec<String> = full_id
        .split('/')
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
      let mut parent_id_tree: CommentIDTree = bincode::deserialize(&raw).unwrap();
      if let Some(child_cidtree) = parent_id_tree.remove_child(path) {
        for cidtree in child_cidtree.children.values() {
          if let Some(raw_child_comment) = comments.get(cidtree.comment.as_bytes())? {
            let child_comment: Comment = Comment::try_from_slice(&raw_child_comment).unwrap();
            child_comment.remove_in_transaction(
              
              kpi,
              ctrees,
              comments,
              raw_contents,
              voters,
              votes,
            )?;
          } else {
            return Err(sled::transaction::ConflictableTransactionError::Abort(()));
          }
        }
        if let Some(raw_child_comment) = comments.get(child_cidtree.comment.as_bytes())? {
          let child_comment = Comment::try_from_slice(&raw_child_comment).unwrap();
          child_comment.remove_in_transaction(
            kpi,
            ctrees,
            comments,
            raw_contents,
            voters,
            votes,
          )?;
        } else {
          return Err(sled::transaction::ConflictableTransactionError::Abort(()));
        }
        if is_root_comment {
          ctrees.remove(root_id.as_bytes())?;
        } else {
          ctrees.insert(
            root_id.as_bytes(),
            bincode::serialize(&parent_id_tree).unwrap(),
          )?;
        }
      } else {
        return Err(sled::transaction::ConflictableTransactionError::Abort(()));
      }
      let mut iter = ORC.comment_voters.scan_prefix(self.id.as_bytes());
      while let Some(pair) = iter.next() {
        voters.remove(pair?.0)?;
      }
      kpi.remove(self.id.as_bytes())?;

      return Ok(());
    }

    Err(sled::transaction::ConflictableTransactionError::Abort(()))
  }

  pub fn remove(&self) -> bool {
    (
      &ORC.comment_key_path_index,
      &ORC.comment_trees,
      &ORC.comments,
      &ORC.comment_raw_content,
      &ORC.comment_voters,
      &ORC.comment_votes,
    )
      .transaction(|(kpi, ctrees, comments, raw_contents, voters, votes)| {
        self.remove_in_transaction(kpi, ctrees, comments, raw_contents, voters, votes)?;
        Ok(())
      })
      .is_ok()
  }

  pub fn vote(&self, usr_id: &str, up: Option<bool>) -> Option<i64> {
    let res: TransactionResult<i64, ()> =
      (&ORC.comment_votes, &ORC.comment_voters).transaction(|(votes, voters)| {
        let vote_id = self.vote_id(usr_id);
        let mut count: i64 = 0;
        if let Some(raw) = voters.get(vote_id.as_bytes())? {
          let old_vote = Vote::try_from_slice(&raw).unwrap();
          if let Some(up) = &up {
            // prevent double voting
            if old_vote.up == *up {
              return Err(sled::transaction::ConflictableTransactionError::Abort(()));
            }
            // handle when they alreay voted and now vote the oposite way
            count += votes.get(self.id.as_bytes())?.unwrap().to_i64();
            if *up {
              count += 2;
            } else {
              count -= 2;
            }
            votes.insert(self.id.as_bytes(), &count.to_be_bytes())?;
          } else {
            // unvote
            voters.remove(vote_id.as_bytes())?;

            count += votes.get(self.id.as_bytes())?.unwrap().to_i64();
            if old_vote.up {
              count -= 1;
            } else {
              count += 1;
            }

            votes.insert(self.id.as_bytes(), &count.to_be_bytes())?;

            return Ok(count);
          }
        } else if up.is_none() {
          return Err(sled::transaction::ConflictableTransactionError::Abort(()));
        } else {
          count += votes.get(self.id.as_bytes())?.unwrap().to_i64();
          if up.clone().unwrap() {
            count += 1;
          } else {
            count -= 1;
          }
          votes.insert(self.id.as_bytes(), &count.to_be_bytes())?;
        }

        let v = Vote {
          id: vote_id,
          when: unix_timestamp(),
          up: up.unwrap(),
        };
        voters.insert(v.id.as_bytes(), v.try_to_vec().unwrap())?;

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

  pub fn upvote(&self, usr_id: &str) -> Option<i64> {
    self.vote(usr_id, Some(true))
  }

  pub fn downvote(&self, usr_id: &str) -> Option<i64> {
    self.vote(usr_id, Some(false))
  }

  pub fn unvote(&self, usr_id: &str) -> Option<i64> {
    self.vote(usr_id, None)
  }
}

pub fn comment_on_writ(
  writ: &Writ,
  usr: &User,
  raw_content: String,
  author_only: bool,
) -> Option<Comment> {
  if let Some(settings) = writ.comment_settings() {
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

    let (id, _own_id) = match Comment::new_first_level_id(&writ.id, &usr.id) {
      Some(i) => i,
      None => return None,
    };

    let mut comment = Comment::new(id, usr.username.clone(), content);
    comment.author_only = author_only;

    let res: TransactionResult<(), ()> = (
      &ORC.comment_trees,
      &ORC.comments,
      &ORC.comment_raw_content,
      &ORC.comment_votes,
    )
      .transaction(|(comment_trees, comments, comment_raw_content, votes)| {
        let cidtree = CommentIDTree {
          comment: comment.id.clone(),
          children: HashMap::new(),
          level: 0,
        };
        comment_trees.insert(comment.id.as_bytes(), bincode::serialize(&cidtree).unwrap())?;
        comments.insert(comment.id.as_bytes(), comment.try_to_vec().unwrap())?;
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
  settings: &CommentSettings,
  parent_comment: &Comment,
  usr: &User,
  writ_id: String,
  raw_content: String,
  author_only: bool,
) -> Option<Comment> {
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

  let parent_id = if parent_comment.id.contains('/') {
    parent_comment.id.clone()
  } else if let Some(id) = parent_comment.key_path() {
    id
  } else {
    return None;
  };

  let (tree_id, parts) = get_prefix_and_parts(&parent_id, 2);
  if let Some(max_level) = settings.max_level {
    if parts.len() + 1 == max_level as usize {
      return None;
    }
  }

  let content = render_md(&raw_content);

  let (id, own_id) = match Comment::new_subcomment_id(&writ_id, &parent_id, &usr.id) {
    Some(i) => i,
    None => return None,
  };

  let mut comment = Comment::new(own_id, usr.username.clone(), content);
  comment.author_only = author_only;

  if (
    &ORC.comment_key_path_index,
    &ORC.comment_trees,
    &ORC.comments,
    &ORC.comment_raw_content,
    &ORC.comment_votes,
  )
    .transaction(|(kpi, comment_trees, comments, comment_raw_content, votes)| {
        if let Some(raw) = comment_trees.get(tree_id.as_bytes())? {
          let mut parent_id_tree: CommentIDTree = bincode::deserialize(&raw).unwrap();
          let id_tree = CommentIDTree {
            comment: comment.id.clone(),
            children: HashMap::new(),
            level: parent_id_tree.level + 1,
          };
          if parts.len() == 0 {
            parent_id_tree.insert(id_tree);
          } else if !parent_id_tree.insert_child(parts.clone(), id_tree) {
            return Err(sled::transaction::ConflictableTransactionError::Abort(()));
          }
          comment_trees.insert(
            tree_id.as_bytes(),
            bincode::serialize(&parent_id_tree).unwrap(),
          )?;
        } else {
          return Err(sled::transaction::ConflictableTransactionError::Abort(()));
        }
        kpi.insert(comment.id.as_bytes(), id.as_bytes())?;
        comments.insert(comment.id.as_bytes(), comment.try_to_vec().unwrap())?;
        comment_raw_content.insert(comment.id.as_bytes(), raw_content.as_bytes())?;
        votes.insert(comment.id.as_bytes(), IVec::from_i64(0))?;
        Ok(())
      },
    )
    .is_ok()
  {
    return Some(comment);
  }
  None
}

pub fn edit_comment(settings: &CommentSettings, rce: RawCommentEdit) -> Option<Comment> {
  if let Some(max_len) = settings.max_comment_length {
    if rce.raw_content.len() > max_len as usize {
      return None;
    }
  }
  if let Some(min_len) = settings.min_comment_length {
    if rce.raw_content.len() < min_len as usize {
      return None;
    }
  }

  if let Ok(comment) = (
    &ORC.comments,
    &ORC.comment_raw_content,
  )
    .transaction(|(comments, comment_raw_content)| {
      if let Ok(Some(raw_comment)) = comments.get(rce.id.as_bytes()) {
        let mut comment = Comment::try_from_slice(&raw_comment).unwrap();
        comment.author_only = rce.author_only.unwrap_or(false);
        comment.content = render_md(&rce.raw_content);
        comment.edited = Some(unix_timestamp());

        comments.insert(rce.id.as_bytes(), comment.try_to_vec().unwrap())?;
        comment_raw_content.insert(rce.id.as_bytes(), rce.raw_content.as_bytes())?;
        return Ok(comment);
      }
      Err(sled::transaction::ConflictableTransactionError::Abort(()))
    })
  {
    return Some(comment);
  }
  None
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct PublicCommentTree {
  comment: PublicComment,
  children: Vec<PublicCommentTree>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct CommentTree {
  comment: Comment,
  children: Vec<CommentTree>,
}

impl CommentTree {
  pub fn public(
    self,
    usr_id: &Option<String>,
    top_level: bool,
  ) -> Option<PublicCommentTree> {
    Some(PublicCommentTree {
      comment: match self.comment.public(usr_id) {
        Some(pc) => pc,
        None => return None,
      },
      children: if top_level {
        self
          .children
          .into_par_iter()
          .filter_map(|c| c.public(usr_id, false))
          .collect()
      } else {
        self
          .children
          .into_iter()
          .filter_map(|c| c.public(usr_id, false))
          .collect()
      },
    })
  }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CommentIDTree {
  comment: String,
  level: u64,
  children: HashMap<String, CommentIDTree>,
}

impl CommentIDTree {
  pub fn insert(&mut self, id_tree: CommentIDTree) {
    self.children.insert(id_tree.comment.clone(), id_tree);
  }

  pub fn subtree(&self, path: Vec<String>) -> Option<&CommentIDTree> {
    let p_len = path.len() - 1;
    let mut i = 0;
    let next_layer: Cell<Option<&HashMap<String, CommentIDTree>>> = Cell::new(Some(&self.children));
    while let Some(children) = next_layer.take() {
      if let Some(child) = children.get(&path[i]) {
        if i == p_len {
          return Some(&child);
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

  fn to_comment_tree(
    &self,
    query: &CommentQuery,
    is_top_level: bool,
  ) -> Option<CommentTree> {
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

    if let Ok(Some(val)) = ORC.comments.get(self.comment.as_bytes()) {
      let comment = Comment::try_from_slice(&val).unwrap();
      if check_query_conditions(query, &comment, &author_id) {
        return Some(CommentTree {
          comment,
          children: if is_top_level {
            self
              .children
              .par_iter()
              .filter_map(|(_, child)| child.to_comment_tree(query, false))
              .collect()
          } else {
            self
              .children
              .iter()
              .filter_map(|(_, child)| child.to_comment_tree(query, false))
              .collect()
          },
        });
      }
    }
    None
  }

  /*pub fn to_comment_tree_sans_query(&self) -> Option<CommentTree> {
    if let Ok(res) = ORC.comments.get(self.comment.as_bytes()) {
      if let Some(val) = res {
        let comment: Comment = Comment::try_from_slice(&val).unwrap();
        return Some(CommentTree {
          comment,
          children: self
            .children
            .iter()
            .filter_map(|(_, child)| child.to_comment_tree_sans_query())
            .collect(),
        });
      }
    }

    None
  } */
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
  pub posted_before: Option<i64>,
  pub posted_after: Option<i64>,
  pub year: Option<i32>,
  pub month: Option<u8>,
  pub day: Option<u8>,
  pub hour: Option<u8>,
  pub max_level: Option<u64>,

  pub path: String,

  pub amount: Option<u64>,
  pub page: u64,
}

pub fn check_query_conditions(query: &CommentQuery, comment: &Comment, author_id: &str) -> bool {
  let is_admin = query.is_admin.unwrap_or(false);

  if let Some(public_status) = query.public {
    if comment.public != public_status {
      return false;
    }
    if !comment.public && !is_admin {
      if let Some(requestor_id) = &query.requestor_id {
        if author_id != *requestor_id {
          return false;
        }
      }
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

  if query.day.is_some() || query.hour.is_some() || query.year.is_some() || query.month.is_some() {
    let posted = datetime_from_unix_timestamp(comment.posted);
    if let Some(year) = &query.year {
      if posted.year() != *year {
        return false;
      }
    }

    if let Some(month) = &query.month {
      if let Some(day) = &query.day {
        // they say this is more efficient, so, you know, meh..
        let (m, d) = posted.month_day();
        if m != *month || d != *day {
          return false;
        }
      } else if posted.month() != *month {
        return false;
      }
    } else if let Some(day) = &query.day {
      if posted.day() != *day {
        return false;
      }
    }

    if let Some(hour) = &query.hour {
      if posted.hour() != *hour {
        return false;
      }
    }
  }

  true
}

pub async fn comment_query(o_usr: Option<&User>, mut query: CommentQuery) -> Option<Vec<CommentTree>> {
  query.path = query.path.trim_end_matches("/").to_string();
  if query.path.is_empty() {
    return None;
  }
  let is_admin = if let Some(usr) = &o_usr {
    query.requestor_id = Some(usr.id.clone());
    ORC.is_admin(&usr.id)
  } else {
    false
  };

  if !query.path.contains('/') {
    if query.path.matches(':').count() == 1 {
      if let Ok(Some(raw)) = ORC.comment_key_path_index.get(query.path.as_bytes()) {
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

  if let Ok(Some(val)) = ORC.comment_settings.get(writ_id.as_bytes()) {
    let settings = CommentSettings::try_from_slice(&val).unwrap();
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

  let amount = query.amount.as_ref().map_or(50, |a| *a);

  query.is_admin = Some(is_admin);

  if let Some(authors) = &query.authors {
    let mut ids: Vec<String> = authors
      .par_iter()
      .filter_map(|a| 
        ORC.usernames.get(a.as_bytes())
          .map_or(None, |id| id.map(|id| id.to_string()))
      )
      .collect();

    if let Some(author_ids) = &mut query.author_ids {
      author_ids.append(&mut ids);
    } else {
      query.author_ids = Some(ids);
    }
  } else if query.author_id.is_none() {
    if let Some(name) = &query.author_name {
      if let Ok(Some(id)) = ORC.usernames.get(name.as_bytes()) {
        query.author_id = Some(id.to_string());
      } else {
        return None;
      }
    } else if let Some(handle) = &query.author_handle {
      if let Ok(Some(id)) = ORC.handles.get(handle.as_bytes()) {
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

  if (is_admin && amount > 500) || amount > 50 {
    return None;
  }

  let (mut tx, mut rx) = tokio::sync::mpsc::channel::<CommentIDTree>(amount as usize);
  let mut iter = ORC.comment_trees.scan_prefix(path.as_bytes());
  let page = query.page;

  tokio::spawn(async move {
    let mut count = 0;
    while let Some(Ok((_, raw))) = iter.next_back() {
      if page < 2 {
        if count == amount {
          break;
        }
      } else if count != (amount * page) {
        count += 1;
        continue;
      }

      let id_tree = bincode::deserialize::<CommentIDTree>(&raw).unwrap();

      if let Some(dp) = depth_path {
        if let Some(st) = id_tree.subtree(dp) {
          if tx.send(st.clone()).await.is_ok() {
            break;
          }
        }
        break;
      }

      if tx.send(id_tree).await.is_ok() {
        count += 1;
      }
    }
  });

  let mut comment_trees = Vec::with_capacity(10);
  while let Some(cit) = rx.recv().await {
    if let Some(ct) = cit.to_comment_tree(&query, true) {
      comment_trees.push(ct);
    }
  }
  Some(comment_trees)
}

#[post("/comments")]
pub async fn post_comment_query(
  req: HttpRequest,
  query: web::Json<CommentQuery>
) -> HttpResponse {
  let o_usr = ORC.user_by_session(&req);
  let usr_id = match &o_usr {
    Some(el) => Some(el.id.clone()),
    None => None,
  };

  match comment_query(
    o_usr.as_ref(),
    query.into_inner()
  ).await{
    Some(comments) => crate::responses::Ok(
      comments
        .into_par_iter()
        .filter_map(|c| c.public(&usr_id, true))
        .collect::<Vec<PublicCommentTree>>(),
    ),
    None => crate::responses::NotFoundEmpty(),
  }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct RawComment {
  pub parent_id: String,
  pub writ_id: String,
  pub raw_content: String,
  pub author_only: Option<bool>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct RawCommentEdit {
  pub id: String,
  pub writ_id: String,
  pub raw_content: String,
  pub author_only: Option<bool>,
}

#[put("/comment")]
pub async fn make_comment(
  req: HttpRequest,
  rc: web::Json<RawComment>,
) -> HttpResponse {
  let usr = match ORC.user_by_session(&req) {
    Some(usr) => usr,
    None => return crate::responses::BadRequest("only authorized users may post comments"),
  };

  let mut rc = rc.into_inner();
  rc.raw_content = rc.raw_content.trim().to_string();

  if !ORC.dev_mode {
    /* hash contents and ratelimit with it to prevent spam
    let hitter = ORC.hash(rc.raw_content.as_bytes());
    if let Some(rl) = ORC.ratelimiter.hit(&hitter, 1, Duration::minutes(60)) {
      if rl.is_timing_out() {
        return crate::responses::TooManyRequests(
          format!("don't copy existing comments, write your own")
        );
      }
    }*/

    let hitter = format!("cmnt{}", usr.id);
    if let Some(rl) = ORC.ratelimiter
      .hit(hitter.as_bytes(), 3, Duration::minutes(2))
    {
      if rl.is_timing_out() {
        return crate::responses::TooManyRequests(format!(
          "too many requests, timeout has {} minutes left.",
          rl.minutes_left()
        ));
      }
    }
  }

  if rc.parent_id.matches(':').count() == 1 || rc.parent_id.contains('/') {
    return make_comment_on_comment(&usr, rc).await;
  }
  make_comment_on_writ(&usr, rc).await
}

#[post("/edit-comment")]
pub async fn edit_comment_request(
  req: HttpRequest,
  rce: web::Json<RawCommentEdit>,
) -> HttpResponse {  
  let usr = match ORC.user_by_session(&req) {
    Some(usr) => usr,
    None => return crate::responses::Forbidden("Unauthorized comment edit attempt"),
  };

  let mut rce = rce.into_inner();
  if rce.id.contains('-') {
    rce.id = rce.id.replace('-', "/").to_string();
  }

  rce.raw_content = rce.raw_content.trim().to_string();

  if !ORC.dev_mode {
    let hitter = format!("ce{}", usr.id);
    if let Some(rl) = ORC.ratelimiter
      .hit(hitter.as_bytes(), 3, Duration::minutes(5))
    {
      if rl.is_timing_out() {
        return crate::responses::TooManyRequests(format!(
          "too many requests, timeout has {} minutes left.",
          rl.minutes_left()
        ));
      }
    }
  }

  make_comment_edit(&usr.id, rce).await
}

#[get("/comment/{id}/raw-content")]
pub async fn fetch_comment_raw_content(
  req: HttpRequest,
  cid: web::Path<String>,
) -> HttpResponse {
  let cid = cid.replace("-", "/");
  // TODO: ratelimiting
  if let Some(usr) = ORC.user_by_session(&req) {
    if let Some(author_id) = Comment::get_author_id_from_uncertain_id(cid.as_str()) {
      if author_id == usr.id {
        if let Ok(Some(raw_rc)) = ORC.comment_raw_content.get(cid.as_bytes()) {
          return crate::responses::Ok(raw_rc.to_string());
        } else {
          return crate::responses::NotFound("comment id didn't match anything of yours");
        }
      }
    }
  }

  crate::responses::Forbidden(
    "You can't load the raw_contents of comments if you aren't logged in or if the contents in question aren't yours"
  )
}

#[delete("/comment")]
pub async fn delete_comment(
  req: HttpRequest,
  ctd: web::Json<String>,
) -> HttpResponse {
  let usr = match ORC.user_by_session(&req) {
    Some(usr) => usr,
    None => {
      return crate::responses::Forbidden("You can't delete your comments if you're not logged in")
    }
  };

  
  if let Some(comment) = Comment::from_id(ctd.as_bytes()) {
    if usr.username == comment.author_name {
      if comment.delete() {
        return crate::responses::Ok("Comment successfully deleted");
      }
    } else {
      return crate::responses::Forbidden("You can't delete someone else's comment");
    }
  } else {
    return crate::responses::BadRequest("can't delete non-existent comment");
  }

  crate::responses::InternalServerError("troubles abound, failed to delete comment :(")
}

#[get("/comment/{id}/upvote")]
pub async fn upvote_comment(
  req: HttpRequest,
  id: web::Path<String>,
) -> HttpResponse {
  let id = id.replace("-", "/");
  if let Some(raw) = ORC.user_id_by_session(&req) {
    let usr_id = raw.to_string();
    if let Some(comment) = Comment::from_id(id.as_bytes()) {
      if let Some(count) = comment.upvote(&usr_id) {
        return crate::responses::AcceptedStatusData("vote went through", count);
      }
    }
  } else {
    return crate::responses::Forbidden("only users may vote on writs");
  }

  crate::responses::InternalServerError("failed to register vote")
}

#[get("/comment/{id}/downvote")]
pub async fn downvote_comment(
  req: HttpRequest,
  id: web::Path<String>,
) -> HttpResponse {
  let id = id.replace("-", "/");
  if let Some(raw) = ORC.user_id_by_session(&req) {
    let usr_id = raw.to_string();
    if let Some(comment) = Comment::from_id(id.as_bytes()) {
      if let Some(count) = comment.downvote(&usr_id) {
        return crate::responses::AcceptedStatusData("vote went through", count);
      }
    }
  } else {
    return crate::responses::Forbidden("only users may vote on writs");
  }

  crate::responses::InternalServerError("failed to register vote")
}

#[get("/comment/{id}/unvote")]
pub async fn unvote_comment(
  req: HttpRequest,
  id: web::Path<String>,
) -> HttpResponse {
  let id = id.replace("-", "/");
  if let Some(raw) = ORC.user_id_by_session(&req) {
    let usr_id = raw.to_string();
    if let Some(comment) = Comment::from_id(id.as_bytes()) {
      if let Some(count) = comment.unvote(&usr_id) {
        return crate::responses::AcceptedStatusData("vote went through", count);
      }
    }
  } else {
    return crate::responses::Forbidden("only users may vote on writs");
  }

  crate::responses::InternalServerError("failed to register vote")
}

pub async fn make_comment_on_writ(usr: &User, rc: RawComment) -> HttpResponse {
  if let Some(writ) = ORC.writ_by_id(&rc.writ_id) {
    if let Some(comment) = comment_on_writ(
      
      &writ,
      usr,
      rc.raw_content,
      rc.author_only.unwrap_or(false),
    ) {
      return crate::responses::AcceptedData(comment);
    }
    return crate::responses::BadRequest("Can't comment on non-existing post");
  }

  crate::responses::InternalServerError("troubles abound, couldn't make subcomment :(")
}

pub async fn make_comment_on_comment(usr: &User, rc: RawComment) -> HttpResponse {
  if let Ok(Some(val)) = ORC.comment_settings.get(rc.writ_id.as_bytes()) {
    let settings = CommentSettings::try_from_slice(&val).unwrap();
    if let Some(parent_comment) = Comment::from_id(rc.parent_id.as_bytes()) {
      if let Some(comment) = comment_on_comment(
        &settings,
        &parent_comment,
        &usr,
        rc.writ_id,
        rc.raw_content,
        parent_comment.author_only,
      ) {
        return crate::responses::AcceptedData(comment);
      }
    }
  }

  crate::responses::InternalServerError("troubles abound, couldn't make subcomment :(")
}

pub async fn make_comment_edit(usr_id: &str, rce: RawCommentEdit) -> HttpResponse {
  if let Some(author_id) = Comment::get_author_id_from_uncertain_id(&rce.id) {
    if author_id != usr_id {
      return crate::responses::Forbidden("You cannot edit another user's comments.");
    }
  } else {
    return crate::responses::BadRequest("Bad comment id");
  }

  if let Ok(Some(val)) = ORC.comment_settings.get(rce.writ_id.as_bytes()) {
    let settings = CommentSettings::try_from_slice(&val).unwrap();
    // TODO: proper error handling instead of do or fail generically
    if let Some(comment) = edit_comment(&settings, rce) {
      if let Some(pc) = comment.public(&Some(usr_id.to_string())) {
        return crate::responses::AcceptedData(pc);
      }
      return crate::responses::Accepted("Comment edited succesfully, but retrieving the updated version hit a snag, no worries just reload the page");
    }
  }
  crate::responses::InternalServerError("troubles abound, couldn't edit comment :(")
}

fn get_prefix_and_parts(id: &str, prefix_parts: usize) -> (String, Vec<String>) {
  let mut parts: Vec<String> = id
    .split_terminator('/')
    .filter(|s| *s != "")
    .map(|p| p.to_string())
    .collect();

  let prefix = parts.drain(..prefix_parts).join("/");

  (prefix, parts)
}
