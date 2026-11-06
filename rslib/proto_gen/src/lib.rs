// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Helpers for generating the RPC interface code.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::LazyLock;

use anki_io::read_to_string;
use anki_io::write_file_if_changed;
use anki_io::ToUtf8Path;
use anyhow::Result;
use camino::Utf8Path;
use inflections::Inflect;
use itertools::Itertools;
use prost_reflect::DescriptorPool;
use prost_reflect::MessageDescriptor;
use prost_reflect::MethodDescriptor;
use regex::Captures;
use regex::Regex;
use walkdir::WalkDir;

/// The RPC services. A service whose name starts with `Backend` is implemented
/// on the Backend, and works without an open collection; any other is
/// implemented on the Collection. Methods are called by name, so each name
/// must be unique across all services.
pub fn get_services(pool: &DescriptorPool) -> Vec<Service> {
    let comments = MethodComments::from_pool(pool);
    let services: Vec<Service> = pool
        .services()
        .map(|service| Service {
            name: service.name().to_string(),
            on_backend: service.name().starts_with("Backend"),
            methods: service
                .methods()
                .map(|method| Method {
                    name: method.name().to_snake_case(),
                    comments: comments.get_for_method(&method),
                    proto: method,
                })
                .collect(),
        })
        .collect();
    let duplicates = services
        .iter()
        .flat_map(|service| &service.methods)
        .map(|method| &method.name)
        .duplicates()
        .collect_vec();
    assert!(
        duplicates.is_empty(),
        "methods defined twice: {duplicates:?}"
    );
    services
}

#[derive(Debug)]
pub struct Service {
    pub name: String,
    /// Implemented on the Backend rather than the Collection.
    pub on_backend: bool,
    pub methods: Vec<Method>,
}

#[derive(Debug, Clone)]
pub struct Method {
    /// snake_case
    pub name: String,
    pub comments: Option<String>,
    pub proto: MethodDescriptor,
}

impl Method {
    /// The input type, if not empty.
    pub fn input(&self) -> Option<MessageDescriptor> {
        msg_if_not_empty(self.proto.input())
    }

    /// The output type, if not empty.
    pub fn output(&self) -> Option<MessageDescriptor> {
        msg_if_not_empty(self.proto.output())
    }
}

fn msg_if_not_empty(msg: MessageDescriptor) -> Option<MessageDescriptor> {
    if msg.full_name() == "anki.generic.Empty" {
        None
    } else {
        Some(msg)
    }
}

#[derive(Debug)]
struct MethodComments<'a> {
    // package name -> method path -> comment
    by_package_and_path: HashMap<&'a str, HashMap<Vec<i32>, String>>,
}

impl<'a> MethodComments<'a> {
    pub fn from_pool(pool: &'a DescriptorPool) -> MethodComments<'a> {
        let mut by_package_and_path = HashMap::new();
        for file in pool.file_descriptor_protos() {
            let path_map = file
                .source_code_info
                .as_ref()
                .unwrap()
                .location
                .iter()
                .map(|l| (l.path.clone(), l.leading_comments().trim().to_string()))
                .collect();
            by_package_and_path.insert(file.package(), path_map);
        }
        Self {
            by_package_and_path,
        }
    }

    pub fn get_for_method(&self, method: &MethodDescriptor) -> Option<String> {
        self.by_package_and_path
            .get(method.parent_file().package_name())
            .and_then(|by_path| by_path.get(method.path()))
            .and_then(|s| if s.is_empty() { None } else { Some(s.into()) })
    }
}

pub fn add_must_use_annotations<P, E>(
    out_dir: &PathBuf,
    should_process_path: P,
    is_empty: E,
) -> Result<()>
where
    P: Fn(&Utf8Path) -> bool,
    E: Fn(&Utf8Path, &str) -> bool,
{
    for file in WalkDir::new(out_dir).into_iter() {
        let file = file?;
        let path = file.path().utf8()?;
        if path.file_name().unwrap().ends_with(".rs") && should_process_path(path) {
            add_must_use_annotations_to_file(path, &is_empty)?;
        }
    }
    Ok(())
}

pub fn add_must_use_annotations_to_file<E>(path: &Utf8Path, is_empty: E) -> Result<()>
where
    E: Fn(&Utf8Path, &str) -> bool,
{
    static MESSAGE_OR_ENUM_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"pub (struct|enum) ([[:alnum:]]+?)\s").unwrap());
    let contents = read_to_string(path)?;
    let contents = MESSAGE_OR_ENUM_RE.replace_all(&contents, |caps: &Captures| {
        let is_enum = caps.get(1).unwrap().as_str() == "enum";
        let name = caps.get(2).unwrap().as_str();
        if is_enum || !is_empty(path, name) {
            format!("#[must_use]\n{}", caps.get(0).unwrap().as_str())
        } else {
            caps.get(0).unwrap().as_str().to_string()
        }
    });
    write_file_if_changed(path, contents.as_ref())?;
    Ok(())
}

/// Given a generated prost filename and a struct name, try to determine whether
/// the message has 0 fields.
///
/// This is unfortunately rather circuitous, as Prost doesn't allow us to easily
/// alter the code generation with access to the associated proto descriptor. So
/// we need to infer the full proto path based on the filename and the Rust type
/// name, which we can only do for top-level elements. For any nested messages
/// we can't find, we assume they must be used.
pub fn determine_if_message_is_empty(pool: &DescriptorPool, path: &Utf8Path, name: &str) -> bool {
    let package = path.file_stem().unwrap();
    let full_name = format!("{package}.{name}");
    if let Some(msg) = pool.get_message_by_name(&full_name) {
        msg.fields().count() == 0
    } else {
        false
    }
}

/// - When building via a local checkout, the path defined in .cargo/config
/// - When building via cargo install or a third-party crate,
///   OUT_DIR/../../anki_descriptors.bin (so it can be seen by the rslib crate)
pub fn descriptors_path() -> PathBuf {
    if let Ok(path) = env::var("DESCRIPTORS_BIN") {
        PathBuf::from(path)
    } else {
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("../../anki_descriptors.bin")
    }
}
