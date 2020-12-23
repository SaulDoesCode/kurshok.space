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
        let exp = IVec::from_i64(unix_timestamp() + from_now);
        let data = ExpirableData::Single{tree, key};
        
        self.expirable_data.insert(
            data.try_to_vec().unwrap(),
            exp
        ).is_ok()
    }

    pub fn expire_keys(&self, from_now: i64, tree: String, keys: Vec<String>) -> bool {
        let exp = IVec::from_i64(unix_timestamp() + from_now);
        let data = ExpirableData::MultiKey{tree, keys};

        self.expirable_data.insert(
            data.try_to_vec().unwrap(),
            exp
        ).is_ok()
    }

    pub fn expire_data(&self, from_now: i64, tree: BTreeMap<String, Vec<String>>) -> bool {
        let exp = IVec::from_i64(unix_timestamp() + from_now);
        let data = ExpirableData::MultiTree(tree);

        self.expirable_data.insert(
            data.try_to_vec().unwrap(),
            exp
        ).is_ok()
    }

    pub fn unexpire_data(&self, data: ExpirableData) -> Option<IVec> {
        if let Ok(o_exp) = self.expirable_data.remove(data.try_to_vec().unwrap()) {
            o_exp
        } else {
            None
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize)]
pub enum ExpirableData {
    Single {tree: String, key: String},
    MultiKey {tree: String, keys: Vec<String>},
    MultiTree(BTreeMap<String, Vec<String>>),
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct ExpirableValue<T> {
    value: T,
    expiry: i64,
}

impl<T> ExpirableValue<T> {
    fn new(value: T, expiry: i64) -> Self {
        Self{value, expiry}
    }

    fn has_expired(&self) -> bool {
        unix_timestamp() > self.expiry
    }

    fn value(self) -> Option<T> {
        if self.has_expired() {
            return None;
        }
        Some(self.value)
    }
}

pub fn start_system() -> thread::JoinHandle<()> {
    thread::spawn(|| {
        loop {
            thread::sleep(std::time::Duration::from_secs(60));
            if ORC.dev_mode {
                println!("cleaning up expired values...");
            }

            let mut expired_list = vec!();

            let mut iter = ORC.expirable_data.iter();
            while let Some(Ok((raw_data, exp))) = iter.next() {
                let now = unix_timestamp();

                if ORC.dev_mode {
                    println!("found something expirable now = {} > exp = {} == {}", now, exp.to_i64(), now == exp.to_i64());
                }

                if now > exp.to_i64() {
                    let data = ExpirableData::try_from_slice(&raw_data).unwrap();
                    expired_list.push(raw_data);
                    match data {
                        ExpirableData::Single{tree, key} => {
                            if ORC.dev_mode {
                                println!("going in for expiry: key - {} tree - {}", &key, &tree);
                            }
                            if let Ok(tr) = ORC.db.open_tree(tree.as_bytes()) {
                               if let Ok(Some(_)) = tr.remove(key.as_bytes()) {
                                   if ORC.dev_mode {
                                       println!("removed key - {} from tree - {}", key, tree);
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

            for data in expired_list {
                if let Ok(_) = ORC.expirable_data.remove(data) {}
            }
        }
    })
}
