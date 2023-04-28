use std::{
    fs::{create_dir, try_exists, File},
    io::{ErrorKind, Read, Write},
    path::PathBuf,
};

use crate::object_id::ObjectId;

use super::ObjectStore;

pub struct DirectoryObjectStore {
    root: PathBuf,
}

impl DirectoryObjectStore {
    pub fn new(root: PathBuf) -> Result<Self, std::io::Error> {
        if !try_exists(&root)? {
            create_dir(&root)?;
        }
        Ok(Self { root })
    }
}

impl ObjectStore for DirectoryObjectStore {
    type Error = std::io::Error;

    fn has(&self, id: ObjectId) -> Result<bool, Self::Error> {
        let s: String = format!("{}", id);
        let subdir: &str = &s[0..2];
        let filename: &str = &s[2..];
        let path = self.root.join(format!("{}/{}", subdir, filename));
        std::fs::try_exists(path)
    }

    fn read(&self, id: ObjectId) -> Result<Option<Vec<u8>>, Self::Error> {
        let s: String = format!("{}", id);
        let subdir: &str = &s[0..2];
        let filename: &str = &s[2..];
        let path = self.root.join(format!("{}/{}", subdir, filename));
        match std::fs::File::options().read(true).open(path) {
            Ok(mut f) => {
                let mut v = Vec::new();
                f.read_to_end(&mut v)?;
                return Ok(Some(v));
            }
            Err(err) => {
                if err.kind() == ErrorKind::NotFound {
                    return Ok(None);
                } else {
                    return Err(err);
                }
            }
        }
    }

    fn insert(&mut self, object: &[u8]) -> Result<ObjectId, Self::Error> {
        let id: ObjectId = object.into();
        let s: String = format!("{}", id);
        let subdir: &str = &s[0..2];
        let filename: &str = &s[2..];
        let subdir_path = self.root.join(format!("{}", subdir));
        eprintln!("subdir_path: {:?}", subdir_path);
        if !std::fs::try_exists(&subdir_path)? {
            eprintln!("subdir doesn't exist");
            std::fs::create_dir(&subdir_path)?;
        }
        let path = subdir_path.join(format!("{}", filename));
        let mut f = File::options().create(true).write(true).open(path)?;
        f.write(object)?;
        Ok(id)
    }
}

#[test]
fn test_directory_object_store() {
    let tempdir = tempfile::tempdir().unwrap();
    let mut store = DirectoryObjectStore::new(tempdir.path().into()).unwrap();
    store.insert(b"hello, world").unwrap();
    let b: &[u8] = b"hello, world";
    assert!(store.has(b.into()).unwrap());
    assert_eq!(store.read(b.into()).unwrap(), Some(Vec::from(b)));
}
