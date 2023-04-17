#![cfg_attr(target_os = "wasi", feature(wasi_ext))]

extern crate async_which;

use futures::{Stream, StreamExt};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::{env, vec};
use tempfile::TempDir;
use tokio::io;

#[cfg(all(unix, feature = "regex"))]
use futures::TryStreamExt;
#[cfg(all(unix, feature = "regex"))]
use regex::Regex;

struct TestFixture {
    /// Temp directory.
    pub tempdir: TempDir,
    /// $PATH
    pub paths: OsString,
    /// Binaries created in $PATH
    pub bins: Vec<PathBuf>,
}

const SUBDIRS: &[&str] = &["a", "b", "c"];
const BIN_NAME: &str = "bin";

#[cfg(all(unix, not(target_os = "wasi")))]
async fn mk_bin(dir: &Path, path: &str, extension: &str) -> io::Result<PathBuf> {
    let bin = dir.join(path).with_extension(extension);

    #[cfg(target_os = "macos")]
    let mode = libc::S_IXUSR as u32;
    #[cfg(target_os = "linux")]
    let mode = libc::S_IXUSR;
    let mode = 0o666 | mode;
    tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .mode(mode)
        .open(&bin)
        .await
        .and_then(|_f| bin.canonicalize())
}

#[cfg(not(target_os = "wasi"))]
async fn touch(dir: &Path, path: &str, extension: &str) -> io::Result<PathBuf> {
    let b = dir.join(path).with_extension(extension);
    tokio::fs::File::create(&b)
        .await
        .and_then(|_f| b.canonicalize())
}

#[cfg(windows)]
async fn mk_bin(dir: &Path, path: &str, extension: &str) -> io::Result<PathBuf> {
    touch(dir, path, extension).await
}

#[cfg(target_os = "wasi")]
async fn mk_bin(dir: &Path, path: &str, extension: &str) -> io::Result<PathBuf> {
    let bin = dir.join(path).with_extension(extension);
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&bin)
        .and_then(|_f| Ok(bin.to_path_buf()))
}

#[cfg(target_os = "wasi")]
async fn touch(dir: &Path, path: &str, extension: &str) -> io::Result<PathBuf> {
    let b = dir.join(path).with_extension(extension);
    std::fs::File::create(&b).and_then(|_f| b.canonicalize())
}

impl TestFixture {
    // tmp/a/bin
    // tmp/a/bin.exe
    // tmp/a/bin.cmd
    // tmp/b/bin
    // tmp/b/bin.exe
    // tmp/b/bin.cmd
    // tmp/c/bin
    // tmp/c/bin.exe
    // tmp/c/bin.cmd
    #[cfg(not(target_os = "wasi"))]
    pub async fn new() -> TestFixture {
        let tempdir = tempfile::tempdir().unwrap();
        let mut builder = tokio::fs::DirBuilder::new();
        builder.recursive(true);
        let mut paths = vec![];
        let mut bins = vec![];
        for d in SUBDIRS.iter() {
            let p = tempdir.path().join(d);
            builder.create(&p).await.unwrap();
            bins.push(mk_bin(&p, BIN_NAME, "").await.unwrap());
            bins.push(mk_bin(&p, BIN_NAME, "exe").await.unwrap());
            bins.push(mk_bin(&p, BIN_NAME, "cmd").await.unwrap());
            paths.push(p);
        }
        let p = tempdir.path().join("win-bin");
        builder.create(&p).await.unwrap();
        bins.push(mk_bin(&p, "win-bin", "exe").await.unwrap());
        paths.push(p);
        TestFixture {
            tempdir,
            paths: env::join_paths(paths).unwrap(),
            bins,
        }
    }

