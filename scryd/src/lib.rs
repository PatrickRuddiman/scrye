//! Helpers shared between the `scryd` bin target and its integration
//! tests. Today: path resolution for `/etc/scryd/config.toml` and
//! `/run/scryd/scryd.sock` under sudo as well as the v0.1.0 per-user
//! XDG paths.

pub mod path_resolution;
