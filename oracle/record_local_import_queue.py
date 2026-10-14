#!/usr/bin/env python3
"""Record "import now" on the reference's "review files to import" dialog.

In the running client, on the `basic` fixture, the real
`ReviewLocalFileImports` panel parses a folder holding a good png, an exact
copy of it, a sidecar-looking text file and an unsupported file. With
"delete original files after successful import" unticked and then ticked,
"import now" is pressed with the client's `new_hdd_import` publication noted
(its paths, import options container, metadata routers, additional tags and
delete flag), then the page it asks for is made for real and left to import.
Recorded for each run: the publication, the new page's name, the file seeds'
statuses once done, and which of the files are still on disk.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_local_import_queue.py
       (writes fixtures/local_import_queue.json)
"""
import json, os, shutil, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, 'fixtures', 'local_import_queue.json')
MEDIA = os.path.join(HERE, 'fixtures', 'media')


def record(session):
    controller = session.controller
    gui = controller.gui
    from hydrus.client.gui.panels import ClientGUILocalFileImports

    def qt(f, *args):
        return controller.CallBlockingToQt(gui, f, *args)

    out = {'runs': []}
    for delete in (False, True):
        work = tempfile.mkdtemp()
        names = ['first.png', 'second_copy.png', 'notes.txt', 'Thumbs.db']
        shutil.copy(os.path.join(MEDIA, 'apng_rgba.png'), os.path.join(work, names[0]))
        shutil.copy(os.path.join(MEDIA, 'apng_rgba.png'), os.path.join(work, names[1]))
        with open(os.path.join(work, names[2]), 'w') as f:
            f.write('just some text')
        with open(os.path.join(work, names[3]), 'wb') as f:
            f.write(b'not a picture')
        paths = [os.path.join(work, n) for n in names]

        panel = qt(lambda: ClientGUILocalFileImports.ReviewLocalFileImports(gui))
        qt(panel._AddPathsToList, paths)
        for _ in range(100):
            time.sleep(0.1)
            if qt(lambda: panel._unparsed_paths_queue.empty() and panel._parsed_paths_queue.empty()):
                break
        time.sleep(0.5)
        if delete:
            def tick():
                panel._delete_after_success.setChecked(True)
                panel.EventDeleteAfterSuccessCheck()
            qt(tick)

        published = []
        real_pub = controller.pub

        def noted(topic, *args, **kwargs):
            if topic == 'new_hdd_import':
                (good, container, routers, tags, delete_flag) = args
                published.append({
                    'paths': [os.path.basename(p) for p in good],
                    'routers': len(routers),
                    'additional_tags': {os.path.basename(k): dict(v) for k, v in tags.items()},
                    'delete_after_success': delete_flag,
                })
                return
            return real_pub(topic, *args, **kwargs)

        controller.pub = noted
        try:
            qt(panel._DoImport)
        finally:
            controller.pub = real_pub
        run = {'delete_after_success': delete, 'publication': published}
        good = [os.path.join(work, n) for n in published[0]['paths']]
        pages_before = qt(lambda: len(gui._notebook.GetMediaPages()))
        qt(gui.NewPageImportHDD, good, None, [], {}, published[0]['delete_after_success'])
        page = qt(lambda: gui._notebook.GetCurrentMediaPage())
        run['page_name'] = qt(lambda: page.GetName())
        run['pages_added'] = qt(lambda: len(gui._notebook.GetMediaPages())) - pages_before
        hdd = qt(lambda: page.GetPageManager().GetVariable('hdd_import'))
        for _ in range(200):
            time.sleep(0.1)
            (value, rng) = hdd.GetValueRange()
            if rng > 0 and value == rng:
                break
        time.sleep(1.0)
        seeds = hdd.GetFileSeedCache().GetFileSeeds()
        run['seeds'] = [[os.path.basename(s.file_seed_data), s.status] for s in seeds]
        run['on_disk'] = [n for n in names if os.path.exists(os.path.join(work, n))]
        out['runs'].append(run)
        qt(panel.deleteLater)
        qt(page.deleteLater)
    return out


def child(out):
    import hydrus_driver, record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f, ensure_ascii=False)


def main():
    sys.path.insert(0, HERE)
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as work:
        path = os.path.join(work, 'r.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(json.dumps(result, indent=1))


if __name__ == '__main__':
    main()
