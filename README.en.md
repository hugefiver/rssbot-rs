# rssbot [![Build Status](https://github.com/iovxw/rssbot/workflows/Rust/badge.svg)](https://github.com/iovxw/rssbot/actions?query=workflow%3ARust) [![Github All Releases](https://img.shields.io/github/downloads/iovxw/rssbot/total.svg)](https://github.com/iovxw/rssbot/releases)

**Other Languages:** [Chinese](README.md)

Telegram RSS bot [@RustRssBot](http://t.me/RustRssBot)

**Supports:**
 - [x] RSS 0.9
 - [x] RSS 0.91
 - [x] RSS 0.92
 - [x] RSS 0.93
 - [x] RSS 0.94
 - [x] RSS 1.0
 - [x] RSS 2.0
 - [x] Atom 0.3
 - [x] Atom 1.0
 - [x] JSON Feed 1

## Usage

    /rss       - Display a list of currently subscribed RSS feeds
    /sub       - Subscribe to an RSS: /sub http://example.com/feed.xml
    /unsub     - Unsubscribe from an RSS: /unsub http://example.com/feed.xml
    /export    - Export to OPML

## Download

The pre-compiled binaries can be downloaded directly from [Releases](https://github.com/iovxw/rssbot/releases). Make sure to use the english binary (`rssbot-en-amd64-linux`). The Linux version is statically linked to *musl*, no other dependencies required.

## Compile

**Please try to download from the Link above, if that's not feasible or you have other requirements you should compile manually**

Install *Rust Nightly* and *Cargo* ([`rustup` recommended](https://www.rustup.rs/)) first, then:

```
LOCALE=en cargo build --release
```

The compiled files are available at: `./target/release/rssbot`

## Run

```
USAGE:
    rssbot [FLAGS] [OPTIONS] <token>

FLAGS:
    -h, --help          Prints help information
        --insecure      DANGER: Insecure mode, accept invalid TLS certificates
        --restricted    Make bot commands only accessible for group admins
    -V, --version       Prints version information

OPTIONS:
        --admin <user id>...        Private mode, only specified user can use this bot. This argument can be passed
                                    multiple times to allow multiple admins
    -d, --database <path>           Path to database [default: ./rssbot.json]
        --max-feed-size <bytes>     Maximum feed size, 0 is unlimited [default: 2097152]
        --max-interval <seconds>    Maximum fetch interval [default: 43200]
        --min-interval <seconds>    Minimum fetch interval [default: 300]

ARGS:
    <token>    Telegram bot token

NOTE: You can get <user id> using bots like @userinfobot @getidsbot
```

Please read the [official docs](https://core.telegram.org/bots#3-how-do-i-create-a-bot) to create a token.

## License

This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or distribute this software, either in source code form or as a compiled binary, for any purpose, commercial or non-commercial, and by any means.
