// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der echte Browser (SPEC.md §2): `open::that`.

use crate::Browser;

/// Öffnet eine URL im Standardbrowser des Systems.
#[derive(Debug, Clone, Copy, Default)]
pub struct OeffneBrowser;

impl OeffneBrowser {
    pub fn neu() -> Self {
        Self
    }
}

impl Browser for OeffneBrowser {
    fn oeffnen(&self, url: &str) -> anyhow::Result<()> {
        open::that(url)?;
        Ok(())
    }
}
