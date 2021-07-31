# oxbow

[![CI]][workflow]

`oxbow` is a Twitch chatbot.

Currently it's capable of storing and sending responses to per-channel custom commands, and solving 
the "wordstonks" game [implemented by Stuck Overflow's `ferris-bot`][ferris-bot] using a binary 
search strategy augmented by information about the Hamming distance to the correct answer.

## Usage

### Prerequisites

Install [`rustup`][rustup].

### Configuration

Create a file `.env`, in the following format (you can copy `.env.sample` if you want to):

```
CLIENT_ID=""
CLIENT_SECRET=""
TWITCH_NAME=""
DATABASE=""
```

- Set `CLIENT_ID` and `CLIENT_SECRET` to a client ID and secret obtained from registering an 
    application in [the Twitch developer portal][dev-portal]. When registering the application, set 
    the redirect URL to `http://localhost:10666/` (note the trailing slash!).
- Set `TWITCH_NAME` to the account name you would like the bot to log in to and chat as.
- Set `DATABASE` to a path (relative to the directory you will run the bot in, or absolute) to a 
    `.sqlite3` file. The file does not have to exist before running the bot; it will be created for 
    you if necessary. Alternatively, remove the `DATABASE=` line to use an in-memory database.

> Instead of creating a `.env` file, you can provide these values through command line arguments or 
> by setting the above environment variables in some other way. Run the program with the `--help` 
> flag for more information.

### Run

Build the bot with `cargo build --release`, and then run the executable at `target/release/oxbow`. 

You must provide the `--channels` argument with a space-separated list of Twitch chat channels to 
join (e.g. `oxbow --channels nerosnm stuck_overflow`), otherwise the bot will not join any channels 
and you will have no way of interacting with it.

To customise the prefix that the bot uses for commands (default: `!`), you can provide the 
`--prefix` argument, e.g. `oxbow --channels nerosnm --prefix '|'`.

> To provide command line arguments if you are using `cargo run` to run the bot rather than the
> executable itself, they should be provided after a `--` argument, e.g. `cargo run --release -- 
> --channels foo bar baz`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or 
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the 
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any 
additional terms or conditions.

[CI]: https://github.com/nerosnm/oxbow/actions/workflows/ci.yml/badge.svg?branch=main
[workflow]: https://github.com/nerosnm/oxbow/actions/workflows/ci.yml
[ferris-bot]: https://github.com/stuck-overflow/ferris-bot/blob/main/src/word_stonks.rs
[rustup]: https://rustup.rs
[dev-portal]: https://dev.twitch.tv/console/apps
