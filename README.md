## This fork: hydrus-rs

This repository also holds **hydrus-rs**, a reimplementation of the hydrus client in Rust (`crates/`). It aims to behave as hydrus v688 does, and checks that against the Python client itself: the scripts in `oracle/` run the Python code in `hydrus/` and record what it does, and the Rust tests compare against those recordings. The Python code is upstream hydrus, kept unchanged as the reference.

What it does so far:

- **Imports a v688 install** without changing it: files (used in place, hardlinked, copied or moved), tags with their siblings and parents, notes, ratings, URLs, file relationships, subscriptions with their histories, import and export folders, duplicates auto-resolution rules, downloader queues and the last session's pages.
- **Serves the Client API** (`hydrus serve`) with the same port, access keys and services, for Hydrus Companion and other tools: every endpoint except `/manage_pages/new_page`, four of them with small gaps (see `parity/manifest.toml`).
- **Runs the background work hydrus runs:** downloaders, subscriptions, thread watchers, import and export folders, the similar-files search and duplicates auto-resolution.
- **A desktop client** (`hydrus-gui`), early, which runs `hydrus serve` while it is open: the last session's pages, search pages with autocomplete, sorting, a tag list and a thumbnail grid, a media viewer (video through mpv), and the duplicate filter, with the reference's comparison statements.

To try it, see [docs/rust/MIGRATING.md](docs/rust/MIGRATING.md). [docs/rust/DIFFERENCES.md](docs/rust/DIFFERENCES.md) lists where it knowingly behaves differently from hydrus, [docs/rust/ARCHITECTURE.md](docs/rust/ARCHITECTURE.md) explains how it is built, and [docs/rust/DECISIONS.md](docs/rust/DECISIONS.md) records what was decided and the roadmap.

The rest of this README is hydrus's own.

## Hydrus Network (Client and Server)

The hydrus network client is a file-management application written for internet-fluent media nerds who have large file collections. It browses with tags instead of folders, a little like a booru on your desktop. If they wish, users can easily share tags anonymously through a public server. Everything is free, no ads, and privacy is the first concern. If you have 10,000+ files and cannot find anything, hydrus might help!

Hydrus supports various filetypes for images, video and audio files, image project files, and more. A full list of supported filetypes is [here](https://hydrusnetwork.github.io/hydrus/filetypes.html). It has audio and video playback via an mpv embed or a native Qt player. Some supported filetypes cannot be viewed directly in the client, such as PDF, but it is easy to launch any file with your OS's default program.

I am continually working on the software and try to put out a new release every Wednesday by 8pm EST. Executable releases are available for Windows and Linux, but the program is in python, so you can also just run it straight from the source code in Windows, Linux, or macOS. I am not active here on github, but I welcome feedback of any sort on other channels and will try to get back to any pings every Saturday. 

The client can download files and parse tags and other metadata from simple websites using easily shareable user-made downloaders. It can also be set to 'subscribe' to any gallery search, repeating it every few days to keep up with new results.

The program's emphasis is on your freedom. You control everything, and the program never phones home. In the same way, it is quite an advanced program, and not a beautiful one, so it isn't for everyone. Try it out, see if you like it!

Hydrus is mostly a solo project. **Feel free to fork and do whatever you like with my code, but public pull requests are currently closed.** The [issue tracker here on Github](https://github.com/hydrusnetwork/hydrus/issues) is active and run by volunteer users.

## Start Here!

**[Getting Started Guide](https://hydrusnetwork.github.io/hydrus/introduction.html)**

This help will walk you through installation and teach you the main systems of the program. Hydrus can do a lot, so while you can skim the help, do not skip it. 

The help is also included in every release.

# Links

* [homepage](https://hydrusnetwork.github.io/hydrus/)
* [issue tracker](https://github.com/hydrusnetwork/hydrus/issues)
* [proton](mailto:hydrus_dev@proton.me)
* [gmail](mailto:hydrus.admin@gmail.com)
* [discord](https://discord.gg/wPHPCUZ)
* [tumblr](https://hydrus.tumblr.com/)
* [x](https://x.com/hydrusnetwork)
* [patreon](https://www.patreon.com/hydrus_dev)
* [user-run repository and wiki](https://github.com/CuddleBear92/Hydrus-Presets-and-Scripts)

## Attribution

I use a number of the Silk Icons by Mark James at famfamfam.com.
