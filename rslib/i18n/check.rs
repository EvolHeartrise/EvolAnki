// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Check the .ftl files at build time to ensure we don't get runtime load
//! failures.

use fluent::FluentBundle;
use fluent::FluentResource;
use unic_langid::LanguageIdentifier;

use super::gather::TranslationsByModule;

pub fn check(modules: &TranslationsByModule) {
    let lang_id: LanguageIdentifier = "en-US".parse().unwrap();
    for (module, content) in modules {
        let resource = FluentResource::try_new(content.clone()).unwrap_or_else(|e| {
            panic!("{content}\nUnable to parse {module}: {e:?}");
        });
        let mut bundle: FluentBundle<FluentResource> = FluentBundle::new(vec![lang_id.clone()]);
        bundle.add_resource(resource).unwrap_or_else(|e| {
            panic!("{content}\nUnable to bundle - duplicate key? {module}: {e:?}");
        });
    }
}
