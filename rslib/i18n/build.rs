// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

mod check;
mod extract;
mod gather;
mod typescript;
mod write_strings;

use anyhow::Result;
use check::check;
use extract::get_modules;
use gather::get_ftl_data;
use write_strings::write_strings;

fn main() -> Result<()> {
    let map = get_ftl_data();
    check(&map);
    let modules = get_modules(&map);
    write_strings(&map, &modules, "strings.rs");
    typescript::write_ts_interface(&modules)?;
    Ok(())
}
