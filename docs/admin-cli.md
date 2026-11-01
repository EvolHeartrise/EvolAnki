# evolanki-admin

For agents. This app has no editor, browser or import page: you manage the owner's cards
with this tool, which is JSON in and JSON out. It does what the backend does, so undo,
FSRS state and deck counts stay right, and no command writes to the database itself.

Installed at `~/.local/share/evolanki/bin/evolanki-admin`, by `tools/install-local`. In a
build tree it's `out/rust/release/evolanki-admin`.

## Running a command

```
evolanki-admin run --collection PATH COMMAND 'JSON'
```

- `--collection` can be left out if `EVOLANKI_COLLECTION` is set. The owner's is
  `~/.local/share/evolanki/collection.anki2`. A path that doesn't exist is an error, so a
  typo doesn't make a new collection.
- The JSON is one object. Pass `-` to read it from stdin, which suits long field values,
  or nothing for `{}`.
- `evolanki-admin commands` lists every command and its arguments. That list is the
  reference; this page is the reasoning.

Exactly one JSON line comes back on stdout. The exit code is 0 if `ok` is true.

```
$ evolanki-admin run note.add '{"notetype":"Basic","deck":"Default","fields":{"Front":"Q","Back":"A"}}'
{"command":"note.add","ok":true,"result":{"card_ids":[1793556382166],"id":1793556382166},"via":"server"}

$ evolanki-admin run note.show '{}'
{"command":"note.show","error":"id must be an integer","ok":false,"via":"server"}
```

- `via`: `server` if it went through the running server, `direct` if the tool opened the
  collection itself.
- `backup`: there if a backup was taken first, with its path.

## Server or not

The collection is locked to whichever process has it open, so while `evolanki-server` runs
nothing else can open it. The server therefore listens on a Unix socket beside the
collection, `collection.admin.sock`, and the tool sends the command there. With no server,
the tool opens the collection, runs the command, and closes it. You needn't care which,
except that:

- **Undo only spans a running server's life.** `undo` undoes the last operation of any
  kind in that server's memory, including a review the owner just did. Check `undo.status`
  first. With no server each command is its own process, so there is nothing to undo, and
  your recovery is a backup.
- An error mentioning that it couldn't open the collection means something else has it:
  most likely desktop Anki, or another `evolanki-admin` run. Try again shortly.
- `check.database` and importing a `.colpkg` clear the undo queue.

The socket has mode 0600, as does everything made private by the kernel's file permissions;
the server also checks that the connecting user is its own. There's no other way in, and
no command here is reachable over HTTP or TCP. Anyone who is this user can do all of this
anyway, by opening the files.

Paths in arguments are read by the server, as the same user. Relative ones are made
absolute first, from where you ran the tool.

## Searching and choosing what to act on

