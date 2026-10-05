#!/usr/bin/env python3
"""Dump Options > media playback > per-filetype handling: the list's rows for a
new client's options (pretty filetype, media and preview show actions and zoom
info, in the list's sort order), which filetypes "add" offers and which rows
"delete" may remove, and for every filetype the edit panel's intro text and
media/preview show-action choices (mpv taken as available). Pure reference
functions and constants; the panel's own methods run on a stand-in object."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from hydrus.core import HydrusConstants as HC
from hydrus.client import ClientConstants as CC
from hydrus.client import ClientOptions
from hydrus.client.gui.panels.options import MediaPlaybackPanel as P

panel = P.MediaPlaybackPanel.__new__(P.MediaPlaybackPanel)
options = ClientOptions.ClientOptions()
rows = []
for mime, view in options.GetMediaViewOptions().items():
    data = (mime, *view[:6], view[6])
    rows.append(dict(mime=mime, display=list(P.MediaPlaybackPanel._GetListCtrlDisplayTuple(panel, data))))
rows.sort(key=lambda r: r["display"])
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
    addable=[[pretty, mime] for pretty, mime in addable],
    deletable=sorted(HC.SEARCHABLE_MIMES),
    actions={str(k): v for k, v in CC.media_viewer_action_string_lookup.items()},
    scales={str(k): v for k, v in CC.media_viewer_scale_string_lookup.items()},
    zooms={str(k): v for k, v in CC.zoom_string_lookup.items()},
    editors=[choices(mime) for mime in sorted(CC.media_viewer_capabilities)],
)
path = os.path.join(HERE, "fixtures/media_view_options.json")
with open(path, "w") as stream:
    json.dump(out, stream, indent=1)
    stream.write("\n")
print("wrote " + path, len(rows), len(out["addable"]), len(out["editors"]))
