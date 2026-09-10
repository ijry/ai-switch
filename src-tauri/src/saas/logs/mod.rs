mod file;
mod memory;
mod postgres;
mod redis;
mod runtime;
mod types;

pub use file::FileLogStore;
pub use memory::{LogQueue, MemoryLogQueue, QueueBatch, QueueStats};
pub use postgres::PostgresLogStore;
pub use redis::RedisLogQueue;
pub use runtime::LogRuntime;
pub use types::*;

#[cfg(test)]
pub(crate) fn canonical_test_directory() -> tempfile::TempDir {
    let root = std::env::temp_dir()
        .canonicalize()
        .expect("resolve temporary directory");
    tempfile::tempdir_in(root).expect("create canonical test directory")
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;
