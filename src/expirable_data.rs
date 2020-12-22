use borsh::{BorshDeserialize, BorshSerialize};
use sled::{transaction::*, IVec, Transactional};

use std::{
    thread,
    collections::BTreeMap
};

use crate::orchestrator::{Orchestrator, ORC};
use crate::utils::{unix_timestamp, FancyIVec};

impl Orchestrator {
    pub fn expire_key(&self, from_now: i64, tree: String, key: String) -> bool {
        let exp = unix_timestamp() + from_now;
        let value = ExpirableData::Single{tree, key};

        self.expirable_data.insert(
            IVec::from_i64(exp),
            value.try_to_vec().unwrap()
        ).is_ok()
    }

    pub fn expire_keys(&self, from_now: i64, tree: String, keys: Vec<String>) -> bool {
        let exp = unix_timestamp() + from_now;
        let value = ExpirableData::MultiKey{tree, keys};

        self.expirable_data.insert(
            IVec::from_i64(exp),
            value.try_to_vec().unwrap()
        ).is_ok()
    }

    pub fn expire_data(&self, from_now: i64, tree: BTreeMap<String, Vec<String>>) -> bool {
        let exp = unix_timestamp() + from_now;
        let value = ExpirableData::MultiTree(tree);

        self.expirable_data.insert(
            IVec::from_i64(exp),
            value.try_to_vec().unwrap()
        ).is_ok()
    }
}

#[derive(BorshSerialize, BorshDeserialize)]
pub enum ExpirableData {
    Single {tree: String, key: String},
    MultiKey {tree: String, keys: Vec<String>},
    MultiTree(BTreeMap<String, Vec<String>>),
}

pub fn start_system() -> thread::JoinHandle<()> {
    thread::spawn(|| {
        loop {
            thread::sleep(std::time::Duration::from_secs(90));
            if ORC.dev_mode {
                println!("cleaning up expired values...");
            }

            let mut expired_list = vec!();

            let mut iter = ORC.expirable_data.iter();
            let now = unix_timestamp();
            while let Some(Ok((key, value))) = iter.next() {
                let exp = key.to_i64();
                if now > exp {
                    expired_list.push(key);
                    let data = ExpirableData::try_from_slice(&value).unwrap();
                    match data {
                        ExpirableData::Single{tree, key} => {
                            if let Ok(tr) = ORC.db.open_tree(tree.as_bytes()) {
                               if let Ok(Some(_)) = tr.remove(key.as_bytes()) {
                                   if ORC.dev_mode {
                                       println!("removed key - {} from tree - {}", tree, key);
                                   }
                               }
                            }
                        },
                        ExpirableData::MultiKey{tree, keys} => {
                            if let Ok(tr) = ORC.db.open_tree(tree.as_bytes()) {
                                let res: TransactionResult<(), ()> = tr.transaction(|tr| {
                                    for key in keys.iter() {
                                        tr.remove(key.as_bytes())?;
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
                            }
                        },
                        ExpirableData::MultiTree(map) => {
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
                                            tr.remove(key.as_bytes())?;
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
                }
            }

            for key in expired_list {
                if let Ok(_) = ORC.expirable_data.remove(key) {}
            }
        }
    })
}
