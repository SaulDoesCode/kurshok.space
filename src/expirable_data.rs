use borsh::{BorshDeserialize, BorshSerialize};
use parking_lot::RwLock;
use rayon::prelude::*;
use sled::{transaction::*, IVec, Transactional};

use std::{
    thread,
    collections::{HashSet, BTreeMap}
};

use crate::orchestrator::{Orchestrator, ORC};
use crate::utils::{unix_timestamp, FancyIVec};

lazy_static! {
  static ref EXPIRY_TIMES: RwLock<HashSet<i64>> = RwLock::new(HashSet::new());
}

impl Orchestrator {
    pub fn expire_key(&self, from_now: i64, tree: String, key: &[u8]) -> bool {
        let exp = unix_timestamp() + from_now;
        let data = ExpirableData::Single{tree, key: key.to_vec()};

        self.expirable_data.insert(
            data.try_to_vec().unwrap(),
            IVec::from_i64(exp)
        ).map_or(false, |res| {
            if let Some(old_exp) = res {
                (EXPIRY_TIMES.write()).remove(&old_exp.to_i64());
            }
            (EXPIRY_TIMES.write()).insert(exp);
            true
        })
    }

    pub fn expire_keys(&self, from_now: i64, tree: String, keys: Vec<Vec<u8>>) -> bool {
        let exp = unix_timestamp() + from_now;
        let data = ExpirableData::MultiKey{tree, keys};

        self.expirable_data.insert(
            data.try_to_vec().unwrap(),
            IVec::from_i64(exp)
        ).map_or(false, |res| {
            if let Some(old_exp) = res {
                (EXPIRY_TIMES.write()).remove(&old_exp.to_i64());
            }
            (EXPIRY_TIMES.write()).insert(exp);
            true
        })
    }

    pub fn expire_data(&self, from_now: i64, tree: BTreeMap<String, Vec<Vec<u8>>>) -> bool {
        let exp = unix_timestamp() + from_now;
        let data = ExpirableData::MultiTree(tree);

        self.expirable_data.insert(
            data.try_to_vec().unwrap(),
            IVec::from_i64(exp)
        ).map_or(false, |res| {
            if let Some(old_exp) = res {
                (EXPIRY_TIMES.write()).remove(&old_exp.to_i64());
            }
            (EXPIRY_TIMES.write()).insert(exp);
            true
        })
    }

    pub fn unexpire_data(&self, data: ExpirableData) -> Option<i64> {
        if let Ok(o_exp) = self.expirable_data.remove(data.try_to_vec().unwrap()) {
            o_exp.map(|raw| {
                let exp = raw.to_i64();
                (*EXPIRY_TIMES.write()).remove(&exp);
                exp
            })
        } else {
            None
        }
    }

    pub fn unexpire_key(&self, tree: String, key: &[u8]) -> Option<i64> {
        self.unexpire_data(ExpirableData::Single{tree, key: key.to_vec()})
    }

    pub fn unexpire_keys(&self, tree: String, keys: Vec<Vec<u8>>) -> Option<i64> {
        self.unexpire_data(ExpirableData::MultiKey{tree, keys})
    }

    pub fn unexpire_map(&self, map: BTreeMap<String, Vec<Vec<u8>>>) -> Option<i64> {
        self.unexpire_data(ExpirableData::MultiTree(map))
    }
}

fn expirable_data_into_map(map: &mut BTreeMap<String, Vec<Vec<u8>>>, data: ExpirableData) {
    match data {
        ExpirableData::Single{tree, key} => {
            if let Some(keys) = map.get_mut(&tree) {
                keys.push(key);
            } else {
                map.insert(tree, vec![key]);
            }
        },
        ExpirableData::MultiKey{tree, keys} => {
            if let Some(new_keys) = map.get_mut(&tree) {
                for key in keys {
                    new_keys.push(key);
                }
            } else {
                map.insert(tree, keys);
            }
        },
        ExpirableData::MultiTree(old_map) => {
            map.extend(old_map);
        }
    }
}

fn expirable_data_to_map(data: ExpirableData) -> BTreeMap<String, Vec<Vec<u8>>> {
    let mut map = BTreeMap::new();
    match data {
        ExpirableData::Single{tree, key} => {
            map.insert(tree, vec![key]);
        },
        ExpirableData::MultiKey{tree, keys} => {
            map.insert(tree, keys);
        },
        ExpirableData::MultiTree(old_map) => {
            map.extend(old_map);
        }
    }

    map
}