    #[cfg(target_os = "wasi")]
    pub async fn new() -> TestFixture {
        let tempdir = tempfile::tempdir_in("/tmp").unwrap();
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        let mut paths = vec![];
        let mut bins = vec![];
        for d in SUBDIRS.iter() {
            let p = tempdir.path().join(d);
            builder.create(&p).unwrap();
            bins.push(mk_bin(&p, BIN_NAME, "").await.unwrap());
            bins.push(mk_bin(&p, BIN_NAME, "exe").await.unwrap());
            bins.push(mk_bin(&p, BIN_NAME, "cmd").await.unwrap());
            paths.push(p);
        }
        let p = tempdir.path().join("win-bin");
        builder.create(&p).unwrap();
        bins.push(mk_bin(&p, "win-bin", "exe").await.unwrap());
        paths.push(p);
        TestFixture {
            tempdir,
            paths: paths
                .into_iter()
                .map(PathBuf::into_os_string)
                .collect::<Vec<_>>()
                .join(OsStr::new(":")),
            bins,
        }
    }

    #[allow(dead_code)]
    pub async fn touch(&self, path: &str, extension: &str) -> io::Result<PathBuf> {
        touch(self.tempdir.path(), path, extension).await
    }

    pub async fn mk_bin(&self, path: &str, extension: &str) -> io::Result<PathBuf> {
        mk_bin(self.tempdir.path(), path, extension).await
    }
}

async fn _which<T: AsRef<OsStr>>(
    f: &TestFixture,
    path: T,
) -> async_which::Result<async_which::CanonicalPath> {
    async_which::CanonicalPath::new_in(path, Some(f.paths.clone()), f.tempdir.path()).await
}

fn _which_all<'a, T: AsRef<OsStr> + 'a>(
    f: &'a TestFixture,
    path: T,
) -> impl Stream<Item = async_which::Result<async_which::CanonicalPath>> + '_ {
    async_which::CanonicalPath::all_in(path, Some(f.paths.clone()), f.tempdir.path())
}

#[tokio::test]
#[cfg(unix)]
async fn it_works() {
    use std::process::Command;
    let result = async_which::Path::new("rustc").await;
    assert!(result.is_ok());

    let which_result = Command::new("which").arg("rustc").output();

    assert_eq!(
        String::from(result.unwrap().to_str().unwrap()),
        String::from_utf8(which_result.unwrap().stdout)
            .unwrap()
            .trim()
    );
}

#[tokio::test]
#[cfg(unix)]
async fn test_which() {
    let f = TestFixture::new().await;
    assert_eq!(_which(&f, BIN_NAME).await.unwrap(), f.bins[0])
}

#[tokio::test]
#[cfg(windows)]
async fn test_which() {
    let f = TestFixture::new().await;
    assert_eq!(_which(&f, BIN_NAME).await.unwrap(), f.bins[1])
}

#[tokio::test]
#[cfg(all(unix, feature = "regex"))]
async fn test_which_re_in_with_matches() {
    let f = TestFixture::new().await;
    f.mk_bin("a/bin_0", "").await.unwrap();
    f.mk_bin("b/bin_1", "").await.unwrap();
    let re = Regex::new(r"bin_\d").unwrap();

    let result: Vec<PathBuf> = async_which::which_re_in(re, f.paths)
        .try_collect()
        .await
        .unwrap();

    let temp = f.tempdir;

    assert_eq!(
        result,
        vec![temp.path().join("a/bin_0"), temp.path().join("b/bin_1")]
    )
}

#[tokio::test]
#[cfg(all(unix, feature = "regex"))]
async fn test_which_re_in_without_matches() {
    let f = TestFixture::new().await;
    let re = Regex::new(r"bi[^n]").unwrap();

    let result: Vec<PathBuf> = async_which::which_re_in(re, f.paths)
        .try_collect()
        .await
        .unwrap();

    assert_eq!(result, Vec::<PathBuf>::new())
}

