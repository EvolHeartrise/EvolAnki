// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

mod generated;

use std::borrow::Cow;
use std::sync::Arc;

use fluent::types::FluentNumber;
use fluent::FluentArgs;
use fluent::FluentResource;
use fluent::FluentValue;
use fluent_bundle::bundle::FluentBundle as FluentBundleOrig;
use serde::Serialize;
use unic_langid::LanguageIdentifier;

type FluentBundle<T> = FluentBundleOrig<T, intl_memoizer::concurrent::IntlLangMemoizer>;

pub use fluent::fluent_args as tr_args;

pub trait Number: Into<FluentNumber> {
    fn round(self) -> Self;
}
impl Number for i32 {
    #[inline]
    fn round(self) -> Self {
        self
    }
}
impl Number for i64 {
    #[inline]
    fn round(self) -> Self {
        self
    }
}
impl Number for u32 {
    #[inline]
    fn round(self) -> Self {
        self
    }
}
impl Number for f32 {
    // round to 2 decimal places
    #[inline]
    fn round(self) -> Self {
        (self * 100.0).round() / 100.0
    }
}
impl Number for u64 {
    #[inline]
    fn round(self) -> Self {
        self
    }
}
impl Number for usize {
    #[inline]
    fn round(self) -> Self {
        self
    }
}

/// Some sample text for testing purposes.
fn test_text() -> &'static str {
    "
valid-key = a valid key
two-args-key = two args: {$one} and {$two}
plural = You have {$hats ->
    [one]   1 hat
   *[other] {$hats} hats
}.
"
}

fn all_strings() -> String {
    let mut text: String = generated::STRINGS.values().copied().collect();
    if cfg!(test) {
        text += test_text();
    }
    text
}

fn english() -> LanguageIdentifier {
    "en-US".parse().unwrap()
}

#[derive(Clone)]
pub struct I18n {
    bundle: Arc<FluentBundle<FluentResource>>,
}

impl I18n {
    /// The strings with Unicode isolation characters around their arguments,
    /// as the web pages get them.
    pub fn new() -> Self {
        Self::with_isolation(!cfg!(test))
    }

    /// The strings without isolation characters, for text that is stored or
    /// compared.
    pub fn template_only() -> Self {
        Self::with_isolation(false)
    }

    fn with_isolation(isolating: bool) -> Self {
        let res = FluentResource::try_new(all_strings()).expect("ftl checked at build time");
        let mut bundle: FluentBundle<FluentResource> =
            FluentBundle::new_concurrent(vec![english()]);
        bundle.add_resource(res).expect("ftl checked at build time");
        bundle.set_formatter(Some(format_number));
        bundle.set_use_isolating(isolating);
        Self {
            bundle: Arc::new(bundle),
        }
    }

    fn translate<'a>(&'a self, key: &str, args: Option<FluentArgs>) -> Cow<'a, str> {
        let Some(pat) = self.bundle.get_message(key).and_then(|msg| msg.value()) else {
            // return the key name if it was missing
            return key.to_string().into();
        };
        let mut errs = vec![];
        let out = self.bundle.format_pattern(pat, args.as_ref(), &mut errs);
        if !errs.is_empty() {
            println!("Error(s) in translation '{key}': {errs:?}");
        }
        // clone so we can discard args
        out.to_string().into()
    }

    /// Return the text of the requested modules (all if empty), for use with
    /// the JS Fluent implementation.
    pub fn resources_for_js(&self, desired_modules: &[String]) -> ResourcesForJavascript {
        let text = if desired_modules.is_empty() {
            generated::STRINGS.values().copied().collect()
        } else {
            desired_modules
                .iter()
                .filter_map(|module| generated::STRINGS.get(module.as_str()).copied())
                .collect()
        };
        ResourcesForJavascript {
            langs: vec![english().to_string()],
            resources: vec![text],
        }
    }
}

impl Default for I18n {
    fn default() -> Self {
        Self::new()
    }
}

/// Numbers are shown with at most two decimal places, and a period as the
/// decimal separator.
fn format_number(
    val: &FluentValue,
    _intl: &intl_memoizer::concurrent::IntlLangMemoizer,
) -> Option<String> {
    match val {
        FluentValue::Number(num) => {
            // create a string with desired maximum digits
            let max_frac_digits = 2;
            let with_max_precision = format!(
                "{number:.precision$}",
                number = num.value,
                precision = max_frac_digits
            );

            // remove any excess trailing zeros
            let mut val: Cow<str> = with_max_precision.trim_end_matches('0').into();

            // adding back any required to meet minimum_fraction_digits
            if let Some(minfd) = num.options.minimum_fraction_digits {
                let pos = val.find('.').expect("expected . in formatted string");
                let frac_num = val.len() - pos - 1;
                let zeros_needed = minfd - frac_num;
                if zeros_needed > 0 {
                    val = format!("{}{}", val, "0".repeat(zeros_needed)).into();
                }
            }

            // lop off any trailing '.'
            Some(val.trim_end_matches('.').to_string())
        }
        _ => None,
    }
}

#[derive(Serialize)]
pub struct ResourcesForJavascript {
    langs: Vec<String>,
    resources: Vec<String>,
}

pub fn without_unicode_isolation(s: &str) -> String {
    s.replace(['\u{2068}', '\u{2069}'], "")
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn decimal_rounding() {
        let tr = I18n::new();

        assert_eq!(tr.browsing_cards_deleted(1.001), "1 card deleted.");
        assert_eq!(tr.browsing_cards_deleted(1.01), "1.01 cards deleted.");
    }

    #[test]
    fn i18n() {
        let tr = I18n::new();
        assert_eq!(tr.translate("valid-key", None), "a valid key");
        assert_eq!(tr.translate("invalid-key", None), "invalid-key");

        assert_eq!(
            tr.translate("two-args-key", Some(tr_args!["one"=>1.1, "two"=>"2"])),
            "two args: 1.1 and 2"
        );

        assert_eq!(
            tr.translate("plural", Some(tr_args!["hats"=>1.0])),
            "You have 1 hat."
        );
        assert_eq!(
            tr.translate("plural", Some(tr_args!["hats"=>1.1])),
            "You have 1.1 hats."
        );
        assert_eq!(
            tr.translate("plural", Some(tr_args!["hats"=>3])),
            "You have 3 hats."
        );
    }
}
