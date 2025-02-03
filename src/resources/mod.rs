use std::{fs, path::{Path, PathBuf}};

pub mod buffer;
pub mod font;

pub struct Resources {
    base_dir: PathBuf,
}

impl Resources {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self { base_dir: base_dir.as_ref().to_owned() }
    }

    pub fn load_binary(&self, path: impl AsRef<Path>) -> anyhow::Result<Vec<u8>> {
        // TODO: WASM
        Ok(fs::read(self.base_dir.join(path))?)
    }

    pub fn load_string(&self, path: impl AsRef<Path>) -> anyhow::Result<String> {
        // TODO: WASM
        Ok(fs::read_to_string(self.base_dir.join(path))?)
    }
}