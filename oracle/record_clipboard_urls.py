#!/usr/bin/env python3
"""Record the real FrameGUI clipboard watcher and URL import policy on Qt.

Clipboard reads and destination pages are substituted while the actual
REPEATINGClipboardWatcher and _ImportURL methods run against real URL classes
and parser links. Record switch resets, unchanged/empty text, unavailable text,
unparseable and unknown URLs, and a fatal access error. No site is contacted.
"""

import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'clipboard_urls.json')


def record(session):
    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusData, HydrusExceptions
    from hydrus.client.networking import ClientNetworkingURLClass as U
    from hydrus.client.parsing import ClientParsing

    controller = session.controller
    gui = controller.gui

    def qt():
        kinds = [('watch', HC.URL_TYPE_WATCHABLE), ('post', HC.URL_TYPE_POST),
                 ('gallery', HC.URL_TYPE_GALLERY), ('file', HC.URL_TYPE_FILE),
                 ('unlinked', HC.URL_TYPE_POST)]
        classes = [U.URLClass(name=name, url_type=kind, url_class_key=bytes([n + 1]) * 32,
                             url_domain_mask=U.URLDomainMask(raw_domains=[name + '.example']),
                             path_components=[], parameters=[],
                             example_url='https://' + name + '.example/1')
                   for n, (name, kind) in enumerate(kinds)]
        parser = ClientParsing.PageParser('clipboard parser', parser_key=b'\x77' * 32)
        manager = controller.network_engine.domain_manager
        manager.SetURLClasses(classes)
        manager.SetParsers([parser])
        manager.SetURLClassKeysToParserKeys({c.GetClassKey(): parser.GetParserKey()
                                           for c in classes if c.GetName() != 'unlinked'})
        calls = []
        messages = []

        class Sidebar:
            def __init__(self, kind):
                self.kind = kind

            def PendURL(self, url, **kwargs):
                calls.append({'url': url, 'destination': self.kind})

        class Page:
            def __init__(self, kind):
                self.sidebar = Sidebar(kind)

            def GetSidebar(self):
                return self.sidebar

            def GetName(self):
                return 'test destination'

        class Notebook:
            def GetOrMakeURLImportPage(self, **kwargs):
                assert kwargs['select_page'] is False
                return Page('urls')

            def GetOrMakeMultipleWatcherPage(self, **kwargs):
                assert kwargs['select_page'] is False
                return Page('watchers')

        class Timer:
            def Cancel(self):
                pass

        clipboard = [None]
        def get_text():
            value = clipboard[0]
            if value is None:
                raise HydrusExceptions.DataMissing('no text')
            if isinstance(value, Exception):
                raise value
            return value

        old_notebook = gui._notebook
        old_clipboard = controller.GetClipboardText
        old_show_text = HydrusData.ShowText
        old_job = gui._clipboard_watcher_repeating_job
        controller.GetClipboardText = get_text
        HydrusData.ShowText = messages.append
        gui._notebook = Notebook()
        gui._clipboard_watcher_repeating_job = Timer()
        gui._last_clipboard_watched_text = ''
        options = controller.new_options
        options.SetBoolean('watch_clipboard_for_watcher_urls', False)
        options.SetBoolean('watch_clipboard_for_other_recognised_urls', False)
        mixed = '\n'.join(['https://watch.example/1', 'https://post.example/1',
                           'https://gallery.example/1', 'http://file.example/a b.jpg#ignored',
                           'https://unlinked.example/1', 'https://unknown.example/1',
                           'http rubbish', 'HTTPS://file.example/upper.jpg'])
        steps = [
            ('disabled', mixed, None),
            ('enable watchers', mixed, 'watch_clipboard_for_watcher_urls'),
            ('unchanged', mixed, None),
            ('enable other urls', mixed, 'watch_clipboard_for_other_recognised_urls'),
            ('unchanged again', mixed, None),
            ('empty', '', None),
            ('repeat after empty', mixed, None),
            ('disable watchers', mixed, 'watch_clipboard_for_watcher_urls'),
            ('trim lines', '\ufeff https://file.example/2.jpg \r\nhttps://unknown.example/2\n', None),
            ('unavailable', None, None),
            ('same after unavailable', '\ufeff https://file.example/2.jpg \r\nhttps://unknown.example/2\n', None),
            ('access failure', RuntimeError('test clipboard access failure'), None),
        ]
        recorded = []
        try:
            for name, text, flip in steps:
                calls.clear()
                messages.clear()
                clipboard[0] = text
                if flip:
                    gui._FlipClipboardWatcher(flip)
                gui.REPEATINGClipboardWatcher()
                recorded.append({'name': name, 'text': text if not isinstance(text, Exception) else None,
                                 'failure': isinstance(text, Exception), 'reset': flip is not None,
                                 'watchers': options.GetBoolean('watch_clipboard_for_watcher_urls'),
                                 'other_recognised': options.GetBoolean('watch_clipboard_for_other_recognised_urls'),
                                 'routed': list(calls), 'messages': list(messages)})
            return {'classes': [c.GetSerialisableTuple() for c in classes],
                    'parser_key': parser.GetParserKey().hex(),
                    'linked': [c.GetClassKey().hex() for c in classes if c.GetName() != 'unlinked'],
                    'steps': recorded}
        finally:
            gui._notebook = old_notebook
            controller.GetClipboardText = old_clipboard
            HydrusData.ShowText = old_show_text
            gui._clipboard_watcher_repeating_job = old_job
            options.SetBoolean('watch_clipboard_for_watcher_urls', False)
            options.SetBoolean('watch_clipboard_for_other_recognised_urls', False)

    return controller.CallBlockingToQt(gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as output:
            json.dump(result, output)
        return
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'recorded.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as source:
            result = json.load(source)
    with open(OUT, 'w') as output:
        json.dump(result, output, indent=1, ensure_ascii=False)
        output.write('\n')
    print(f'wrote {OUT}: {len(result["steps"])} clipboard steps')


if __name__ == '__main__':
    main()