fn merge_expirable_data(data: ExpirableData, old_data: ExpirableData) -> ExpirableData {
    let mut map = expirable_data_to_map(old_data);
    expirable_data_into_map(&mut map, data);

    ExpirableData::MultiTree(map)
}

#[derive(BorshSerialize, BorshDeserialize)]
pub enum ExpirableData {
    Single {tree: String, key: Vec<u8>},
    MultiKey {tree: String, keys: Vec<Vec<u8>>},
    MultiTree(BTreeMap<String, Vec<Vec<u8>>>),
}

pub fn start_system() -> thread::JoinHandle<()> {
    let mut iter = ORC.expirable_data.iter();
    while let Some(Ok((_, exp))) = iter.next() {
        let exp = exp.to_i64();
        (EXPIRY_TIMES.write()).insert(exp);
    }
    clean_up_all();

    thread::spawn(|| {
        loop {
            let now = unix_timestamp();
            if EXPIRY_TIMES.read().par_iter().any(|e| now >= *e) {
                clean_up_all();
            }
            thread::sleep(std::time::Duration::from_secs(1));
        }
    })
}

pub fn clean_up_all() {
    let mut for_removal: Vec<i64> = vec![];

    let mut iter = ORC.expirable_data.iter();
    while let Some(Ok((raw_data, raw_exp))) = iter.next() {
        let now = unix_timestamp();
        let exp = raw_exp.to_i64();
        if ORC.dev_mode {
            println!(
                "found something expirable now = {} >= exp = {} == {}",
                now, exp, now >= exp
            );
        }

        if now >= exp {
            clean_up_expirable_datum(raw_data, None);
            for_removal.push(exp);
        }
    }

    let mut et = EXPIRY_TIMES.write();
    for el in for_removal{
        (*et).remove(&el);
    }
}

fn clean_up_expirable_datum(raw_data: IVec, exp: Option<i64>) -> bool {
    let data = ExpirableData::try_from_slice(&raw_data).unwrap();
    let mut ok = false;
    match data {
        ExpirableData::Single{tree, key} => {
            if let Ok(tr) = ORC.db.open_tree(tree.as_bytes()) {
                if let Ok(Some(_)) = tr.remove(key.as_slice()) {
                    ok = true;
                }
            }
        },
        ExpirableData::MultiKey{tree, keys} => {
            if let Ok(tr) = ORC.db.open_tree(tree.as_bytes()) {
                let res: TransactionResult<(), ()> = tr.transaction(|tr| {
                    for key in keys.iter() {
                        tr.remove(key.as_slice())?;
                    }
                    Ok(())
                });
                if ORC.dev_mode {
                    let ok = if res.is_ok() {
                        "ok"
                    } else {
                        "not ok"
                    };
                    println!("removing keys from tree - {} went {}", tree, ok);
                }
                ok = res.is_ok();
            } else if ORC.dev_mode {
                println!("ExpirableData::MultiKey couldn't open tree - {}", &tree);
            }
        },
        ExpirableData::MultiTree(map) => {
            if ORC.dev_mode {
                println!("going in for the big multi-tree expiry - {:?}", &map);
            }

            let mut trees = vec!();
            for tree in map.keys() {
                if let Ok(tr) = ORC.db.open_tree(tree.as_bytes()) {
                    trees.push(tr);
                }
            }

            let res: TransactionResult<(), ()> = trees.as_slice().transaction(|trs| {
                let mut key_set = map.values();
                for tr in trs {
                    if let Some(keys) = key_set.next() {
                        for key in keys.iter() {
                            tr.remove(key.as_slice())?;
                        }
                    }
                }
                Ok(())
            });

            if ORC.dev_mode {
                let ok = if res.is_ok() {
                    "ok"
                } else {
                    "not ok"
                };
                println!("removing many keys from many trees went {}", ok);
            }
        },
    }

    if let Some(exp) = exp {
        (*EXPIRY_TIMES.write()).remove(&exp);
    }
    if let Err(_) = ORC.expirable_data.remove(&raw_data) {
        if let Ok(_) = ORC.expirable_data.remove(raw_data) {}
    }

    ok
}
