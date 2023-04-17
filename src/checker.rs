use crate::finder::Checker;
use futures::{stream::futures_unordered::FuturesUnordered, StreamExt};
#[cfg(any(unix, target_os = "wasi"))]
use std::ffi::CString;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(target_os = "wasi")]
use std::os::wasi::ffi::OsStrExt;
use std::{future, iter::FromIterator, path::Path};

pub struct ExecutableChecker;

impl ExecutableChecker {
    pub fn new() -> ExecutableChecker {
        ExecutableChecker
    }
}

#[async_trait::async_trait]
impl Checker for ExecutableChecker {
    #[cfg(any(unix, target_os = "wasi"))]
    async fn is_valid(&self, path: &Path) -> bool {
        CString::new(path.as_os_str().as_bytes())
            .map(|c| unsafe { libc::access(c.as_ptr(), libc::X_OK) == 0 })
            .unwrap_or(false)
    }

    #[cfg(windows)]
    async fn is_valid(&self, _path: &Path) -> bool {
        true
    }
}

pub struct ExistedChecker;

impl ExistedChecker {
    pub fn new() -> ExistedChecker {
        ExistedChecker
    }
}

#[async_trait::async_trait]
impl Checker for ExistedChecker {
    #[cfg(target_os = "windows")]
    async fn is_valid(&self, path: &Path) -> bool {
        tokio::fs::symlink_metadata(path)
            .await
            .map(|metadata| {
                let file_type = metadata.file_type();
                file_type.is_file() || file_type.is_symlink()
            })
            .unwrap_or(false)
    }

    #[cfg(unix)]
    async fn is_valid(&self, path: &Path) -> bool {
        tokio::fs::metadata(path)
            .await
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
    }

    #[cfg(target_os = "wasi")]
    async fn is_valid(&self, path: &Path) -> bool {
        std::fs::metadata(path)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
    }
}

pub struct CompositeChecker {
    checkers: Vec<Box<dyn Checker>>,
}

impl CompositeChecker {
    pub fn new() -> CompositeChecker {
        CompositeChecker {
            checkers: Vec::new(),
        }
    }

    pub fn add_checker(mut self, checker: Box<dyn Checker>) -> CompositeChecker {
        self.checkers.push(checker);
        self
    }
}

#[async_trait::async_trait]
impl Checker for CompositeChecker {
    async fn is_valid(&self, path: &Path) -> bool {
        let jobs = self.checkers.iter().map(|checker| checker.is_valid(path));
        FuturesUnordered::from_iter(jobs).all(future::ready).await
    }
}
