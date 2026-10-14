#!/usr/bin/env python3
"""Record `ClientGUIDialogsDocumentation.OpenDocumentation`, which every help
button and the Help menu's "open help" and "changelog" go through.

Against a help directory that has the page: what it opens (a `file:///` URL
of the local file, kept with its `#fragment`). Against none (the reference run
from source): the question it asks (`GetYesYesNo`: message, title, yes
choices, no label) and what each answer opens (the online page, the how-to-build
guide, or nothing for "forget it" and a cancelled question).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_documentation.py
       (writes oracle/fixtures/documentation.json)
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

PAGES = ['index.html', 'changelog.html#version_688', 'external_programs.html', 'getting_started_import_options.html']


def record(session):
    c = session.controller

    def drive():
        from hydrus.core import HydrusConstants as HC
        from hydrus.core import HydrusExceptions
        from hydrus.client.gui import ClientGUIDialogsDocumentation as D
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.executables import ClientGUIExecutableActions as X

        old = (HC.HELP_DIR, Q.GetYesYesNo, X.OpenExternallyURLDefault)
        opened, asked = [], []
        answer = [None]

        def ask(win, message, title='Are you sure?', yes_tuples=None, no_label='no'):
            asked.append({'message': message, 'title': title, 'yes': [t[0] for t in yes_tuples], 'no': no_label})
            if answer[0] is None:
                raise HydrusExceptions.CancelledException()
            return yes_tuples[answer[0]][1]

        def launch(win, url):
            opened.append(url)
            return True

        Q.GetYesYesNo, X.OpenExternallyURLDefault = ask, launch
        out = {'local': [], 'missing': []}
        try:
            with tempfile.TemporaryDirectory() as help_dir:
                HC.HELP_DIR = help_dir
                for page in PAGES:
                    with open(os.path.join(help_dir, page.split('#')[0]), 'w') as f:
                        f.write('<html></html>')
                for page in PAGES:
                    opened.clear(); asked.clear()
                    D.OpenDocumentation(c.gui, page)
                    out['local'].append({'page': page, 'opened': [u.replace(help_dir, '<help>') for u in opened], 'asked': list(asked)})
            with tempfile.TemporaryDirectory() as help_dir:
                HC.HELP_DIR = help_dir
                for page in PAGES:
                    for choice in (0, 1, None):
                        answer[0] = choice
                        opened.clear(); asked.clear()
                        D.OpenDocumentation(c.gui, page)
                        out['missing'].append({'page': page, 'answer': choice, 'asked': list(asked), 'opened': list(opened)})
            return out
        finally:
            HC.HELP_DIR, Q.GetYesYesNo, X.OpenExternallyURLDefault = old

    return c.CallBlockingToQt(c.gui, drive)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        import record_api
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
        return
    with tempfile.TemporaryDirectory() as work:
        output = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as f:
            result = json.load(f)
        with open(os.path.join(HERE, 'fixtures/documentation.json'), 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)


if __name__ == '__main__':
    main()
