// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use super::*;
use crate::prelude::*;

impl SqliteStorage {
    pub(crate) fn usn(&self) -> Result<Usn> {
        Ok(Usn(-1))
    }
}
