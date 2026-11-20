//! Hands URLs to the system handler (SPEC §3.6.2).

use std::fmt::Debug;

/// Opens a URL outside the viewer.
pub trait Opener: Debug {
    /// Starts the handler without waiting for it; the error is shown to the user.
    fn open(&self, url: &str) -> Result<(), String>;
}

/// The system handler (`xdg-open` and friends) through the `open` crate.
#[derive(Debug, Default)]
pub struct SystemOpener;

impl Opener for SystemOpener {
    fn open(&self, _url: &str) -> Result<(), String> {
        Err("not implemented".into())
    }
}

/// Default of a new `App`: opens nothing, so tests never launch a browser.
#[derive(Debug)]
pub(super) struct Unavailable;

impl Opener for Unavailable {
    fn open(&self, _url: &str) -> Result<(), String> {
        Err("No opener available".into())
    }
}
