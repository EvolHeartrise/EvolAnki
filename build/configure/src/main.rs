// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

mod rust;
mod web;

use anyhow::Result;
use ninja_gen::glob;
use ninja_gen::inputs;
use ninja_gen::protobuf::check_proto;
use ninja_gen::protobuf::setup_protoc;
use ninja_gen::Build;
use rust::build_rust;
use rust::check_rust;
use web::build_and_check_web;
use web::check_sql;

fn main() -> Result<()> {
    let mut build = Build::new()?;
    let build = &mut build;

    setup_protoc(build)?;
    check_proto(build, inputs![glob!["proto/**/*.proto"]])?;

    build_rust(build)?;
    build_and_check_web(build)?;

    check_rust(build)?;

    check_sql(build)?;

    build.trailing_text = "default web\n".into();

    build.write_build_file()?;

    Ok(())
}
