#![allow(clippy::expect_used, clippy::panic, dead_code, deprecated)]

mod common;

mod add;
mod config;
mod help;
mod hooks;
mod list;
mod remove;
mod trust;

// jj (Jujutsu) integration tests
mod jj_add;
mod jj_list;
mod jj_remove;
