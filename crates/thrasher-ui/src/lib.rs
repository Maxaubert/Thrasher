//! Thrasher's user interface: the Rust code Slint generates from
//! `crates/thrasher-app/ui/app.slint`. It lives in its own crate so the release profile
//! can build it for size and a change to the app's Rust does not compile this very large
//! module again (docs/building.md, Generated UI code).
#![allow(clippy::all)]

slint::include_modules!();
