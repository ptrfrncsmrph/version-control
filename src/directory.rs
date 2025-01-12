use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{read_dir, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{object_id::ObjectId, object_store::ObjectStore};

/// A directory tree, with [`ObjectId`]s at the leaves.
#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize, Default)]
pub struct Directory {
    #[serde(flatten)]
    pub root: BTreeMap<String, DirectoryEntry>,
}

#[derive(Debug)]
pub enum Error<Store: ObjectStore> {
    ObjectMissing(ObjectId),
    Store(Store::Error),
    IO(std::io::Error),
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct Diff {
    pub deleted: BTreeSet<String>,
    pub added: BTreeMap<String, DirectoryEntry>,
    pub modified: BTreeMap<String, DiffEntry>,
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum DiffEntry {
    File(ObjectId),
    Directory(Box<Diff>),
}

impl DirectoryEntry {
    pub fn diff(&self, other: &DirectoryEntry) -> Option<DiffEntry> {
        use DirectoryEntry::*;
        match (self, other) {
            (File(id), File(id_)) => {
                if id != id_ {
                    Some(DiffEntry::File(*id_))
                } else {
                    None
                }
            }
            (Directory(_), File(id)) => Some(DiffEntry::File(*id)),
            (File(_), Directory(d)) => Some(DiffEntry::Directory(Box::new(Diff {
                deleted: BTreeSet::new(),
                added: d.root.clone(),
                modified: BTreeMap::new(),
            }))),
            (Directory(d), Directory(d_)) => {
                if d == d_ {
                    None
                } else {
                    Some(DiffEntry::Directory(Box::new(d.diff(d_))))
                }
            }
        }
    }
}

impl Directory {
    /// Compute the diff between this directory structure and the one
    /// which is currently located at the path.
    pub fn diff(&self, other: &Directory) -> Diff {
        let added: BTreeMap<String, DirectoryEntry> = other
            .root
            .iter()
            .filter(|(file_name, _dir_entry)| !self.root.contains_key(*file_name))
            .map(|(fname, dir_entry)| (fname.clone(), dir_entry.clone()))
            .collect();
        let deleted: BTreeSet<String> = self
            .root
            .iter()
            .filter(|(file_name, _dir_entry)| !other.root.contains_key(*file_name))
            .map(|(fname, _dir_entry)| fname.clone())
            .collect();
        let modified: BTreeMap<String, DiffEntry> = self
            .root
            .iter()
            .filter_map(|(file_name, dir_entry)| {
                other.root.get(file_name).and_then(|other_dir_entry| {
                    dir_entry
                        .diff(other_dir_entry)
                        .map(|diff| (file_name.clone(), diff))
                })
            })
            .collect();
        Diff {
            added,
            deleted,
            modified,
        }
    }

    /// Write out the directory structure at the given directory path.
    ///
    /// The target directory must already exist.
    pub fn write<Store: ObjectStore>(
        &self,
        store: &Store,
        path: &Path,
    ) -> Result<(), Error<Store>> {
        if read_dir(path).is_ok() {
            for (file_name, entry) in self.root.iter() {
                match entry {
                    DirectoryEntry::File(id) => {
                        let v = store.read(*id).map_err(Error::Store)?;
                        match v {
                            Some(v) => {
                                let mut f = File::options()
                                    .create(true)
                                    .write(true)
                                    .open(path.join(file_name))
                                    .map_err(Error::IO)?;
                                f.write(&v).map_err(Error::IO)?;
                            }
                            None => return Err(Error::ObjectMissing(*id)),
                        }
                    }
                    DirectoryEntry::Directory(dir) => {
                        dir.write(store, PathBuf::from(path).join(file_name).as_path())?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// The set of file names which we will ignore at any level.
#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct Ignores {
    pub set: BTreeSet<String>,
}

impl Default for Ignores {
    fn default() -> Self {
        Ignores {
            set: vec![String::from(".rev")].into_iter().collect(),
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum DirectoryEntry {
    Directory(Box<Directory>),
    File(ObjectId),
}

impl Directory {
    pub fn new<Store: ObjectStore>(
        dir: &Path,
        ignores: &Ignores,
        store: &mut Store,
    ) -> Result<Self, Error<Store>> {
        let mut root = BTreeMap::new();
        for f in std::fs::read_dir(dir).map_err(Error::IO)? {
            let dir_entry = f.map_err(Error::IO)?;
            if ignores
                .set
                .contains(&dir_entry.file_name().into_string().unwrap())
            {
                continue;
            }
            let file_type = dir_entry.file_type().map_err(Error::IO)?;
            if file_type.is_dir() {
                let directory = Directory::new(dir_entry.path().as_path(), ignores, store)?;
                root.insert(
                    dir_entry.file_name().into_string().unwrap(),
                    DirectoryEntry::Directory(Box::new(directory)),
                );
            } else if file_type.is_file() {
                let id = ObjectId::try_from(dir_entry.path().as_path()).map_err(Error::IO)?;
                root.insert(
                    dir_entry.file_name().into_string().unwrap(),
                    DirectoryEntry::File(id),
                );
                let mut v = Vec::new();
                let mut obj_file = File::options()
                    .read(true)
                    .open(dir_entry.path())
                    .map_err(Error::IO)?;
                obj_file.read_to_end(&mut v).map_err(Error::IO)?;
                store.insert(&v).map_err(Error::Store)?;
            } else {
                eprintln!(
                    "TODO support things which aren't files or directories: {:?}",
                    dir_entry.file_name()
                );
            }
        }
        Ok(Directory { root })
    }
}

#[test]
fn test_directory() {
    use crate::object_store::in_memory::InMemoryObjectStore;
    use std::env::current_dir;
    let dir = current_dir().unwrap();
    let mut store = InMemoryObjectStore::new();
    let codebase = Directory::new(
        dir.as_path(),
        &Ignores {
            set: vec![
                String::from(".git"),
                String::from(".rev"),
                String::from("target"),
            ]
            .into_iter()
            .collect(),
        },
        &mut store,
    )
    .unwrap();
    let readme_path = String::from("README.md");
    assert!(codebase.root.get(&readme_path).is_some());
}
