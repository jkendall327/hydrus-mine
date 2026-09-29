# Decisions needed from you

GitHub issues are disabled on this repository, so open questions for the
maintainer are collected here. None of them blocks current work: each says
what happens by default. Answer by editing this file, in a commit message, or
in chat. Answered questions move to the log at the bottom.

## 1. GUI technology

The rewrite is backend-first (native store, importer, Client API). The GUI
comes last, and the toolkit is your call.

| option | for | against |
|---|---|---|
| **Qt 6 / QML via [cxx-qt](https://github.com/KDAB/cxx-qt)** | Keeps Qt, which you asked for. Native look, mature widgets, Qt's image and video stack. | Needs a C++ toolchain and Qt at build time. QML is a different model from the QtWidgets the Python client uses, so there's little to reuse. The Rust/Qt bridge adds ceremony. |
| **[Slint](https://slint.dev)** | Rust-native, declarative, fast, easy to build. Handles large virtualised grids well. | Younger widget set. Licensing: GPLv3, royalty-free (with attribution), or commercial. Fine for personal use. |
| **[egui](https://github.com/emilk/egui)** | Pure Rust, fastest to iterate on, very fast at runtime. | Immediate-mode look and feel. Less polished for a complex, dialog-heavy app. |
| **Web UI served by `hydrus serve`** | Uses the Client API we're already building. Works from any device on your network. | Not a native desktop app (needs a browser or a Tauri wrapper). Video and keyboard-heavy workflows need care. |

**Recommendation:** Qt/QML via cxx-qt if staying on Qt matters to you,
otherwise Slint.
**Default:** keep building the backend and the Client API (every option needs
them); no GUI code yet.

## 2. Which hydrus features do you actually use?

This decides what gets built first, and what may never need building:

- tag repositories (the PTR): update sync, processing, petitions
- downloaders: gallery downloaders, subscriptions, watchers, parsers, login
  scripts (the largest and jankiest part of the reference, and a candidate for
  a cleaner redesign rather than a port)
- duplicates: similar-file search and the duplicates filter
- file repositories, the hydrus server, IPFS
- import and export folders, sidecars
- which Client API tools (Hydrus Companion, Hydrus Web, hyshare, ...)?
  Knowing them lets me test against their real request patterns.

**Default order:** Client API parity, then local import and search, then
duplicates, then downloaders (redesigned), then tag repository sync. The
store is designed for PTR scale (~10⁹ mappings) either way.

## 3. Client API over HTTPS?

The reference can serve its Client API over HTTPS with a self-signed
certificate. `hydrus serve` serves plain HTTP, and warns if the imported
Client API service has HTTPS switched on.
**Default:** HTTP only, until you want HTTPS (rustls with a generated
certificate is straightforward).

## 4. Naming

The project is "hydrus-rs" in the code and docs, and the binary is `hydrus`.
If you'd like a different name, now is when renaming is cheap.
**Default:** keep both.

---

## Answered

(none yet)
