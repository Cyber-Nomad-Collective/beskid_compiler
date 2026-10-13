//! The single release harness executable (`harness-beskid_tests_release`).
//!
//! Every module below holds the reviewed host-level release cases of one family. The producer runs
//! `--exact <module>::<test>` once per required id; the release reader re-derives each outcome from
//! the retained libtest output and the installed invocation proof lines.
#![allow(non_snake_case)]

mod bsol_host_v06;
mod cli_journeys_v06;
mod dependency_journeys_v06;
mod serialization_host_v06;