#[tokio::test]
#[cfg(all(unix, feature = "regex"))]
async fn test_which_re_accepts_owned_and_borrow() {
    let async_drop = |_| async {};

    async_which::which_re(Regex::new(r".").unwrap())
        .for_each(async_drop)
        .await;
    async_which::which_re(&Regex::new(r".").unwrap())
        .for_each(async_drop)
        .await;
    async_which::which_re_in(Regex::new(r".").unwrap(), "pth")
        .for_each(async_drop)
        .await;
    async_which::which_re_in(&Regex::new(r".").unwrap(), "pth")
        .for_each(async_drop)
        .await;
}

#[tokio::test]
#[cfg(unix)]
async fn test_which_extension() {
    let f = TestFixture::new().await;
    let b = Path::new(&BIN_NAME).with_extension("");
    assert_eq!(_which(&f, b).await.unwrap(), f.bins[0])
}

#[tokio::test]
#[cfg(windows)]
async fn test_which_extension() {
    let f = TestFixture::new().await;
    let b = Path::new(&BIN_NAME).with_extension("cmd");
    assert_eq!(_which(&f, b).await.unwrap(), f.bins[2])
}

#[tokio::test]
#[cfg(windows)]
async fn test_which_no_extension() {
    let f = TestFixture::new().await;
    let b = Path::new("win-bin");
    let which_result = async_which::which_in(b, Some(&f.paths), ".").await.unwrap();
    // Make sure the extension is the correct case.
    assert_eq!(which_result.extension(), f.bins[9].extension());
    assert_eq!(
        tokio::fs::canonicalize(&which_result).await.unwrap(),
        f.bins[9]
    )
}

#[tokio::test]
async fn test_which_not_found() {
    let f = TestFixture::new().await;
    assert!(_which(&f, "a").await.is_err());
}

#[tokio::test]
async fn test_which_second() {
    let f = TestFixture::new().await;
    let b = f
        .mk_bin("b/another", env::consts::EXE_EXTENSION)
        .await
        .unwrap();
    assert_eq!(_which(&f, "another").await.unwrap(), b);
}

#[tokio::test]
#[cfg(not(target_os = "wasi"))]
async fn test_which_all() {
    let f = TestFixture::new().await;
    let actual = _which_all(&f, BIN_NAME)
        .map(|c| c.unwrap())
        .collect::<Vec<_>>()
        .await;
    let mut expected = f
        .bins
        .iter()
        .map(|p| p.canonicalize().unwrap())
        .collect::<Vec<_>>();
    #[cfg(windows)]
    {
        expected.retain(|p| p.file_stem().unwrap() == BIN_NAME);
        expected.retain(|p| p.extension().map(|ext| ext == "exe" || ext == "cmd") == Some(true));
    }
    #[cfg(not(windows))]
    {
        expected.retain(|p| p.file_name().unwrap() == BIN_NAME);
    }
    assert_eq!(actual, expected);
}

#[tokio::test]
#[cfg(target_os = "wasi")]
async fn test_which_all() {
    let f = TestFixture::new().await;
    let actual = _which_all(&f, BIN_NAME)
        .map(|c| c.unwrap())
        .collect::<Vec<_>>()
        .await;
    let mut expected = f.bins;

    #[cfg(windows)]
    {
        expected.retain(|p| p.file_stem().unwrap() == BIN_NAME);
        expected.retain(|p| p.extension().map(|ext| ext == "exe" || ext == "cmd") == Some(true));
    }
    #[cfg(not(windows))]
    {
        expected.retain(|p| p.file_name().unwrap() == BIN_NAME);
    }
    assert_eq!(actual, expected);
}

