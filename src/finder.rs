use crate::checker::CompositeChecker;
use crate::error::*;
#[cfg(windows)]
use crate::helper::has_executable_extension;
use either::Either;
use futures::prelude::*;
#[cfg(feature = "regex")]
use regex::Regex;
#[cfg(feature = "regex")]
use std::borrow::Borrow;
use std::ffi::OsStr;
use std::iter;
use std::path::{Path, PathBuf};

#[async_trait::async_trait]
pub trait Checker: Sync {
    async fn is_valid(&self, path: &Path) -> bool;
}

trait PathExt {
    fn has_separator(&self) -> bool;

    fn to_absolute<P>(self, cwd: P) -> PathBuf
    where
        P: AsRef<Path>;
}

impl PathExt for PathBuf {
    fn has_separator(&self) -> bool {
        self.components().count() > 1
    }

    fn to_absolute<P>(self, cwd: P) -> PathBuf
    where
        P: AsRef<Path>,
    {
        if self.is_absolute() {
            self
        } else {
            let mut new_path = PathBuf::from(cwd.as_ref());
            new_path.push(self);
            new_path
        }
    }
}

pub struct Finder;

impl Finder {
    pub fn new() -> Finder {
        Finder
    }

    #[cfg(target_os = "wasi")]
    fn path_split<U>(p: U) -> Vec<PathBuf>
    where
        U: AsRef<OsStr>,
    {
        p.as_ref()
            .to_string_lossy()
            .split(":")
            .map(PathBuf::from)
            .collect()
    }

    #[cfg(not(target_os = "wasi"))]
    fn path_split<U>(p: U) -> Vec<PathBuf>
    where
        U: AsRef<OsStr>,
    {
        std::env::split_paths(&p).collect()
    }

    pub fn find<T, U, V>(
        &self,
        binary_name: T,
        paths: Option<U>,
        cwd: Option<V>,
        binary_checker: CompositeChecker,
    ) -> impl Stream<Item = Result<PathBuf>>
    where
        T: AsRef<OsStr>,
        U: AsRef<OsStr>,
        V: AsRef<Path>,
    {
        let path = PathBuf::from(&binary_name);

        let binary_path_candidates = match (cwd, paths) {
            (Some(cwd), _) if path.has_separator() => {
                // Search binary in cwd if the path have a path separator.
                Ok(Either::Left(
                    Self::cwd_search_candidates(path, cwd).into_iter(),
                ))
            }
            (_, Some(p)) => {
                // Search binary in PATHs(defined in environment variable).
                let paths = Self::path_split(p);
                Ok(Either::Right(
                    Self::path_search_candidates(path, paths).into_iter(),
                ))
            }
            _ => Err(Error::CannotFindBinaryPath),
        };

        async_stream::try_stream! {
            for p in binary_path_candidates? {
                println!("antes {:?}", p);

                if binary_checker.is_valid(&p).await {
                println!("depois {:?}", p);

                    yield correct_casing(p).await;
                }
            }
        }
    }

    #[cfg(all(feature = "regex", all(target_os = "wasi")))]
    fn select_all_files(paths: Vec<PathBuf>) -> impl Stream<Item = PathBuf> {
        let iter = paths
            .into_iter()
            .map(std::fs::read_dir)
            .filter_map(|f| f.ok())
            .flatten()
            .filter_map(|f| f.ok())
            .map(|p| p.path());

        stream::iter(iter)
    }

    #[cfg(all(feature = "regex", not(target_os = "wasi")))]
    fn select_all_files(paths: Vec<PathBuf>) -> impl Stream<Item = PathBuf> {
        use futures::stream::FuturesUnordered;
        use tokio_stream::wrappers::ReadDirStream;

        let jobs = paths
            .into_iter()
            .map(|f| tokio::fs::read_dir(f))
            .collect::<FuturesUnordered<_>>();

        jobs.map_ok(ReadDirStream::new)
            .try_flatten()
            .filter_map(|f| async { f.ok() })
            .map(|f| f.path())
    }

