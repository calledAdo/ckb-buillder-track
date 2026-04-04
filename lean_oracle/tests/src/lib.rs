// This file is the root module list for the host-side test crate.
//
// The crate is intentionally simple: it only exposes test modules that exercise
// the shared parsing, hashing, proof, and signature logic.

// Only compile this module during tests.
#[cfg(test)]
mod oracle_data_tests;
#[cfg(test)]
mod oracle_integration_tests;