`search.notes` and `search.cards` take [Anki's search
syntax](https://docs.ankiweb.net/searching.html): `deck:Name`, `tag:x`, `is:due`,
`is:suspended`, `front:text`, `nid:123`, `cid:123`, `prop:ivl>30`, `"note:Basic"`, and so
on. Bad syntax gives an error. Quote names with spaces.

Commands that take cards or notes take either `"ids":[...]` or `"search":"..."`, not both.
An empty search, and one that matches nothing, are errors rather than meaning everything
or doing nothing. Before changing anything by search, run the same search to see how many
it matches.

`tag.*` commands and `note.delete` take **note** ids, and `card.*` and `deck.move-cards`
take **card** ids. Notes have one or more cards. `note.show` gives you the cards of a note.

## Notes and media

Field values are **HTML**, and shown as they are. Escape `<` and `&` in plain text. A
`[sound:name.mp3]` plays; `<img src="name.png">` shows an image. Don't bring scripts: they
never run in this app, and neither do `onclick` and the like. Images and media from other
sites don't load either, so put them in media.

Add a file first, and use the name that comes back, as it can differ:

```
$ evolanki-admin run media.add '{"path":"/tmp/word.mp3"}'
{"ok":true,"result":{"image_tag":"<img src=\"word.mp3\">","name":"word.mp3","renamed":false,"sound_tag":"[sound:word.mp3]"},...}
```

- If that name is taken by identical content, you get the same name. If by different
  content, a new name. That's `renamed`. Existing cards aren't affected.
- `media.replace` overwrites a file under its name, for every card using it. The old one is
  kept in `backups/media-replaced`. Browsers may keep showing the old file for an hour.
- Prefer mp3 or m4a, and png, jpeg or webp. iPhones before iOS 18.4 can't play `.ogg` or
  `.opus`.
- Undo doesn't cover media. Unused media isn't removed; `check.media` lists it.

`note.add` refuses a duplicate first field unless given `allow_duplicate`, and an empty
first field always. It can make several cards, as the note type says.

`note.edit` changes only the fields you name. `tags`, if given, replaces all of them; use
`tag.add` and `tag.remove` otherwise. Scheduling isn't touched, and a card whose template no
longer applies is left for `cards.empty`.

## Scheduling

You rarely need these. FSRS decides when cards are due, and the owner reviews.

- `card.suspend` and `card.bury`, and their opposites. `card.unsuspend` and `card.unbury`
  only change cards in that state, and say how many they changed.
- `card.set-due`: `"days":"0"` is today, `"1-7"` a random day in that range, and `"3!"` also
  sets the interval to 3. It logs an entry in the card's history, as desktop Anki does.
- A card suspended or buried while it's on the owner's screen refuses the answer they
  then give: they see a notice and the next card. A note edit doesn't get in the way.

## Decks, tags and note types

- Deck arguments are a name (`A::B`, whose parents are created as needed) or an id. Same
  for note types.
- `deck.delete` also **deletes the cards in the deck and its children**, with their review
  history. Move them first if you don't mean that. With a filtered deck, cards go home
  instead.
- `tag.rename` also renames children of the tag.
- `notetype.add-field` adds it at the end, empty on all existing notes.
  `notetype.edit-template` is refused if the template is invalid, such as naming a field
  that doesn't exist. Check with `card.show` on a card that uses it.

## Filtered decks and custom study

`filtered.create`, with an order from `commands`. The cards go on being due as normal in
that deck, and with `reschedule` false, answering them doesn't change their schedule.
`filtered.empty` returns them; `filtered.rebuild` fills it again; `deck.delete` returns them
and removes the deck. There's no page to make these, but the reviewer studies them.

`custom-study` does what the desktop dialog does. `new_limit_delta` and
`review_limit_delta` raise today's limits on that deck; the others fill a filtered deck
called `Custom Study Session`, replacing what it held.

## Checking

- `check.database` repairs what it can, and says what it did. Takes a backup first.
- `check.media` reports missing and unused files. It doesn't remove any, and doesn't change
  notes.
- `cards.empty` reports cards with a blank front. `"apply":true` deletes them, after a
  backup.
- `stats.summary` is the place to start: how many of everything, what's due, how many have
  memory states.

## Importing

One way, into this collection. `import` takes `.apkg` or `.colpkg` by extension.

- `.apkg`: adds to the collection. By default it brings notes and cards as new, without
  scheduling or deck options, and keeps your notes if theirs aren't newer. `with_scheduling`
  brings reviews too, and any such cards lacking a memory state get one. `found_notes`,
  `new`, `updated` and others say what happened.
- `.colpkg`: **replaces everything, media excepted**. Requires `"replace_collection":true`.
  Pages get an error while that runs, for a few seconds, and work again afterwards. If the
  owner has been reviewing, those reviews are only in the backup that it takes first.

## Backups

Taken automatically, and reported as `backup`, for:

- destructive commands, always: `note.delete`, `deck.delete`, `tag.rename`, `media.replace`,
  `notetype.add-field`, `notetype.edit-template`, `check.database`, `import`, and
  `cards.empty` when it applies;
- bulk ones: any command that changes things using `search`, or more than 20 ids.

If the backup fails, the command doesn't run. Otherwise, `backup.now`. They count towards
the [retention limits](web-server.md#backups) of 48 recent ones and one a day for 30 days,
so don't run many hundreds of bulk commands in a row, or you'll push out the last good one:
fold them into one call. To restore, follow the same section.

Media isn't in those backups. See there.

## Concurrency

The owner may be reviewing as you work. Commands queue for the collection with their
requests, so it's safe, but a long one holds them up: `import`, `check.database` and large
searches may take a few seconds. A review shown in a browser stays on screen until
answered; the next one reflects your change. The server's backups are taken at quiet
moments, and need nothing from you.

## Converting a desktop profile

```
evolanki-admin convert --source "~/.local/share/Anki2/User 1/collection.anki2" \
    --target ~/.local/share/evolanki/collection.anki2
evolanki-admin validate --target ~/.local/share/evolanki/collection.anki2 \
    --report ~/.local/share/evolanki/conversion-report.md --work-dir /tmp
```

`convert` refuses if desktop Anki is running, something has the profile's files open, or
anything exists at the target, as it never overwrites. The source is only read: the database
through SQLite's backup API on a read-only, immutable URI, and media with `cp`. It hashes
them before and after, and fails if they changed. A snapshot of the source stays in
`conversion/`, and `validate` compares against that. Run `validate` with no server on the
target; it runs anything touching the backend on a throwaway copy, in the work folder, which
needs room for all the media. Run it again whenever you like, though expect the report to
show what's changed since then.

## What's not here

Deleting media, removing a field or a template, repositioning new cards, changing deck
options, running the optimizer: the owner uses the deck options page for the last two.
