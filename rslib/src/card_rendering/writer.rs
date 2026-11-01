// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::fmt::Write as _;

use super::CardNodes;
use super::Directive;
use super::Node;
use crate::text::decode_entities;

impl CardNodes<'_> {
    pub(super) fn write_without_av_tags(&self) -> String {
        AvStripper::new().write(self)
    }

    pub(super) fn write_and_extract_av_tags(
        &self,
        question_side: bool,
    ) -> (String, Vec<anki_proto::card_rendering::AvTag>) {
        let mut extractor = AvExtractor::new(question_side);
        (extractor.write(self), extractor.tags)
    }

    pub(super) fn write_with_pretty_av_tags(&self) -> String {
        AvPrettifier::new().write(self)
    }
}

trait Write {
    fn write<'iter, 'nodes: 'iter, T>(&mut self, nodes: T) -> String
    where
        T: IntoIterator<Item = &'iter Node<'nodes>>,
    {
        let mut buf = String::new();
        for node in nodes {
            match node {
                Node::Text(s) => self.write_text(&mut buf, s),
                Node::Sound(r) => self.write_sound(&mut buf, r),
                Node::Directive(directive) => self.write_directive(&mut buf, directive),
            };
        }
        buf
    }

    fn write_text(&mut self, buf: &mut String, txt: &str) {
        buf.push_str(txt);
    }

    fn write_sound(&mut self, buf: &mut String, resource: &str) {
        write!(buf, "[sound:{resource}]").unwrap();
    }

    fn write_directive(&mut self, buf: &mut String, directive: &Directive) {
        // Text-to-speech is gone. A deck from elsewhere can still have these tags,
        // whose text was only ever spoken, so nothing shows in their place.
        if directive.name == "tts" {
            return;
        }
        write!(buf, "[anki:{}", directive.name).unwrap();
        for (key, val) in &directive.options {
            self.write_directive_option(buf, key, val);
        }
        buf.push(']');
        self.write_directive_content(buf, directive.content);
        write!(buf, "[/anki:{}]", directive.name).unwrap();
    }

    fn write_directive_option(&mut self, buf: &mut String, key: &str, val: &str) {
        if val.contains([']', ' ', '\t', '\r', '\n']) {
            write!(buf, " {key}=\"{val}\"").unwrap();
        } else {
            write!(buf, " {key}={val}").unwrap();
        }
    }

    fn write_directive_content(&mut self, buf: &mut String, content: &str) {
        buf.push_str(content);
    }
}

struct AvStripper;

impl AvStripper {
    fn new() -> Self {
        Self {}
    }
}

impl Write for AvStripper {
    fn write_sound(&mut self, _buf: &mut String, _resource: &str) {}
}

struct AvExtractor {
    side: char,
    tags: Vec<anki_proto::card_rendering::AvTag>,
}

impl AvExtractor {
    fn new(question_side: bool) -> Self {
        Self {
            side: if question_side { 'q' } else { 'a' },
            tags: vec![],
        }
    }

    fn write_play_tag(&self, buf: &mut String) {
        write!(buf, "[anki:play:{}:{}]", self.side, self.tags.len()).unwrap();
    }
}

impl Write for AvExtractor {
    fn write_sound(&mut self, buf: &mut String, resource: &str) {
        self.write_play_tag(buf);
        self.tags.push(anki_proto::card_rendering::AvTag {
            sound: decode_entities(resource).into(),
        });
    }
}

struct AvPrettifier;

impl AvPrettifier {
    fn new() -> Self {
        Self {}
    }
}

impl Write for AvPrettifier {
    fn write_sound(&mut self, buf: &mut String, resource: &str) {
        write!(buf, "🔉{resource}🔉").unwrap();
    }
}

#[cfg(test)]
mod test {
    use super::*;

    struct Writer;
    impl Write for Writer {}
    impl Writer {
        fn new() -> Self {
            Self {}
        }
    }

    /// Parse input, write it out, and assert equality with input or separately
    /// passed output.
    macro_rules! roundtrip {
        ($input:expr) => {
            assert_eq!($input, Writer::new().write(&CardNodes::parse($input)));
        };
        ($input:expr, $output:expr) => {
            assert_eq!(Writer::new().write(&CardNodes::parse($input)), $output);
        };
    }

    #[test]
    fn writing() {
        roundtrip!("foo");
        roundtrip!("[sound:foo]");
        roundtrip!("[anki:foo bar=baz]spam[/anki:foo]");

        // normalizing (not currently exposed)
        roundtrip!(
            "[anki:foo\nbar=baz ][/anki:foo]",
            "[anki:foo bar=baz][/anki:foo]"
        );
    }
}
