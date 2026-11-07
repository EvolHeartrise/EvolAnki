// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// DeckConfig inside deck_config.proto
#![allow(clippy::module_inception)]

mod generic_helpers;

macro_rules! protobuf {
    ($ident:ident, $name:literal) => {
        pub mod $ident {
            include!(concat!(env!("OUT_DIR"), "/anki.", $name, ".rs"));
        }
    };
}

protobuf!(card_rendering, "card_rendering");
protobuf!(cards, "cards");
protobuf!(collection, "collection");
protobuf!(config, "config");
protobuf!(deck_config, "deck_config");
protobuf!(decks, "decks");
protobuf!(generic, "generic");
protobuf!(import_export, "import_export");
protobuf!(media, "media");
protobuf!(notes, "notes");
protobuf!(notetypes, "notetypes");
protobuf!(scheduler, "scheduler");
protobuf!(search, "search");
protobuf!(stats, "stats");
protobuf!(tags, "tags");
