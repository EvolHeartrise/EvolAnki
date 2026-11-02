// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

pub(crate) mod card;
mod collection_timestamps;
mod config;
mod dbcheck;
mod deck;
mod deckconfig;
mod note;
mod notetype;
mod revlog;
mod sqlite;
mod sync;
mod tag;
mod upgrades;

use std::fmt::Write;

/// What a card's queue is once the collection has been opened, as an SQL
/// expression. Queues -2 and -3 held buried cards, which no longer exist.
/// Other software can still leave cards there, and a card keeps what it needs
/// in its other columns, so opening a collection returns them to normal. This
/// has to agree with `Card::restore_queue_from_type()`.
pub const QUEUE_ONCE_OPENED_SQL: &str = "CASE
    WHEN queue NOT IN (-2, -3) THEN queue
    WHEN type = 0 THEN 0
    WHEN type = 2 THEN 2
    WHEN due > 1000000000 THEN 1
    ELSE 3
  END";

pub(crate) use deck::all_decks;
pub(crate) use deckconfig::all_deck_config_unchecked;
pub use sqlite::copy_database;
pub use sqlite::verify_copied_collection;
pub(crate) use sqlite::ProcessTextFlags;
pub(crate) use sqlite::SqliteStorage;

/// Write a list of IDs as '(x,y,...)' into the provided string.
pub fn ids_to_string<D, I>(buf: &mut String, ids: I)
where
    D: std::fmt::Display,
    I: IntoIterator<Item = D>,
{
    buf.push('(');
    write_comma_separated_ids(buf, ids);
    buf.push(')');
}

/// Write a list of Ids as 'x,y,...' into the provided string.
pub(crate) fn write_comma_separated_ids<D, I>(buf: &mut String, ids: I)
where
    D: std::fmt::Display,
    I: IntoIterator<Item = D>,
{
    let mut trailing_sep = false;
    for id in ids {
        write!(buf, "{id},").unwrap();
        trailing_sep = true;
    }
    if trailing_sep {
        buf.pop();
    }
}

pub(crate) fn comma_separated_ids<T>(ids: &[T]) -> String
where
    T: std::fmt::Display,
{
    let mut buf = String::new();
    write_comma_separated_ids(&mut buf, ids);

    buf
}

#[cfg(test)]
mod test {
    use super::ids_to_string;

    #[test]
    fn ids_string() {
        let mut s = String::new();
        ids_to_string(&mut s, [0; 0]);
        assert_eq!(s, "()");
        s.clear();
        ids_to_string(&mut s, [7]);
        assert_eq!(s, "(7)");
        s.clear();
        ids_to_string(&mut s, [7, 6]);
        assert_eq!(s, "(7,6)");
        s.clear();
        ids_to_string(&mut s, [7, 6, 5]);
        assert_eq!(s, "(7,6,5)");
        s.clear();
    }
}