    #[cfg(feature = "regex")]
    pub fn find_re<T>(
        &self,
        binary_regex: impl Borrow<Regex>,
        paths: T,
        binary_checker: CompositeChecker,
    ) -> impl Stream<Item = Result<PathBuf>>
    where
        T: AsRef<OsStr>,
    {
        let paths = Self::path_split(paths);
        async_stream::try_stream! {
            for await f in Self::select_all_files(paths) {
                if let Some(unicode_file_name) =  f.file_name().and_then(OsStr::to_str) {
                    if binary_regex.borrow().is_match(&unicode_file_name) && binary_checker.is_valid(&f).await {
                        yield f;
                    }
                }
            }
        }
    }

    fn cwd_search_candidates<C>(binary_name: PathBuf, cwd: C) -> impl IntoIterator<Item = PathBuf>
    where
        C: AsRef<Path>,
    {
        let path = binary_name.to_absolute(cwd);

        Self::append_extension(iter::once(path))
    }

    fn path_search_candidates<P>(
        binary_name: PathBuf,
        paths: P,
    ) -> impl IntoIterator<Item = PathBuf>
    where
        P: IntoIterator<Item = PathBuf>,
    {
        let new_paths = paths.into_iter().map(move |p| p.join(binary_name.clone()));

        Self::append_extension(new_paths)
    }

    #[cfg(unix)]
    fn append_extension<P>(paths: P) -> impl IntoIterator<Item = PathBuf>
    where
        P: IntoIterator<Item = PathBuf>,
    {
        paths
    }

    #[cfg(windows)]
    fn append_extension<P>(paths: P) -> impl IntoIterator<Item = PathBuf>
    where
        P: IntoIterator<Item = PathBuf>,
    {
        use once_cell::sync::Lazy;

        // Sample %PATHEXT%: .COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC
        // PATH_EXTENSIONS is then [".COM", ".EXE", ".BAT", …].
        // (In one use of PATH_EXTENSIONS we skip the dot, but in the other we need it;
        // hence its retention.)
        static PATH_EXTENSIONS: Lazy<Vec<String>> = Lazy::new(|| {
            std::env::var("PATHEXT")
                .map(|pathext| {
                    pathext
                        .split(';')
                        .filter_map(|s| {
                            if s.as_bytes().first() == Some(&b'.') {
                                Some(s.to_owned())
                            } else {
                                // Invalid segment; just ignore it.
                                None
                            }
                        })
                        .collect()
                })
                // PATHEXT not being set or not being a proper Unicode string is exceedingly
                // improbable and would probably break Windows badly. Still, don't crash:
                .unwrap_or_default()
        });

        paths
            .into_iter()
            .flat_map(move |p| -> Box<dyn Iterator<Item = _>> {
                // Check if path already have executable extension
                if has_executable_extension(&p, &PATH_EXTENSIONS) {
                    Box::new(iter::once(p))
                } else {
                    let bare_file = p.extension().map(|_| p.clone());
                    // Appended paths with windows executable extensions.
                    // e.g. path `c:/windows/bin[.ext]` will expand to:
                    // [c:/windows/bin.ext]
                    // c:/windows/bin[.ext].COM
                    // c:/windows/bin[.ext].EXE
                    // c:/windows/bin[.ext].CMD
                    // ...
                    Box::new(
                        bare_file
                            .into_iter()
                            .chain(PATH_EXTENSIONS.iter().map(move |e| {
                                // Append the extension.
                                let mut p = p.clone().into_os_string();
                                p.push(e);

                                PathBuf::from(p)
                            })),
                    )
                }
            })
    }

    #[cfg(target_os = "wasi")]
    fn append_extension<P>(paths: P) -> impl IntoIterator<Item = PathBuf>
    where
        P: IntoIterator<Item = PathBuf>,
    {
        paths
            .into_iter()
            .flat_map(|f| [f.clone(), f.with_extension(std::env::consts::EXE_EXTENSION)])
    }
}

#[cfg(target_os = "windows")]
async fn correct_casing(mut p: PathBuf) -> PathBuf {
    if let (Some(parent), Some(file_name)) = (p.parent(), p.file_name()) {
        if let Ok(mut iter) = tokio::fs::read_dir(parent).await {
            while let Ok(e) = iter.next_entry().await {
                if let Some(e) = e {
                    if e.file_name().eq_ignore_ascii_case(file_name) {
                        p.pop();
                        p.push(e.file_name());
                        break;
                    }
                }
            }
        }
    }
    p
}

#[cfg(not(target_os = "windows"))]
async fn correct_casing(p: PathBuf) -> PathBuf {
    p
}
