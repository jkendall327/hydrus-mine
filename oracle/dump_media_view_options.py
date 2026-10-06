#!/usr/bin/env python3
"""Dump Options > media playback > per-filetype handling: the list's rows for a
new client's options (pretty filetype, media and preview show actions and zoom
info, in the list's sort order), which filetypes "add" offers and which rows
"delete" may remove, and for every filetype the edit panel's intro text and
media/preview show-action choices. Defaults are recorded explicitly with and
without mpv on a non-macOS platform; the macOS fallback is checked too. Pure reference
functions and constants; the panel's own methods run on a stand-in object."""
import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from hydrus.core import HydrusConstants as HC
from hydrus.client import ClientConstants as CC
from hydrus.client import ClientOptions
from hydrus.client.gui.canvas import ClientGUIMPV
from hydrus.client.gui.panels.options import MediaPlaybackPanel as P

panel = P.MediaPlaybackPanel.__new__(P.MediaPlaybackPanel)
def defaults(mpv_available, platform_macos):
    original = ClientGUIMPV.MPV_IS_AVAILABLE, HC.PLATFORM_MACOS
    try:
        ClientGUIMPV.MPV_IS_AVAILABLE = mpv_available
        HC.PLATFORM_MACOS = platform_macos
        views = ClientOptions.ClientOptions().GetMediaViewOptions()
    finally:
        ClientGUIMPV.MPV_IS_AVAILABLE, HC.PLATFORM_MACOS = original
    rows = []
    for mime, view in views.items():
        data = (mime, *view[:6], view[6])
        rows.append(dict(mime=mime, display=list(P.MediaPlaybackPanel._GetListCtrlDisplayTuple(panel, data))))
    rows.sort(key=lambda row: row["display"])
    return dict(mpv_available=mpv_available, platform_macos=platform_macos,
                views=[[mime, list(view)] for mime, view in sorted(views.items())], rows=rows)


qt_defaults = defaults(False, False)
mpv_defaults = defaults(True, False)
macos_defaults = defaults(True, True)
assert macos_defaults["rows"] == qt_defaults["rows"]
assert macos_defaults["views"] == qt_defaults["views"]
rows = qt_defaults["rows"]
set_mimes = {r["mime"] for r in rows}
addable = sorted((P.MediaPlaybackPanel._GetPrettyMime(panel, m), m) for m in set(HC.SEARCHABLE_MIMES) - set_mimes)


def choices(mime):
    possible, can_pause, can_embed = CC.media_viewer_capabilities[mime]
    media, preview = [], []
    for action in possible:
        s = CC.media_viewer_action_string_lookup[action]
        if action == CC.MEDIA_VIEWER_ACTION_SHOW_WITH_NATIVE and mime in [HC.GENERAL_VIDEO] + list(HC.VIDEO):
            s += " (no audio support)"
        media.append([s, action])
        if action != CC.MEDIA_VIEWER_ACTION_DO_NOT_SHOW_ON_ACTIVATION_OPEN_EXTERNALLY:
            preview.append([s, action])
    text = "Setting media view options for " + HC.mime_string_lookup[mime] + "."
    if CC.MEDIA_VIEWER_ACTION_SHOW_WITH_MPV in possible:
        text += "\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback."
    shows = not set(possible).isdisjoint({CC.MEDIA_VIEWER_ACTION_SHOW_WITH_NATIVE, CC.MEDIA_VIEWER_ACTION_SHOW_WITH_MPV, CC.MEDIA_VIEWER_ACTION_SHOW_WITH_QTMEDIAPLAYER})
    try:
        pretty = P.MediaPlaybackPanel._GetPrettyMime(panel, mime)
    except KeyError:
        pretty = HC.mime_string_lookup[mime]
    return dict(mime=mime, pretty=pretty, intro=text,
                media=media, preview=preview, zoom_rows=shows)


out = dict(
    rows=rows,
    default_contexts={"qt": qt_defaults, "mpv": mpv_defaults},
    macos_mpv_available_matches_qt=True,
    addable=[[pretty, mime] for pretty, mime in addable],
    deletable=sorted(HC.SEARCHABLE_MIMES),
    actions={str(k): v for k, v in CC.media_viewer_action_string_lookup.items()},
    scales={str(k): v for k, v in CC.media_viewer_scale_string_lookup.items()},
    zooms={str(k): v for k, v in CC.zoom_string_lookup.items()},
    editors=[choices(mime) for mime in sorted(CC.media_viewer_capabilities)],
)
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", default=os.path.join(HERE, "fixtures/media_view_options.json"))
path = parser.parse_args().output
with open(path, "w") as stream:
    json.dump(out, stream, indent=1)
    stream.write("\n")
print("wrote " + path, len(rows), len(out["addable"]), len(out["editors"]))
