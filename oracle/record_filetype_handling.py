#!/usr/bin/env python3
"""Add, edit and delete a per-filetype media handling row, and what the
reference's viewer then does with a jpeg.

On a new client, in the real "media playback" options panel
(`MediaPlaybackPanel`): "add" (the filetype chosen from the list, the real
`EditMediaViewOptionsPanel` set to "show at 100%" for scaling up),
`UpdateOptions` (Options OK); "edit" of that row (to "scale to the canvas
size") and OK; "delete" of the row (the real list's "Remove all selected?"
question accepted) and OK. After each OK, for a small jpeg (300x200) and a
big jpeg (4000x3000) in a 1000x750 canvas, the zooms the viewer would use
(`CalculateCanvasZooms` with the options' view options for the mime, whose
default zoom type is the viewer's initial zoom). Also the rows the table
listed, the addable filetypes and the "can delete" state of a general row.
Writes `fixtures/filetype_handling.json`.

Run: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_filetype_handling.py
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

FILES = [('small jpeg', (300, 200)), ('big jpeg', (4000, 3000))]
CANVAS = (1000, 750)


class FakeMedia(object):
    def __init__(self, mime, resolution):
        self._mime = mime
        self._resolution = resolution

    def GetMime(self):
        return self._mime

    def GetResolution(self):
        return self._resolution

    def HasUsefulResolution(self):
        (w, h) = self._resolution
        return w is not None and w != 0 and h is not None and h != 0


def record(session):
    def qt():
        from qtpy import QtCore as QC
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui.canvas import ClientGUICanvasMedia
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEdit
        from hydrus.client.gui.panels.options.MediaPlaybackPanel import MediaPlaybackPanel
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels

        c = session.controller
        original = c.new_options.Duplicate()
        mime = HC.IMAGE_JPEG
        panels = []
        state = {'scale_up': None, 'asked': []}

        def select(win, title, choice_tuples, **kwargs):
            state['asked'].append({'select': title, 'choices': sorted(label for (label, data) in choice_tuples)})
            for (label, data) in choice_tuples:
                if data == mime:
                    return data
            raise Exception('no jpeg')

        def yes_no(win, message, **kwargs):
            state['asked'].append({'yes_no': message})
            return QW.QDialog.DialogCode.Accepted

        def exec_(dlg):
            panel = dlg.findChild(ClientGUIScrolledPanelsEdit.EditMediaViewOptionsPanel)
            state['asked'].append({'dialog': dlg.windowTitle(), 'media_scale_up_before': panel._media_scale_up.GetValue()})
            if state['scale_up'] is not None:
                panel._media_scale_up.SetValue(state['scale_up'])
            return QW.QDialog.DialogCode.Accepted

        ClientGUIDialogsQuick.SelectFromList = select
        ClientGUIDialogsQuick.GetYesNo = yes_no
        ClientGUITopLevelWindowsPanels.DialogEdit.exec = exec_

        def listed(panel):
            return sorted(HC.mime_string_lookup.get(data[0], str(data[0])) if False else data[0] for data in panel._filetype_handling_listctrl.GetData())

        def zooms():
            out = {}
            for (name, resolution) in FILES:
                media = FakeMedia(mime, resolution)
                (show_action, *rest) = c.new_options._GetMediaViewOptions(mime)
                z = ClientGUICanvasMedia.CalculateCanvasZooms(QC.QSize(*CANVAS), CC.CANVAS_MEDIA_VIEWER, 1.0, media, show_action)
                out[name] = {str(k): v for (k, v) in z.items()}
            return out

        def row_of(panel, wanted):
            for data in panel._filetype_handling_listctrl.GetData():
                if data[0] == wanted:
                    return data
            return None

        out = {'canvas': list(CANVAS), 'files': {n: list(r) for (n, r) in FILES}, 'phases': []}
        try:
            panel = MediaPlaybackPanel(c.gui)
            panels.append(panel)
            out['start'] = {
                'jpeg_row': row_of(panel, mime) is not None,
                'addable_includes_jpeg': mime in panel._GetUnsetMediaViewFiletypes(),
                'can_add': panel._CanAddMediaViewOption(),
                'zooms': zooms(),
                'show_action': c.new_options._GetMediaViewOptions(mime)[0],
            }

            # add
            state['scale_up'] = CC.MEDIA_VIEWER_SCALE_100
            state['asked'] = []
            panel.AddMediaViewerOptions()
            staged = row_of(panel, mime)
            before_ok = zooms()
            panel.UpdateOptions()
            out['phases'].append({
                'do': 'add', 'scale_up': CC.MEDIA_VIEWER_SCALE_100, 'asked': state['asked'],
                'row_listed': staged is not None, 'staged_scale_up': None if staged is None else staged[7][0],
                'zooms_before_ok': before_ok, 'zooms': zooms(),
                'saved_scale_up': c.new_options._GetMediaViewOptions(mime)[6][0],
            })

            # edit (a panel opened afresh on what is saved)
            panel = MediaPlaybackPanel(c.gui)
            panels.append(panel)
            panel._filetype_handling_listctrl.SelectDatas([row_of(panel, mime)], deselect_others=True)
            state['scale_up'] = CC.MEDIA_VIEWER_SCALE_TO_CANVAS
            state['asked'] = []
            panel.EditMediaViewerOptions()
            before_ok = zooms()
            staged = row_of(panel, mime)
            panel.UpdateOptions()
            out['phases'].append({
                'do': 'edit', 'scale_up': CC.MEDIA_VIEWER_SCALE_TO_CANVAS, 'asked': state['asked'],
                'row_listed': staged is not None, 'staged_scale_up': None if staged is None else staged[7][0],
                'zooms_before_ok': before_ok, 'zooms': zooms(),
                'saved_scale_up': c.new_options._GetMediaViewOptions(mime)[6][0],
            })

            # delete
            panel = MediaPlaybackPanel(c.gui)
            panels.append(panel)
            panel._filetype_handling_listctrl.SelectDatas([row_of(panel, mime)], deselect_others=True)
            can_delete = panel._CanDeleteMediaViewOptions()
            state['asked'] = []
            panel._filetype_handling_listctrl.ProcessDeleteAction()
            before_ok = zooms()
            staged = row_of(panel, mime)
            panel.UpdateOptions()
            out['phases'].append({
                'do': 'delete', 'can_delete': can_delete, 'asked': state['asked'],
                'row_listed': staged is not None,
                'zooms_before_ok': before_ok, 'zooms': zooms(),
                'show_action': c.new_options._GetMediaViewOptions(mime)[0],
            })
            return out
        finally:
            c.new_options = original
            for p in panels:
                p.hide()
                p.deleteLater()

    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as directory:
        result = Path(directory) / 'record.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(result))
        out = json.loads(result.read_text())
    path = HERE / 'fixtures/filetype_handling.json'
    path.write_text(json.dumps(out, indent=1) + '\n')
    print('wrote ' + str(path))


if __name__ == '__main__':
    main()
