// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Generates a (camelCaseName, service index, method index) table for every
//! backend RPC, using the same index logic and names as the generated
//! TypeScript client. What may actually be called is decided by the allow-list
//! in src/rpc.rs.

use std::env;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;

use anki_proto_gen::descriptors_path;
use anki_proto_gen::get_services;
use anyhow::Result;
use inflections::Inflect;
use prost_reflect::DescriptorPool;

fn main() -> Result<()> {
    let descriptors = descriptors_path();
    println!("cargo:rerun-if-changed={}", descriptors.display());
    let pool = DescriptorPool::decode(fs::read(descriptors)?.as_ref())?;
    let (_, services) = get_services(&pool);

    let mut out = String::from("pub const ALL_METHODS: &[(&str, u32, u32)] = &[\n");
    for service in &services {
        for method in service.all_methods() {
            writeln!(
                out,
                "    ({:?}, {}, {}),",
                method.name.to_camel_case(),
                service.index,
                method.index
            )?;
        }
    }
    out.push_str("];\n");
    fs::write(PathBuf::from(env::var("OUT_DIR")?).join("methods.rs"), out)?;
    Ok(())
}
