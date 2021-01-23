use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Serialize, Deserialize};
use sled::{transaction::*, Transactional};
use rayon::prelude::*;

use std::{
    thread,
    collections::BTreeMap
};

use crate::orchestrator::{Orchestrator, ORC};
use crate::utils::{unix_timestamp};

impl Orchestrator {
    pub fn expire_data(&self, from_now: i64, data: ExpirableData, unexpire_key: Option<&[u8]>) -> bool {
        let exp = unix_timestamp() + from_now;

        let res: TransactionResult<(), ()> = (
            &self.expirable_data,
            &self.expirable_data_unexpire_keys
        ).transaction(|(ed, unexpire_keys)| {
            if let Some(uk) = unexpire_key {
                if unexpire_keys.insert(uk, &exp.to_be_bytes())?.is_some() {
                    return Err(sled::transaction::ConflictableTransactionError::Abort(()));
                }
            }

            let container = ExpDataContainer{
                data: data.clone(),
                unexpire_key: unexpire_key.map(|key| key.to_vec())
            };

            if let Some(raw) = ed.get(&exp.to_be_bytes())? {
                let mut old_containers: Vec<ExpDataContainer> = BorshDeserialize::try_from_slice(&raw).unwrap();
                old_containers.push(container);

                ed.insert(
                    &exp.to_be_bytes(),
                    old_containers.try_to_vec().unwrap().as_slice()
                )?;
            } else {
                let containers: Vec<ExpDataContainer> = vec![container];
                ed.insert(
                    &exp.to_be_bytes(),
                    containers.try_to_vec().unwrap().as_slice()
                )?;
            }
            Ok(())
        });

        res.is_ok()
    }

    pub fn unexpire_data(&self, unexpire_key: &[u8]) -> bool {
        let res: TransactionResult<(), ()> = (
            &self.expirable_data,
            &self.expirable_data_unexpire_keys
        ).transaction(|(ed, unexpire_keys)| {
            if let Some(key) = unexpire_keys.remove(unexpire_key)? {
                if let Some(raw) = ed.get(&key)? {
                    let mut containers: Vec<ExpDataContainer> = BorshDeserialize::try_from_slice(&raw).unwrap();
                    let uk = unexpire_key.to_vec();
                    containers.drain_filter(|c| {
                        if let Some(og_uk) = &c.unexpire_key {
                            return og_uk.eq(&uk)
                        }
                        false
                    });

                    if containers.len() == 0 {
                        ed.remove(key)?;
                    } else {
                        ed.insert(key, containers.try_to_vec().unwrap().as_slice())?;
                    }
                    return Ok(());
                }
            }
            if self.dev_mode {
                println!("failed to unexpire data with key {:?}", unexpire_key);
            }
            Err(sled::transaction::ConflictableTransactionError::Abort(()))
        });

        res.is_ok()
    }
}

/*
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
        ExpirableData::MultiTree(old_map) => map.extend(old_map),
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
*/

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone)]
pub enum ExpirableData {
    Single {tree: String, key: Vec<u8>},
    MultiKey {tree: String, keys: Vec<Vec<u8>>},
    MultiTree(BTreeMap<String, Vec<Vec<u8>>>),
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
struct ExpDataContainer{
    unexpire_key: Option<Vec<u8>>,
    data: ExpirableData,
}

pub fn start_system() -> thread::JoinHandle<()> {
    thread::spawn(|| {
        loop {
            clean_up_all();
            thread::sleep(std::time::Duration::from_secs(1));
        }
    })
}

pub fn clean_up_all() {
    let now = unix_timestamp();
    let now_bytes: &[u8] = &now.to_be_bytes();

    let mut iter = ORC.expirable_data.range(..now_bytes);
    while let Some(Ok((key, raw))) = iter.next() {
        let containers: Vec<ExpDataContainer> = BorshDeserialize::try_from_slice(&raw).unwrap();

        containers.par_iter().for_each(|c| {
            if ORC.dev_mode {
                println!(
                    "found something expirable: {:?}",
                    &c.data
                );
            }


            if let Some(uk) = &c.unexpire_key {
                if let Err(_) = ORC.expirable_data_unexpire_keys.remove(uk.as_slice()) {}
            }

            if clean_up_expirable_datum(&c.data) && ORC.dev_mode {
                println!("cleaned up expirable datum");
            }
        });

        if let Err(_) = ORC.expirable_data.remove(key) {}
    }
}

fn clean_up_expirable_datum(data: &ExpirableData) -> bool {
    let mut ok = false;
    match data {
        ExpirableData::Single{tree, key} => {
            if let Ok(tr) = ORC.db.open_tree(tree.as_bytes()) {
                if let Ok(Some(_)) = tr.remove(key.as_slice()) {
                    ok = true;
                    if ORC.dev_mode {
                        println!("removing key from tree - {} went ok", tree);
                    }
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

    ok
}
