//! Persistent IR cache using content-addressed storage.
//!
//! Each IR node is serialized to JSON and stored on disk keyed by its
//! `ContentHash`. Only nodes whose content hash differs from the cached
//! version are re-serialized, enabling incremental compilation.
//!
//! Cache layout:
//! ```text
//! .brak-cache/
//!   ast/
//!     <hash>.json
//!   hir/
//!     <hash>.json
//!   mir/
//!     <hash>.json
//!   lir/
//!     <hash>.json
//! ```

use std::path::PathBuf;
use brak_core::Result;
use brak_ir_ast::ast::Program;
use brak_ir_hir::hir::HirProgram;
use brak_ir_mir::mir::MirProgram;
use brak_ir_lir::lir::LirProgram;

/// The bitcode cache manages serialized IR at all levels.
pub struct BitcodeCache {
    root: PathBuf,
}

impl BitcodeCache {
    /// Create a new cache rooted at `path` (typically `.brak-cache/`).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let root = path.into();
        let dirs = ["ast", "hir", "mir", "lir"];
        for d in &dirs {
            let _ = std::fs::create_dir_all(root.join(d));
        }
        Self { root }
    }

    /// Load cached AST if its hash matches; otherwise run `produce` and cache
    /// the result before returning it.
    pub fn get_or_compute_ast(
        &self,
        hash: u64,
        produce: impl FnOnce() -> Program,
    ) -> Result<Program> {
        self.get_or_compute("ast", hash, produce)
    }

    /// Load cached HIR if its hash matches; otherwise run `produce` and cache
    /// the result before returning it.
    pub fn get_or_compute_hir(
        &self,
        hash: u64,
        produce: impl FnOnce() -> HirProgram,
    ) -> Result<HirProgram> {
        self.get_or_compute("hir", hash, produce)
    }

    /// Load cached MIR if its hash matches; otherwise run `produce` and cache
    /// the result before returning it.
    pub fn get_or_compute_mir(
        &self,
        hash: u64,
        produce: impl FnOnce() -> MirProgram,
    ) -> Result<MirProgram> {
        self.get_or_compute("mir", hash, produce)
    }

    /// Load cached LIR if its hash matches; otherwise run `produce` and cache
    /// the result before returning it.
    pub fn get_or_compute_lir(
        &self,
        hash: u64,
        produce: impl FnOnce() -> LirProgram,
    ) -> Result<LirProgram> {
        self.get_or_compute("lir", hash, produce)
    }

    /// Check if a cached entry exists for the given hash at the given level.
    pub fn contains(&self, level: &str, hash: u64) -> bool {
        self.path_for(level, hash).exists()
    }

    /// Remove the cached entry for the given hash at the given level.
    pub fn invalidate(&self, level: &str, hash: u64) -> Result<()> {
        let path = self.path_for(level, hash);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Clear all cached entries at all levels.
    pub fn clear_all(&self) -> Result<()> {
        for level in &["ast", "hir", "mir", "lir"] {
            let dir = self.root.join(level);
            if dir.exists() {
                std::fs::remove_dir_all(&dir)?;
                std::fs::create_dir_all(&dir)?;
            }
        }
        Ok(())
    }

    // ── internal helpers ────────────────────────────────────────

    fn path_for(&self, level: &str, hash: u64) -> PathBuf {
        self.root.join(level).join(format!("{:016x}.json", hash))
    }

    fn get_or_compute<T: serde::Serialize + serde::de::DeserializeOwned>(
        &self,
        level: &str,
        hash: u64,
        produce: impl FnOnce() -> T,
    ) -> Result<T> {
        let path = self.path_for(level, hash);

        if path.exists() {
            let bytes = std::fs::read(&path)?;
            let value = serde_json::from_slice(&bytes)?;
            return Ok(value);
        }

        let value = produce();
        let bytes = serde_json::to_vec(&value)?;
        std::fs::write(&path, &bytes)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brak_core::ContentHash;

    #[test]
    fn test_cache_store_and_load() {
        let dir = std::env::temp_dir().join("brak_bitcode_test");
        let _ = std::fs::remove_dir_all(&dir);
        let cache = BitcodeCache::new(&dir);

        let prog = Program { items: vec![] };
        let hash = prog.content_hash();
        let loaded = cache.get_or_compute_ast(hash, || Program { items: vec![] }).unwrap();
        assert!(loaded.items.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_hit() {
        let dir = std::env::temp_dir().join("brak_bitcode_hit_test");
        let _ = std::fs::remove_dir_all(&dir);
        let cache = BitcodeCache::new(&dir);

        let prog = Program { items: vec![] };
        let hash = prog.content_hash();
        let _first = cache.get_or_compute_ast(hash, || prog).unwrap();
        let second = cache.get_or_compute_ast(hash, || {
            panic!("should not be called on cache hit");
        }).unwrap();
        assert!(second.items.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
