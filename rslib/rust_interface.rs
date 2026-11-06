// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::env;
use std::fmt::Write;
use std::path::PathBuf;

use anki_io::write_file_if_changed;
use anki_proto_gen::get_services;
use anki_proto_gen::Method;
use anki_proto_gen::Service;
use anyhow::Context;
use anyhow::Result;
use inflections::Inflect;
use prost_reflect::DescriptorPool;

/// Writes a trait per service, for the Backend or the Collection to implement,
/// and `Backend::run_method()`, which calls any method by its camelCase name.
pub fn write_rust_interface(pool: &DescriptorPool) -> Result<()> {
    let services = get_services(pool);
    let mut buf = String::new();
    buf.push_str("use crate::error::Result; use prost::Message;");
    for service in &services {
        render_trait(service, &mut buf);
    }
    render_run_method(&services, &mut buf);

    let buf = format_code(buf)?;
    let out_dir = env::var("OUT_DIR").unwrap();
    let path = PathBuf::from(out_dir).join("backend.rs");
    write_file_if_changed(path, buf).context("write file")?;
    Ok(())
}

fn format_code(code: String) -> Result<String> {
    let syntax_tree = syn::parse_file(&code)?;
    Ok(prettyplease::unparse(&syntax_tree))
}

fn render_trait(service: &Service, buf: &mut String) {
    let name = &service.name;
    let self_kind = if service.on_backend {
        "&self"
    } else {
        "&mut self"
    };
    writeln!(buf, "pub trait {name} {{").unwrap();
    for method in &service.methods {
        let method_name = &method.name;
        let input_with_label = method.get_input_arg_with_label();
        let output_type = method.get_output_type();
        writeln!(
            buf,
            "fn {method_name}({self_kind}, {input_with_label}) -> Result<{output_type}>;"
        )
        .unwrap();
    }
    buf.push('}');
}

fn render_run_method(services: &[Service], buf: &mut String) {
    let methods = || {
        services
            .iter()
            .flat_map(|s| s.methods.iter().map(move |m| (s, m)))
    };
    buf.push_str(
        "impl crate::backend::Backend {
        /// Every method [Self::run_method] knows, by camelCase name.
        pub const METHODS: &[&str] = &[",
    );
    for (_, method) in methods() {
        write!(buf, "{:?},", method.name.to_camel_case()).unwrap();
    }
    buf.push_str(
        "];

        /// Call a method by its camelCase name, with protobuf-encoded input,
        /// returning protobuf-encoded output. None if there's no such method.
        pub fn run_method(&self, method: &str, input: &[u8]) -> Option<Result<Vec<u8>>> {
            let run: fn(&Self, &[u8]) -> Result<Vec<u8>> = match method {",
    );
    for (service, method) in methods() {
        render_match_arm(service, method, buf);
    }
    buf.push_str(
        "_ => return None,
            };
            Some(run(self, input))
        }
    }",
    );
}

fn render_match_arm(service: &Service, method: &Method, buf: &mut String) {
    let camel = method.name.to_camel_case();
    let decode_input =
        method.text_if_input_not_empty(|kind| format!("let input = {kind}::decode(input)?;"));
    let input = method.text_if_input_not_empty(|_| "input".into());
    let call = format!(
        "crate::services::{}::{}(target, {input})",
        service.name, method.name
    );
    let body = if method.output().is_none() {
        format!("{decode_input} {call}?; Ok(Vec::new())")
    } else {
        format!("{decode_input} Ok({call}?.encode_to_vec())")
    };
    let input_param = if method.input().is_some() {
        "input"
    } else {
        "_input"
    };
    if service.on_backend {
        writeln!(buf, "{camel:?} => |target, {input_param}| {{ {body} }},").unwrap();
    } else {
        writeln!(
            buf,
            "{camel:?} => |backend, {input_param}| backend.with_col(|target| {{ {body} }}),"
        )
        .unwrap();
    }
}

trait MethodHelpers {
    fn input_type(&self) -> Option<String>;
    fn output_type(&self) -> Option<String>;
    fn text_if_input_not_empty(&self, text: impl Fn(&String) -> String) -> String;
    fn get_input_arg_with_label(&self) -> String;
    fn get_output_type(&self) -> String;
}

impl MethodHelpers for Method {
    fn input_type(&self) -> Option<String> {
        self.input().map(|t| rust_type(t.full_name()))
    }

    fn output_type(&self) -> Option<String> {
        self.output().map(|t| rust_type(t.full_name()))
    }
    /// No text if generic::Empty
    fn text_if_input_not_empty(&self, text: impl Fn(&String) -> String) -> String {
        self.input_type().as_ref().map(text).unwrap_or_default()
    }

    /// No text if generic::Empty
    fn get_input_arg_with_label(&self) -> String {
        self.input_type()
            .as_ref()
            .map(|t| format!("input: {t}"))
            .unwrap_or_default()
    }

    /// () if generic::Empty
    fn get_output_type(&self) -> String {
        self.output_type().as_deref().unwrap_or("()").into()
    }
}

fn rust_type(name: &str) -> String {
    let Some((head, tail)) = name.rsplit_once('.') else {
        panic!()
    };
    format!(
        "{}::{}",
        head.to_snake_case()
            .replace('.', "::")
            .replace("anki::", "anki_proto::"),
        tail
    )
}
