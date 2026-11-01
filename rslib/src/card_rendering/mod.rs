// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;

mod parser;
pub(crate) mod service;
mod writer;

pub fn strip_av_tags<S: Into<String> + AsRef<str>>(txt: S) -> String {
    nodes_or_text_only(txt.as_ref())
        .map(|nodes| nodes.write_without_av_tags())
        .unwrap_or_else(|| txt.into())
}

pub fn extract_av_tags<S: Into<String> + AsRef<str>>(
    txt: S,
    question_side: bool,
) -> (String, Vec<anki_proto::card_rendering::AvTag>) {
    nodes_or_text_only(txt.as_ref())
        .map(|nodes| nodes.write_and_extract_av_tags(question_side))
        .unwrap_or_else(|| (txt.into(), vec![]))
}

pub fn prettify_av_tags<S: Into<String> + AsRef<str>>(txt: S) -> String {
    nodes_or_text_only(txt.as_ref())
        .map(|nodes| nodes.write_with_pretty_av_tags())
        .unwrap_or_else(|| txt.into())
}

/// Parse `txt` into [CardNodes] and return the result,
/// or [None] if it only contains text nodes.
fn nodes_or_text_only(txt: &str) -> Option<CardNodes<'_>> {
    let nodes = CardNodes::parse(txt);
    (!nodes.text_only).then_some(nodes)
}

#[derive(Debug, PartialEq)]
struct CardNodes<'a> {
    nodes: Vec<Node<'a>>,
    text_only: bool,
}

impl<'iter, 'nodes> IntoIterator for &'iter CardNodes<'nodes> {
    type Item = &'iter Node<'nodes>;
    type IntoIter = std::slice::Iter<'iter, Node<'nodes>>;

    fn into_iter(self) -> Self::IntoIter {
        self.nodes.iter()
    }
}

#[derive(Debug, PartialEq)]
enum Node<'a> {
    Text(&'a str),
    SoundOrVideo(&'a str),
    Directive(Directive<'a>),
}

#[derive(Debug, PartialEq, Eq)]
struct Directive<'a> {
    name: &'a str,
    content: &'a str,
    options: HashMap<&'a str, &'a str>,
}

#[cfg(feature = "bench")]
#[inline]
pub fn anki_directive_benchmark() {
    CardNodes::parse("[anki:foo bar=baz][/anki:foo]");
}

#[cfg(test)]
mod test {
    use super::*;

    /// Strip av tags and assert equality with input or separately passed
    /// output.
    macro_rules! assert_av_stripped {
        ($input:expr) => {
            assert_eq!($input, strip_av_tags($input));
        };
        ($input:expr, $output:expr) => {
            assert_eq!(strip_av_tags($input), $output);
        };
    }

    #[test]
    fn av_stripping() {
        assert_av_stripped!("foo [sound:bar] baz", "foo  baz");
        assert_av_stripped!("[anki:tts bar=baz]spam[/anki:tts]", "");
        assert_av_stripped!("[anki:foo bar=baz]spam[/anki:foo]");
    }

    #[test]
    fn av_extracting() {
        let (txt, tags) = extract_av_tags("foo [sound:bar.mp3] baz", true);
        assert_eq!(
            (txt.as_str(), tags),
            (
                "foo [anki:play:q:0] baz",
                vec![anki_proto::card_rendering::AvTag {
                    value: Some(anki_proto::card_rendering::av_tag::Value::SoundOrVideo(
                        "bar.mp3".to_string()
                    ))
                }],
            ),
        );
    }

    /// A deck made for other Anki clients can still hold these. They speak text
    /// and were never shown, so they leave nothing behind, whatever their options.
    #[test]
    fn tts_tags_are_dropped() {
        for tag in [
            "[anki:tts]foo[/anki:tts]",
            "[anki:tts lang=en_US voices=Bob speed=0.5]foo[/anki:tts]",
        ] {
            let input = format!("a{tag}b");
            assert_eq!(extract_av_tags(&input, true), ("ab".to_string(), vec![]));
            assert_eq!(strip_av_tags(input.as_str()), "ab");
            assert_eq!(prettify_av_tags(input.as_str()), "ab");
        }
    }
}