#[tokio::test]
#[cfg(unix)]
async fn test_which_absolute() {
    let f = TestFixture::new().await;
    assert_eq!(
        _which(&f, &f.bins[3]).await.unwrap(),
        f.bins[3].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(windows)]
async fn test_which_absolute() {
    let f = TestFixture::new().await;
    assert_eq!(
        _which(&f, &f.bins[4]).await.unwrap(),
        f.bins[4].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(windows)]
async fn test_which_absolute_path_case() {
    // Test that an absolute path with an uppercase extension
    // is accepted.
    let f = TestFixture::new().await;
    let p = &f.bins[4];
    assert_eq!(
        _which(&f, p).await.unwrap(),
        f.bins[4].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(unix)]
async fn test_which_absolute_extension() {
    let f = TestFixture::new().await;
    // Don't append EXE_EXTENSION here.
    let b = f.bins[3].parent().unwrap().join(BIN_NAME);
    assert_eq!(
        _which(&f, b).await.unwrap(),
        f.bins[3].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(windows)]
async fn test_which_absolute_extension() {
    let f = TestFixture::new().await;
    // Don't append EXE_EXTENSION here.
    let b = f.bins[4].parent().unwrap().join(BIN_NAME);
    assert_eq!(
        _which(&f, b).await.unwrap(),
        f.bins[4].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(unix)]
async fn test_which_relative() {
    let f = TestFixture::new().await;
    assert_eq!(
        _which(&f, "b/bin").await.unwrap(),
        f.bins[3].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(windows)]
async fn test_which_relative() {
    let f = TestFixture::new().await;
    assert_eq!(
        _which(&f, "b/bin").await.unwrap(),
        f.bins[4].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(unix)]
async fn test_which_relative_extension() {
    // test_which_relative tests a relative path without an extension,
    // so test a relative path with an extension here.
    let f = TestFixture::new().await;
    let b = Path::new("b/bin").with_extension(env::consts::EXE_EXTENSION);
    assert_eq!(
        _which(&f, b).await.unwrap(),
        f.bins[3].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(windows)]
async fn test_which_relative_extension() {
    // test_which_relative tests a relative path without an extension,
    // so test a relative path with an extension here.
    let f = TestFixture::new().await;
    let b = Path::new("b/bin").with_extension("cmd");
    assert_eq!(
        _which(&f, b).await.unwrap(),
        f.bins[5].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(windows)]
async fn test_which_relative_extension_case() {
    // Test that a relative path with an uppercase extension
    // is accepted.
    let f = TestFixture::new().await;
    let b = Path::new("b/bin").with_extension("EXE");
    assert_eq!(
        _which(&f, b).await.unwrap(),
        f.bins[4].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(unix)]
async fn test_which_relative_leading_dot() {
    let f = TestFixture::new().await;
    assert_eq!(
        _which(&f, "./b/bin").await.unwrap(),
        f.bins[3].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(windows)]
async fn test_which_relative_leading_dot() {
    let f = TestFixture::new().await;
    assert_eq!(
        _which(&f, "./b/bin").await.unwrap(),
        f.bins[4].canonicalize().unwrap()
    );
}

#[tokio::test]
#[cfg(unix)]
async fn test_which_non_executable() {
    // Shouldn't return non-executable files.
    let f = TestFixture::new().await;
    f.touch("b/another", "").await.unwrap();
    assert!(_which(&f, "another").await.is_err());
}

#[tokio::test]
#[cfg(unix)]
async fn test_which_absolute_non_executable() {
    // Shouldn't return non-executable files, even if given an absolute path.
    let f = TestFixture::new().await;
    let b = f.touch("b/another", "").await.unwrap();
    assert!(_which(&f, b).await.is_err());
}

#[tokio::test]
#[cfg(unix)]
async fn test_which_relative_non_executable() {
    // Shouldn't return non-executable files.
    let f = TestFixture::new().await;
    f.touch("b/another", "").await.unwrap();
    assert!(_which(&f, "b/another").await.is_err());
}

#[tokio::test]
async fn test_failure() {
    let f = TestFixture::new().await;

    let run = async move {
        let p = _which(&f, "./b/bin").await?;
        async_which::Result::Ok(p.into_path_buf())
    };

    let _ = run.await;
}
