#![cfg(not(target_arch = "wasm32"))]

// Compile the real matcher in a separate native test executable so its allocator
// instrumentation does not enter the venue library's credential-scope proof.
include!("../src/trading/acknowledgement.rs");

#[path = "support/acknowledgement_alloc_tests.rs"]
mod allocation_tests;
