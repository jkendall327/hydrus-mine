#!/usr/bin/env python3
"""Record closing a non-empty importer page (`confirm_non_empty_downloader_page_close`).

Fresh temporary `basic` client. In a real `PagesNotebook` one page of each
importer kind is made (urls, simple downloader, gallery, watcher, local
files) holding a few import objects (their file seed cache gets file seeds;
the import is paused so it is not "still importing"). For the option on and
off, `AskIfAbleToClose` is called as closing the page calls it, plain and for
a session close, and the question it would put to the user (the yes/no
dialog is answered by script, once yes and once no) is recorded, with
whether the page stayed open. An importer still importing is recorded too
(unpaused, never started). A search page and an empty importer page are
the controls.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'importer_page_close.json')
ITEMS = 3


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusExceptions
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.pages import ClientGUIPages, ClientGUIPageManager, ClientGUISidebarImporters as SI
    from hydrus.client.importing import ClientImportFileSeeds as FS, ClientImportWatchers as W
    c = session.controller
    gui = c.gui
    old_yesno = ClientGUIDialogsQuick.GetYesNo
    classes = [SI.SidebarImporterMultipleGallery, SI.SidebarImporterMultipleWatcher, SI.SidebarImporterSimpleDownloader, SI.SidebarImporterURLs, SI.SidebarImporterHDD]
    old_starts = [cls.Start for cls in classes]
    for cls in classes:
        cls.Start = lambda self: None

    def seeds(cache, n=ITEMS):
        cache.AddFileSeeds([FS.FileSeed(FS.FILE_SEED_TYPE_URL, f'https://example.com/{i}.jpg') for i in range(n)])

    def work():
        notebook = ClientGUIPages.PagesNotebook(gui, 'recording')
        context = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        pages = {}
        pages['search'] = notebook.NewPageQuery(context, page_name='search', select_page=False)
        for kind, make in [('urls', notebook.NewPageImportURLs), ('simple', notebook.NewPageImportSimpleDownloader),
                           ('gallery', notebook.NewPageImportGallery), ('watcher', notebook.NewPageImportMultipleWatcher)]:
            pages[kind] = make()
        hdd_manager = ClientGUIPageManager.CreatePageManagerImportHDD([], None if False else __import__('hydrus.client.importing.options.ImportOptionsContainer', fromlist=['x']).ImportOptionsContainer(), [], None, False)
        pages['local'] = notebook.NewPage(hdd_manager, select_page=False)
        pages['empty urls'] = notebook.NewPageImportURLs()
        # fill: (the import objects behind each page)
        imports = {}
        imports['urls'] = pages['urls']._page_manager.GetVariable('urls_import')
        imports['simple'] = pages['simple']._page_manager.GetVariable('simple_downloader_import')
        imports['local'] = pages['local']._page_manager.GetVariable('hdd_import')
        for kind in ['urls', 'simple', 'local']:
            seeds(imports[kind].GetFileSeedCache())
            (imports[kind].PausePlayFiles if kind == 'simple' else imports[kind].PausePlay)()
        multiple = pages['gallery']._page_manager.GetVariable('multiple_gallery_import')
        from hydrus.client.importing import ClientImportGallery as G
        gallery = G.GalleryImport(query='one', source_name='test', initial_search_urls=['https://example.com/search?q=one'], start_file_queue_paused=True, start_gallery_queue_paused=True)
        multiple._AddGalleryImport(gallery)
        seeds(gallery.GetFileSeedCache())
        multiple_watchers = pages['watcher']._page_manager.GetVariable('multiple_watcher_import')
        watcher = W.WatcherImport()
        multiple_watchers.AddWatcher(watcher)
        seeds(watcher.GetFileSeedCache())
        watcher.PausePlayFiles()
        watcher.PausePlayChecking()
        records = []
        for enabled in [True, False]:
            c.new_options.SetBoolean('confirm_non_empty_downloader_page_close', enabled)
            for kind in ['search', 'urls', 'simple', 'gallery', 'watcher', 'local', 'empty urls']:
                for for_session in [False, True]:
                    for accepted in [True, False]:
                        questions = []

                        def answer(parent, message, *args, **kwargs):
                            questions.append(message)
                            return QW.QDialog.DialogCode.Accepted if accepted else QW.QDialog.DialogCode.Rejected
                        ClientGUIDialogsQuick.GetYesNo = answer
                        error = None
                        try:
                            pages[kind].AskIfAbleToClose(for_session_close=for_session)
                        except HydrusExceptions.VetoException as e:
                            error = str(e)
                        records.append(dict(option=enabled, kind=kind, for_session=for_session, answer_yes=accepted, questions=questions, vetoed=error))
        # a page still importing asks whatever the option says
        working = notebook.NewPageImportURLs()
        urls_import = working._page_manager.GetVariable('urls_import')
        seeds(urls_import.GetFileSeedCache())
        for enabled in [True, False]:
            c.new_options.SetBoolean('confirm_non_empty_downloader_page_close', enabled)
            for accepted in [True, False]:
                questions = []

                def answer(parent, message, *args, **kwargs):
                    questions.append(message)
                    return QW.QDialog.DialogCode.Accepted if accepted else QW.QDialog.DialogCode.Rejected
                ClientGUIDialogsQuick.GetYesNo = answer
                error = None
                try:
                    working.AskIfAbleToClose()
                except HydrusExceptions.VetoException as e:
                    error = str(e)
                records.append(dict(option=enabled, kind='urls still importing', for_session=False, answer_yes=accepted, questions=questions, vetoed=error))
        return dict(items=ITEMS, records=records)
    try:
        return c.CallBlockingToQt(gui, work)
    finally:
        ClientGUIDialogsQuick.GetYesNo = old_yesno
        for cls, start in zip(classes, old_starts):
            cls.Start = start


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
