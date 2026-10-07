# Outline emoji

`NotoEmoji.ttf` is the unmodified Noto Emoji variable font from
[google/fonts at b979dba422e445492b0eb9951ac52ee0b4d648c3](https://github.com/google/fonts/tree/b979dba422e445492b0eb9951ac52ee0b4d648c3/ofl/notoemoji),
upstream filename `NotoEmoji[wght].ttf`. Its SIL Open Font License and copyright
notices are preserved in `NotoEmoji-OFL.txt`.

SHA-256: `de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551`.

Slint 1.18.1's software renderer paints outlines, while the Linux system's Noto
Color Emoji has bitmap glyphs only. This font supplies monochrome outlines,
including the fox used in the recorded OR connector editor. Startup prepends it
before the platform's preferred emoji face in Slint's actual sans-serif/system-UI
fallback chains, retaining every preceding text fallback and the platform tail.
Fontique appends that tail implicitly, so the retained prefix is searched twice
for unsupported characters; repeated initialization does not grow it. Explicitly
requested bitmap fonts can still outrank this fallback. Script mappings stay intact.
Emoji colour and shape need not match Qt or a platform colour-emoji font.

The isolated adapter in `src/fonts.rs` uses the internal core font context;
upgrade the pinned Slint runtime, build helper and core dependency together and
rerun the actual font-selection and rendered OR editor regressions.
