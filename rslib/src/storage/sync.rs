// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use super::*;
use crate::prelude::*;

impl SqliteStorage {
    pub(crate) fn usn(&self, server: bool) -> Result<Usn> {
        if server {
            Ok(Usn(self
                .db
                .prepare_cached("select usn from col")?
                .query_row([], |row| row.get(0))?))
        } else {
            Ok(Usn(-1))
        }
    }

    #[cfg(test)]
    pub(crate) fn increment_usn(&self) -> Result<()> {
        self.db
            .prepare_cached("update col set usn = usn + 1")?
            .execute([])?;
        Ok(())
    }
}
