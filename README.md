# rssbot [![Build Status](https://github.com/iovxw/rssbot/workflows/Rust/badge.svg)](https://github.com/iovxw/rssbot/actions?query=workflow%3ARust) [![Github All Releases](https://img.shields.io/github/downloads/iovxw/rssbot/total.svg)](https://github.com/iovxw/rssbot/releases)

**Other Languages:** [English](README.en.md)

Telegram RSS 机器人 [@RustRssBot](http://t.me/RustRssBot)

**支持:**
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

## 使用

    /rss       - 显示当前订阅的 RSS 列表
    /sub       - 订阅一个 RSS: /sub http://example.com/feed.xml
    /unsub     - 退订一个 RSS: /unsub http://example.com/feed.xml
    /export    - 导出为 OPML

## 下载

可直接从 [Releases](https://github.com/iovxw/rssbot/releases) 下载预编译的程序（带 `zh` 的为中文版）, Linux 版本为 *musl* 静态链接, 无需其他依赖

## 编译

**请先尝试从上面下载, 如不可行或者有其他需求再手动编译**

先安装 *Rust Nightly* 以及 *Cargo* (推荐使用 [`rustup`](https://www.rustup.rs/)), 然后:

```
cargo build --release
```

编译好的文件位于: `./target/release/rssbot`

## 运行

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

`<token>` 请参照 [这里](https://core.telegram.org/bots#3-how-do-i-create-a-bot) 申请

## License

This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or distribute this software, either in source code form or as a compiled binary, for any purpose, commercial or non-commercial, and by any means.
