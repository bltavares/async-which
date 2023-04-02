[![Build Status](https://github.com/harryfei/which-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/harryfei/which-rs/actions/workflows/rust.yml)

# which

A Rust equivalent of Unix command "which", written in async.
Locate installed executable in cross platforms.

## Support platforms

* Linux
* Windows
* macOS
* wasi (experimental, nightly w/o default-features)

## Examples

1) To find which rustc executable binary is using.

    ``` rust
    use async_which::which;

    let result = which("rustc").await.unwrap();
    assert_eq!(result, PathBuf::from("/usr/bin/rustc"));
    ```

2. After enabling the `regex` feature, find all cargo subcommand executables on the path:

    ``` rust
    use async_which::which_re;

    which_re(Regex::new("^cargo-.*").unwrap()).await.unwrap()
        .for_each(|pth| println!("{}", pth.to_string_lossy()));
    ```

## Documentation

The documentation is [available online](https://docs.rs/async-which/).

## Original

This is a fork of [harryfei/which-rs](https://github.com/harryfei/which-rs) adding the async filesystem calls.
All credits to them.
