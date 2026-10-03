#!/usr/bin/env python3
"""Record the real Detailed File Metadata panel's conditional boxes, read-only
text, PNG instruction, EXIF sorting, supported types and nested basics conversion in a
running reference client. Writes fixtures/embedded_metadata_window.json.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

def record(session):
    controller = session.controller
    def f():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsReview as R
        from hydrus.client.gui.widgets import ClientGUICommon
        from hydrus.client.media import ClientMediaResultPrettyInfo as P
        from hydrus.client.media import ClientMediaResultPrettyInfoObjects as O
        from hydrus.client.gui import ClientGUIAsync
        from hydrus.client.gui.media import ClientGUIMediaModalActions as A
        line = O.PrettyMediaResultInfoLine
        sub = O.PrettyMediaResultInfoLinesSubmenu
        lines = [line('one file', True), sub('all modified times', False, [line('local: yesterday', False), sub('sources', False, [line('example.com: today', False)])])]
        out = {'basics': P.ConvertInfoLinesToTextBlock(lines), 'cases': []}
        from dump_embedded_metadata import LOOKED_AT
        out['reads_embedded'] = {HC.mime_string_lookup[mime]: mime == HC.APPLICATION_PDF or mime in LOOKED_AT for mime in [HC.APPLICATION_PDF, HC.IMAGE_JPEG, HC.IMAGE_PNG, HC.IMAGE_WEBP, HC.IMAGE_TIFF, HC.AUDIO_MP3]}
        from hydrus.core import HydrusLists
        datas = [(271, 'Camera\0Maker\0'), (37510, b'ASCII\0\0\0a comment'), (65000, 'Straße'), (65001, 'STRASSE'), (282, 300.0)]
        out['list_rows'] = [list(R.ReviewFileEmbeddedMetadata._ConvertEXIFToDataTuple(None, row)) + [row[1].hex() if isinstance(row[1], bytes) else str(row[1])] for row in datas]
        out['sort_orders'] = []
        keys = [HydrusLists.ConvertTupleOfDatasToCasefolded(R.ReviewFileEmbeddedMetadata._ConvertEXIFToSortTuple(None, row)) for row in datas]
        for column in range(3):
            for asc in [True, False]:
                order = sorted(range(len(datas)), key=lambda i: (keys[i][column], keys[i]), reverse=not asc)
                out['sort_orders'].append({'column': column, 'ascending': asc, 'order': order})
        for name, mime, exif, xmp, iptc, text, extra in [
            ('empty', HC.IMAGE_JPEG, None, None, None, None, []),
            ('png', HC.IMAGE_PNG, {271: 'Camera\0Maker\0', 37510: b'ASCII\0\0\0a comment'}, {'source': 'camera'}, {(2, 5): 'title'}, 'a caption', [('colour mode', 'RGB')]),
            ('non-local', HC.IMAGE_JPEG, None, None, None, 'This file is not local to this computer!', []),
        ]:
            panel = R.ReviewFileEmbeddedMetadata(controller.gui, mime, out['basics'], exif, xmp, iptc, text, extra)
            boxes = [{ 'title': w._title_st.text(), 'visible': not w.isHidden() } for w in panel.findChildren(ClientGUICommon.StaticBox)]
            instruction = next(w.text() for w in panel.findChildren(QW.QLabel) if w.text().startswith('Double-click'))
            out['cases'].append({'name': name, 'boxes': boxes, 'instruction': instruction, 'xmp': panel._xmp_text.toPlainText(), 'iptc': panel._iptc_text.toPlainText(), 'text': panel._human_readable_text.toPlainText(), 'readonly': [w.isReadOnly() for w in [panel._xmp_text, panel._iptc_text, panel._human_readable_text]]})
            panel.deleteLater()
        from hydrus.client import ClientPDFHandling
        out['pdf'] = {}
        malformed = os.path.join(HERE, 'fixtures', 'metadata', 'embedded_malformed.pdf')
        with open(malformed, 'wb') as stream:
            stream.write(b'not a PDF\n')
        from dump_media import blank_pdf
        fields = blank_pdf(info=[
            b'<< /Keywords ', ('str', b'one, two'),
            b' /Subject ', ('str', b'A subject'),
            b' /Title ', ('str', b'\xfe\xff' + 'Unicode title \u03a9'.encode('utf-16-be')),
            b' /Author ', ('str', b'A User'), b' >>',
        ])
        with open(os.path.join(HERE, 'fixtures', 'metadata', 'embedded_pdf_fields.pdf'), 'wb') as stream:
            stream.write(fields)
        for name in ['pdf_text.pdf', 'pdf_empty_title.pdf', 'pdf_keywords_only.pdf', 'pdf_image.pdf', 'pdf_encrypted_owner.pdf', 'pdf_encrypted_user.pdf', '../metadata/embedded_malformed.pdf', '../metadata/embedded_pdf_fields.pdf']:
            path = os.path.join(HERE, 'fixtures', 'media', name)
            try:
                text = ClientPDFHandling.GetHumanReadableEmbeddedMetadata(path)
            except Exception:
                text = None
            out['pdf'][name] = text
        # The non-local message comes from the actual worker, with its I/O bypassed.
        from dump_embedded_metadata import Media, Job, captured
        original_job = ClientGUIAsync.AsyncQtJob
        original_info = P.GetPrettyMediaResultInfoLines
        try:
            ClientGUIAsync.AsyncQtJob = Job
            P.GetPrettyMediaResultInfoLines = lambda _: lines
            media = Media(HC.IMAGE_JPEG, False)
            media.IsLocal = lambda: False
            captured.clear()
            A.ShowFileEmbeddedMetadata(None, media)
            out['non_local_worker_text'] = captured[0]()[5]
        finally:
            ClientGUIAsync.AsyncQtJob = original_job
            P.GetPrettyMediaResultInfoLines = original_info
        return out
    return controller.CallBlockingToQt(controller.gui, f)

def child(path):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(path, 'w') as stream:
        json.dump(result, stream, indent=2)

def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'metadata.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream:
            result = json.load(stream)
    with open(os.path.join(HERE, 'fixtures', 'embedded_metadata_window.json'), 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')

if __name__ == '__main__':
    main()
