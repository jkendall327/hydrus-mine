#!/usr/bin/env python3
"""Record real Qt GUG selector captions, functional groups and nested selection.

Synthetic URL classes/parser links and GUGs are installed in the real manager.
Dialog timers drive actual SelectGUGKeyAndName widgets, including Cancel and
non-functional choices; no network requests or real-site definitions are used.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'gallery_source.json')


def record(session):
    def qt():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.core import HydrusConstants as HC, HydrusExceptions
        from hydrus.client.gui import ClientGUIDialogsMessage as M
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W
        from hydrus.client.gui.importing import ClientGUIImport as I
        from hydrus.client.networking import ClientNetworkingGUG as G, ClientNetworkingURLClass as U
        from hydrus.client.parsing import ClientParsing as P
        manager = session.controller.network_engine.domain_manager
        classes = [U.URLClass(name=name, url_type=HC.URL_TYPE_GALLERY, url_class_key=bytes([key])*32,
            url_domain_mask=U.URLDomainMask(raw_domains=[domain]), path_components=[], parameters=[],
            example_url='https://' + domain + '/search?q=test')
            for name, key, domain in [('synthetic gallery', 81, 'gallery.example'), ('unlinked gallery', 82, 'unlinked.example'), ('missing parser gallery', 83, 'missing-parser.example')]]
        parser = P.PageParser('synthetic gallery parser', parser_key=bytes([91])*32)
        manager.SetURLClasses(classes); manager.SetParsers([parser])
        manager.SetURLClassKeysToParserKeys({classes[0].GetClassKey(): parser.GetParserKey(), classes[2].GetClassKey(): bytes([99])*32})
        def single(name, key, domain='gallery.example', phrase='%tags%'):
            return G.GalleryURLGenerator(name, gug_key=bytes([key])*32,
                url_template='https://' + domain + '/search?q=%tags%', replacement_phrase=phrase,
                example_search_text='blue eyes', initial_search_text='tag')
        gugs = [single('zeta visible', 11), single('Alpha visible', 12), single('beta hidden', 13),
            single('unknown URL', 14, 'unknown.example'), single('unlinked URL', 15, 'unlinked.example'),
            single('missing parser', 16, 'missing-parser.example'), single('broken template', 17, phrase=''),
            G.NestedGalleryURLGenerator('nested broken', gug_key=bytes([18])*32, gug_keys_and_names=[(bytes([15])*32, 'unlinked URL')]),
            G.NestedGalleryURLGenerator('nested missing', gug_key=bytes([19])*32, gug_keys_and_names=[(bytes([77])*32, 'gone')]),
            G.NestedGalleryURLGenerator('nested empty', gug_key=bytes([20])*32, gug_keys_and_names=[])]
        manager.SetGUGs(gugs); manager.SetGUGKeysToDisplay([g.GetGUGKey() for g in gugs if g.GetName() != 'beta hidden'])
        def pair(value): return None if value is None else [value[0].hex(), value[1]]
        labels = []
        for value in [None, (bytes([12])*32, 'old name'), (bytes([77])*32, 'beta hidden'), (bytes([77])*32, 'gone'), (bytes([12])*32, '')]:
            button = I.GUGKeyAndNameSelector(session.controller.gui, value)
            labels.append({'given': pair(value), 'caption': button.text(), 'value': pair(button.GetValue())})
            button.deleteLater()
        functional = []
        for gug in gugs:
            try: gug.CheckFunctional(); error = None
            except Exception as e: error = str(e)
            functional.append({'name': gug.GetName(), 'error': error})
        dialogs = []; answers = []; warnings = []
        original_exec = W.DialogEdit.exec; original_warning = M.ShowWarning
        def execute(dlg):
            panel = dlg._panel; listing = panel._list
            entry = {'title': dlg.windowTitle(), 'choices': [listing.item(i).text() for i in range(listing.count())], 'selected': [item.text() for item in listing.selectedItems()]}
            answer = answers.pop(0); entry['answer'] = answer; dialogs.append(entry)
            def respond():
                if answer is None: dlg.reject()
                else:
                    listing.clearSelection(); listing.item(answer).setSelected(True); dlg.accept()
            QC.QTimer.singleShot(0, respond)
            return original_exec(dlg)
        W.DialogEdit.exec = execute; M.ShowWarning = lambda parent, text: warnings.append(text)
        cases = []
        try:
            current = (bytes([77])*32, 'beta hidden')
            for name, scripted in [('cancel', [None]), ('visible', [0]), ('other', [4, 0]), ('other cancel', [4, None]), ('non-functional', [5, 0])]:
                dialogs.clear(); warnings.clear(); answers[:] = scripted
                try: value = pair(I.SelectGUGKeyAndName(session.controller.gui, current)); cancelled = False
                except HydrusExceptions.CancelledException: value = None; cancelled = True
                assert not answers
                cases.append({'case': name, 'dialogs': list(dialogs), 'value': value, 'cancelled': cancelled})
            manager.SetGUGs([])
            for for_sub in [False, True]:
                warnings.clear()
                try: I.SelectGUGKeyAndName(session.controller.gui, None, for_new_sub=for_sub)
                except HydrusExceptions.CancelledException: pass
                cases.append({'case': 'empty subscription' if for_sub else 'empty', 'warnings': list(warnings)})
        finally: W.DialogEdit.exec = original_exec; M.ShowWarning = original_warning
        return {'gugs': [g.GetSerialisableTuple() for g in gugs], 'classes': [c.GetSerialisableTuple() for c in classes], 'parser': parser.GetSerialisableTuple(),
            'display': [g.GetGUGKey().hex() for g in gugs if g.GetName() != 'beta hidden'], 'labels': labels, 'functional': functional, 'cases': cases}
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv) > 1:
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream: json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as work:
        path = os.path.join(work, 'gallery.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream: result = json.load(stream)
    with open(OUT, 'w') as stream: json.dump(result, stream, indent=2, ensure_ascii=False); stream.write('\n')
    print('recorded gallery source Qt selector')

if __name__ == '__main__': main()
