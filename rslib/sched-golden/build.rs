// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::env;
use std::fs;
use std::path::PathBuf;

use anki_proto_gen::descriptors_path;
use anyhow::Result;

fn main() -> Result<()> {
    let descriptors = descriptors_path();
    println!("cargo:rerun-if-changed={}", descriptors.display());
    fs::copy(
        &descriptors,
        PathBuf::from(env::var("OUT_DIR")?).join("descriptors.bin"),
    )?;
    // The getrandom crate and std look this symbol up with dlsym, which only
    // finds what the executable exports. See src/clock.rs.
    println!("cargo:rustc-link-arg-bins=-Wl,--export-dynamic-symbol=getrandom");
    Ok(())
}
