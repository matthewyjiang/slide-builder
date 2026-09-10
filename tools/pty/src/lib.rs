//! Test-only Unix PTY automation, independent of the application runtime.
#![cfg(unix)]

mod harness;
pub mod keys;
mod launch;
mod pty;

pub use harness::PtyHarness;
pub use keys::Key;
pub use launch::IsolatedWorkspace;
pub use pty::PtySize;
