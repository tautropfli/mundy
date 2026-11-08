#![allow(unused_imports, dead_code)]

#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    all(target_family = "wasm", target_os = "unknown")
))]
mod scan;
#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    all(target_family = "wasm", target_os = "unknown")
))]
pub(crate) use scan::*;

mod dedup;
pub(crate) use dedup::*;

#[cfg(target_os = "linux")]
mod either;
#[cfg(target_os = "linux")]
pub(crate) use either::*;

#[cfg(any(target_os = "linux", test))]
mod merge;
#[cfg(any(target_os = "linux", test))]
pub(crate) use merge::*;
