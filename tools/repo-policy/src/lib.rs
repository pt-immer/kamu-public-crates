//! Repository policy: one decoder per repository artifact, and the checks that read them.
//!
//! Every pinned version this repository depends on is stated once, in
//! `.config/dev-tools.json`, and reached from here. A consumer that restates a pin instead of
//! reading it is a second copy, and the two only stay equal for as long as someone remembers.

pub mod dev_tools;
