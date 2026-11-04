// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Write strings to a strings.rs file that will be compiled into the binary.

use std::fmt::Write;
use std::fs;
use std::path::PathBuf;

use inflections::Inflect;

use crate::extract::Module;
use crate::extract::Translation;
use crate::extract::VariableKind;
use crate::gather::TranslationsByModule;

pub fn write_strings(map: &TranslationsByModule, modules: &[Module], out_fn: &str) {
    let mut buf = String::new();

    // module name->ftl text
    write_module_map(map, &mut buf);
    // methods to generate messages
    write_methods(modules, &mut buf);

    let dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let path = dir.join(out_fn);
    fs::write(path, buf).unwrap();
}

fn write_methods(modules: &[Module], buf: &mut String) {
    buf.push_str(
        r#"
#[allow(unused_imports)]
use crate::{I18n,Number};
#[allow(unused_imports)]
use fluent::{FluentValue, FluentArgs};
use std::borrow::Cow;

"#,
    );
    buf.push_str("impl I18n {\n");
    for module in modules {
        for translation in &module.translations {
            let func = translation.key.to_snake_case();
            let key = &translation.key;
            let doc = translation.text.replace('\n', " ");
            let in_args;
            let out_args;
            let var_build;
            if translation.variables.is_empty() {
                in_args = "".to_string();
                out_args = ", None".to_string();
                var_build = "".to_string();
            } else {
                in_args = build_in_args(translation);
                var_build = build_vars(translation);
                out_args = ", Some(args)".to_string();
            }

            writeln!(
                buf,
                r#"
    /// {doc}
    #[inline]
    pub fn {func}<'a>(&'a self{in_args}) -> Cow<'a, str> {{
{var_build}
        self.translate("{key}"{out_args})
    }}"#,
            )
            .unwrap();
        }
    }

    buf.push_str("}\n");
}

fn build_vars(translation: &Translation) -> String {
    if translation.variables.is_empty() {
        "let args = None;\n".into()
    } else {
        let mut buf = String::from(
            r#"
        let mut args = FluentArgs::new();
"#,
        );
        for v in &translation.variables {
            let fluent_name = &v.name;
            let rust_name = v.name.to_snake_case();
            let trailer = match v.kind {
                VariableKind::Any => "",
                VariableKind::Int | VariableKind::Float => ".round().into()",
                VariableKind::String => ".into()",
            };
            writeln!(
                buf,
                r#"        args.set("{fluent_name}", {rust_name}{trailer});"#,
            )
            .unwrap();
        }

        buf
    }
}

fn build_in_args(translation: &Translation) -> String {
    let v: Vec<_> = translation
        .variables
        .iter()
        .map(|var| {
            let kind = match var.kind {
                VariableKind::Int => "impl Number",
                VariableKind::Float => "impl Number",
                VariableKind::String => "impl Into<String>",
                // VariableKind::Any => "&str",
                _ => "impl Into<FluentValue<'a>>",
            };
            format!("{}: {}", var.name.to_snake_case(), kind)
        })
        .collect();
    format!(", {}", v.join(", "))
}

fn write_module_map(modules: &TranslationsByModule, buf: &mut String) {
    buf.push_str(
        "
pub(crate) const STRINGS: phf::Map<&str, &str> = phf::phf_map! {
",
    );

    for (module, contents) in modules {
        let escaped_contents = escape_unicode_control_chars(contents);
        writeln!(buf, r###"    "{module}" => r##"{escaped_contents}"##,"###).unwrap();
    }

    buf.push_str("};\n");
}

fn escape_unicode_control_chars(input: &str) -> String {
    use regex::Regex;

    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"[\u{202a}-\u{202e}\u{2066}-\u{2069}]").unwrap());

    re.replace_all(input, |caps: &regex::Captures| {
        let c = caps.get(0).unwrap().as_str().chars().next().unwrap();
        format!("\\u{{{:04x}}}", c as u32)
    })
    .into_owned()
}
