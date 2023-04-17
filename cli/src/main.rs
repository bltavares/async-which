use std::{collections::HashSet, path::PathBuf};

use futures::TryStreamExt;
use regex::Regex;

#[cfg(not(windows))]
fn file_name(f: PathBuf) -> String {
    f.display().to_string()
}

#[cfg(windows)]
fn file_name(f: PathBuf) -> String {
    f.display().to_string().to_lowercase()
}


#[cfg_attr(not(target_os = "wasi"), tokio::main)]
#[cfg_attr(target_os = "wasi", tokio::main(flavor = "current_thread"))]
async fn main() {
    let name = std::env::args().nth(1).expect("No binary name given as first argument.");
    let regex = Regex::new(&name).expect("not a valid regex pattern.");

    let found = async_which::which_re(regex)
        .map_ok(file_name)
        .try_collect::<HashSet<_>>()
        .await.expect("failed to find binary");

    if found.is_empty() {
        println!("Not found: {}", name);
        return;
    }

    for path in found {
        println!("{}", path);
    }
}
