use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use include_dir::{include_dir, Dir, File};
use serde::{Deserialize, Serialize};

static CAS_SEED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/cas-seed");

#[derive(Debug, Clone, Serialize)]
pub struct CasEntry {
    pub alias: String,
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct CasStore {
    root: PathBuf,
    objects: PathBuf,
    aliases_file: PathBuf,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct AliasIndex {
    aliases: BTreeMap<String, String>,
}

impl CasStore {
    pub fn open(root: PathBuf) -> Result<Self> {
        let objects = root.join("objects");
        fs::create_dir_all(&objects)
            .with_context(|| format!("failed to create CAS objects dir {}", objects.display()))?;

        Ok(Self {
            aliases_file: root.join("aliases.json"),
            root,
            objects,
        })
    }

    pub fn ingest_embedded_seed(&self) -> Result<()> {
        let mut index = self.read_index()?;

        for file in embedded_files(&CAS_SEED) {
            let alias = normalize_alias(file.path());
            if alias.is_empty() {
                continue;
            }

            index
                .aliases
                .insert(alias, self.put_bytes(file.contents())?);
        }

        self.write_index(&index)
    }

    pub fn put_bytes(&self, bytes: &[u8]) -> Result<String> {
        let hash = blake3::hash(bytes).to_hex().to_string();
        let path = self.object_path(&hash)?;

        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let tmp_path = path.with_extension("tmp");
            {
                let mut file = fs::File::create(&tmp_path)
                    .with_context(|| format!("failed to create {}", tmp_path.display()))?;
                file.write_all(bytes)?;
                file.sync_all()?;
            }
            fs::rename(&tmp_path, &path)?;
        }

        Ok(hash)
    }

    pub fn resolve(&self, id: &str) -> Result<String> {
        if Self::looks_like_hash(id) {
            return Ok(id.to_owned());
        }

        let index = self.read_index()?;
        index
            .aliases
            .get(id)
            .cloned()
            .with_context(|| format!("CAS alias not found: {id}"))
    }

    pub fn object_file(&self, id: &str) -> Result<PathBuf> {
        let hash = self.resolve(id)?;
        let path = self.object_path(&hash)?;
        if path.is_file() {
            Ok(path)
        } else {
            anyhow::bail!("CAS object not found: {hash}");
        }
    }

    pub fn read(&self, id: &str) -> Result<Vec<u8>> {
        let path = self.object_file(id)?;
        fs::read(&path).with_context(|| format!("failed to read {}", path.display()))
    }

    pub fn read_text(&self, id: &str) -> Result<String> {
        let bytes = self.read(id)?;
        String::from_utf8(bytes).with_context(|| format!("CAS object is not valid UTF-8: {id}"))
    }

    pub fn list(&self) -> Result<Vec<CasEntry>> {
        let index = self.read_index()?;
        let mut entries = Vec::with_capacity(index.aliases.len());

        for (alias, hash) in index.aliases {
            let path = self.object_path(&hash)?;
            let size = fs::metadata(path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            entries.push(CasEntry { alias, hash, size });
        }

        Ok(entries)
    }

    fn object_path(&self, hash: &str) -> Result<PathBuf> {
        if !Self::looks_like_hash(hash) {
            anyhow::bail!("invalid BLAKE3 hash: {hash}");
        }

        Ok(self.objects.join(&hash[0..2]).join(&hash[2..4]).join(hash))
    }

    fn read_index(&self) -> Result<AliasIndex> {
        if !self.aliases_file.exists() {
            return Ok(AliasIndex::default());
        }

        let bytes = fs::read(&self.aliases_file)
            .with_context(|| format!("failed to read {}", self.aliases_file.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", self.aliases_file.display()))
    }

    fn write_index(&self, index: &AliasIndex) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let bytes = serde_json::to_vec_pretty(index)?;
        fs::write(&self.aliases_file, bytes)
            .with_context(|| format!("failed to write {}", self.aliases_file.display()))
    }

    fn looks_like_hash(value: &str) -> bool {
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

fn embedded_files(dir: &'static Dir<'static>) -> Vec<&'static File<'static>> {
    let mut files = Vec::new();
    collect_embedded_files(dir, &mut files);
    files
}

fn collect_embedded_files(dir: &'static Dir<'static>, files: &mut Vec<&'static File<'static>>) {
    files.extend(dir.files());

    for child in dir.dirs() {
        collect_embedded_files(child, files);
    }
}

fn normalize_alias(path: &std::path::Path) -> String {
    path.to_string_lossy()
        .trim_start_matches('/')
        .replace(std::path::MAIN_SEPARATOR, "/")
}
