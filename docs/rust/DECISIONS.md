# Decisions

Open questions for the maintainer go in GitHub issues. This file keeps the
record of what was decided, and why the roadmap looks the way it does.

## Decided (2026-09-29)

- **GUI: Slint.** To be tried when the GUI work starts. The backend and
  Client API come first either way.
- **Name: hydrus-rs**, with the binary called `hydrus`.
- **Client API over HTTP only.** No HTTPS.
- **Features in use, which set the priorities:**
  - used: downloaders; duplicates filter (would be used more if it were
    faster); import and export folders; sidecars; Hydrus Companion (in
    Vivaldi) through the Client API
  - not used: the PTR and other tag repositories; the hydrus server; file
    repositories; IPFS

## Roadmap that follows

1. **Client API parity**, with Hydrus Companion's request patterns checked
   first: URL lookups (`/add_urls/get_url_info`, `/add_urls/get_url_files`),
   sending URLs (`/add_urls/add_url`), tags, pages, cookies and headers.
2. **Search and local import**: file search, autocomplete, the media
   pipeline, the file import pipeline.
3. **Duplicates**, built for speed: perceptual-hash search, potential pairs
   and the filter's data model.
4. **Downloaders**, redesigned rather than ported: gallery downloaders,
   subscriptions, watchers, URL classes, parsers. `/add_urls/add_url` needs
   this.
5. **Import/export folders and sidecars.**
6. **GUI in Slint.**

Deprioritised: tag repository sync, the hydrus server, file repositories and
IPFS. Their data and settings are still imported verbatim, so nothing is lost
if they're wanted later.
