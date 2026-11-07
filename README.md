# EvolAnki

A slim, FSRS-only spaced repetition web app for one person, served over the local
network, with no login, by `evolanki-server`. It is a modified version of
[Anki](https://github.com/ankitects/anki), and isn't affiliated with Anki or its authors.

## Getting started

```
tools/run-web
```

That needs Rust, Node 22 or 24, and [just](https://just.systems). It builds the pages
and server, then serves a demo collection, and prints the addresses to open it at. Use
`tools/run-web --release path/to/col.anki2` for real. Anyone on your network can reach
it, so read [what that means](./docs/web-server.md#no-login-what-that-means). See
[the server docs](./docs/web-server.md) and [Development](./docs/development.md).

## Changes from Anki

As the GNU AGPL requires, this notes that this is a modified version of Anki. Compared to
upstream, as of November 2026, EvolAnki:

- removed the Qt desktop app, and with it add-ons and the launcher
- removed the Python layer (`pylib`, `rsbridge`) and Python tooling
- removed sync and AnkiHub: collection and media sync, the sync server, AnkiWeb and
  GitHub update and add-on lookups, and AnkiDroid's backend support
- removed installers and packaging (wheels, Briefcase templates, audio packages)
- removed the editor, importers, card browser, change notetype and image occlusion, and
  suspend controls in the reviewer
- removed card flags
- removed leeches
- removed the card info page and its forgetting curve
- removed the congratulations page
- removed burying: cards of one note are shown on the same day
- removed translations other than English, and upstream's manual
- no longer runs JavaScript in cards
- added `evolanki-server` (`rslib/web-server`), and web pages for the deck list, overview
  and reviewer, laid out for phones, with automatic backups
- added `evolanki-admin`, a JSON command-line tool that agents use to manage cards, and to
  convert a desktop profile ([docs](./docs/admin-cli.md))
- always uses FSRS

The full history of changes is in this repository's commits.

## Contributors

The following people have contributed to Anki: [CONTRIBUTORS](./CONTRIBUTORS)

## License

GNU AGPL, version 3 or later, with other licenses for some included items:
[LICENSE](./LICENSE). If you run a modified version for others over a network, the AGPL
requires you to offer them its source.
