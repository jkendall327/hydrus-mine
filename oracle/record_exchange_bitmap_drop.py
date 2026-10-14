#!/usr/bin/env python3
"""Record the reference's clipboard bitmap and PNG file drops for exchanges.

Two real panels are driven unchanged:

* `ReviewDownloaderImport` (Network > downloader definitions > import): its
  `IngestClipboard` with a real bitmap on the Qt clipboard (a greyscale hydrus
  PNG, the same pixels as RGB and as RGBA, and an image that is not a hydrus
  PNG), its paste button and `ImportFromDragDrop` over a PNG, a JSON file and
  files that cannot load.
* `EditSubscriptionsPanel`'s list panel: `_ImportFromClipboard` with a bitmap,
  and `ImportFromDragDrop` (its yes/no question, its order of PNGs before JSON,
  its messages).

Dialogs are replaced by scripted answers that note what they were asked.
Outputs: fixtures/exchange_bitmap_drop.json and the three PNGs it uses.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
NOW = 1_700_000_000


def record(session):
    c = session.controller

    def work():
        from qtpy import QtGui as QG, QtWidgets as QW
        from hydrus.core import HydrusSerialisable as S, HydrusTime as T
        from hydrus.client import ClientSerialisable as PNG, ClientStrings
        from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as D, ClientGUISubscriptions as G
        from hydrus.client.gui.panels import ClientGUISerialisableImport as Import
        from hydrus.client.importing import ClientImportSubscriptions as Subs, ClientImportSubscriptionQuery as Query
        from hydrus.client.networking import ClientNetworkingGUG as GUG
        from hydrus.client.networking import ClientNetworkingURLClass as U
        from hydrus.core import HydrusConstants as HC

        old_now = T.GetNow
        T.GetNow = lambda: NOW
        messages, questions = [], []
        accepted = [True]

        def note(kind):
            return lambda owner, *args, **kw: messages.append([kind, *[str(a) for a in args]])

        real = (M.ShowInformation, M.ShowWarning, M.ShowCritical, D.GetYesNo)
        M.ShowInformation = note('information')
        M.ShowWarning = note('warning')
        M.ShowCritical = note('critical')

        def answer(owner, text, title='Are you sure?', **kw):
            questions.append({'message': text, 'title': title, 'accepted': accepted[0]})
            return QW.QDialog.DialogCode.Accepted if accepted[0] else QW.QDialog.DialogCode.Rejected

        D.GetYesNo = answer
        out_dir = os.path.join(HERE, 'fixtures')
        clipboard = QW.QApplication.clipboard()

        def take_messages():
            taken = list(messages)
            messages.clear()
            return taken

        def take_questions():
            taken = list(questions)
            questions.clear()
            return taken

        def put_image(image):
            clipboard.setImage(image)
            QW.QApplication.processEvents()

        def variants(path):
            grey = QG.QImage(path)
            rgb = grey.convertToFormat(QG.QImage.Format.Format_RGB888)
            rgba = grey.convertToFormat(QG.QImage.Format.Format_RGBA8888)
            # the other channels and the alpha differ from the first: the
            # reference reads only the first channel of what it is given
            varied = QG.QImage(grey.width(), grey.height(), QG.QImage.Format.Format_RGBA8888)
            for y in range(grey.height()):
                for x in range(grey.width()):
                    value = QG.QColor(grey.pixel(x, y)).red()
                    varied.setPixelColor(x, y, QG.QColor(value, (value * 7 + 40) % 256, 255 - value, 100 + (x + y) % 100))
            return {'grey': grey, 'rgb': rgb, 'rgba': rgba, 'varied': varied}

        try:
            # one downloader package and one subscription bundle, as hydrus PNGs
            gug = GUG.GalleryURLGenerator('bitmap gallery', url_template='https://bitmap.example/search/%tags%', replacement_phrase='%tags%')
            gug.SetGUGKeyAndName((bytes.fromhex('a3' * 32), gug.GetName()))
            package = S.SerialisableList([gug])
            package_png = os.path.join(out_dir, 'exchange_bitmap_package.png')
            PNG.DumpToPNG(512, package.DumpToNetworkBytes(), 'bitmap gallery', 'one gug', 'synthetic bitmap exchange', package_png)

            sub = Subs.Subscription('Bitmap artist', gug_key_and_name=(bytes.fromhex('11' * 32), 'synthetic gallery'))
            sub.SetPaused(True)
            header = Query.SubscriptionQueryHeader()
            header.SetQueryText('blue_eyes')
            header.SetDisplayName('blue artist')
            log = Query.SubscriptionQueryLogContainer(header.GetQueryLogContainerName())
            header.SyncToQueryLogContainer(sub.GetCheckerOptions(), log)
            sub.SetQueryHeaders([header])
            # the list's own export is the bundle its import takes
            copied = []
            old_pub = c.pub
            c.pub = lambda *args, **kw: copied.append(list(args)) if args and args[0] == 'clipboard' else None
            source = G.EditSubscriptionsPanel(c.gui, [sub])
            source._names_to_edited_query_log_containers[log.GetName()] = log
            try:
                source._subscriptions.SelectDatas(source._subscriptions.GetData())
                source._subscriptions_panel._ExportToClipboard()
            finally:
                c.pub = old_pub
                source.deleteLater()
            sub_bundle = S.CreateFromString(copied[-1][2])
            sub_png = os.path.join(out_dir, 'exchange_bitmap_subscriptions.png')
            PNG.DumpToPNG(512, sub_bundle.DumpToNetworkBytes(), 'subscriptions', 'one subscription', 'synthetic bitmap exchange', sub_png)

            # an image that is not a hydrus PNG
            plain = QG.QImage(32, 32, QG.QImage.Format.Format_RGB888)
            plain.fill(QG.QColor(200, 30, 30))
            plain_png = os.path.join(out_dir, 'exchange_bitmap_plain.png')
            plain.save(plain_png)

            out = {'package_json': json.loads(package.DumpToString())}

            # -- the downloader import: IngestClipboard
            ingest = {}
            for name, image in variants(package_png).items():
                put_image(image)
                payloads = Import.IngestClipboard()
                ingest[name] = [[description, json.loads(text)] for description, text in payloads]
            put_image(QG.QImage(plain_png))
            try:
                Import.IngestClipboard()
                ingest['plain'] = None
            except Exception as e:
                ingest['plain'] = str(e)
            clipboard.setText('not a package')
            QW.QApplication.processEvents()
            ingest['text'] = [list(p) for p in Import.IngestClipboard()]
            out['ingest_clipboard'] = ingest

            # -- the downloader import: paste and drop, with the manager reset between
            engine = c.network_engine
            domain = engine.domain_manager

            def gug_names():
                return sorted(g.GetName() for g in domain.GetGUGs())

            domain.SetGUGs([])
            domain.SetURLClasses([])
            domain.SetParsers([])
            importer = Import.ReviewDownloaderImport(c.gui, engine)
            importer._select_from_list.setChecked(False)
            cases = {}
            try:
                for name, image in variants(package_png).items():
                    domain.SetGUGs([])
                    put_image(image)
                    importer._Paste()
                    cases['paste_' + name] = dict(gugs=gug_names(), questions=take_questions(), messages=take_messages())
                domain.SetGUGs([])
                put_image(QG.QImage(plain_png))
                importer._Paste()
                cases['paste_plain'] = dict(gugs=gug_names(), questions=take_questions(), messages=take_messages())
                domain.SetGUGs([])
                json_path = os.path.join(out_dir, 'exchange_bitmap_package.json')
                with open(json_path, 'w') as f:
                    f.write(package.DumpToString())
                importer.ImportFromDragDrop([package_png])
                cases['drop_png'] = dict(gugs=gug_names(), questions=take_questions(), messages=take_messages())
                domain.SetGUGs([])
                importer.ImportFromDragDrop([json_path])
                cases['drop_json'] = dict(gugs=gug_names(), questions=take_questions(), messages=take_messages())
                domain.SetGUGs([])
                importer.ImportFromDragDrop([plain_png])
                cases['drop_plain_png'] = dict(gugs=gug_names(), questions=take_questions(), messages=take_messages())
                domain.SetGUGs([])
                missing = os.path.join(out_dir, 'exchange_bitmap_missing.png')
                importer.ImportFromDragDrop([missing])
                cases['drop_missing_png'] = dict(gugs=gug_names(), questions=take_questions(), messages=take_messages())
                domain.SetGUGs([])
                importer.ImportFromDragDrop([plain_png, package_png])
                cases['drop_bad_then_good'] = dict(gugs=gug_names(), questions=take_questions(), messages=take_messages())
                domain.SetGUGs([])
                upper = os.path.join(out_dir, 'exchange_bitmap_UPPER.PNG')
                with open(package_png, 'rb') as src, open(upper, 'wb') as dst:
                    dst.write(src.read())
                try:
                    importer.ImportFromDragDrop([upper])
                    cases['drop_upper_extension'] = dict(gugs=gug_names(), questions=take_questions(), messages=take_messages())
                finally:
                    os.remove(upper)
            finally:
                importer.deleteLater()
            out['downloader_import'] = cases

            # -- the subscription list
            panel = G.EditSubscriptionsPanel(c.gui, [])
            listing = panel._subscriptions
            exchange = panel._subscriptions_panel
            lists = {}
            try:
                def names():
                    return sorted(s.GetName() for s in listing.GetData())

                def reset():
                    listing.DeleteDatas(listing.GetData())

                for name, image in variants(sub_png).items():
                    reset()
                    put_image(image)
                    exchange._ImportFromClipboard()
                    lists['clipboard_' + name] = dict(names=names(), messages=take_messages(), questions=take_questions())
                reset()
                put_image(QG.QImage(plain_png))
                exchange._ImportFromClipboard()
                lists['clipboard_plain'] = dict(names=names(), messages=take_messages(), questions=take_questions())
                clipboard.setText('none')
                sub_json = os.path.join(out_dir, 'exchange_bitmap_subscriptions.json')
                with open(sub_json, 'w') as f:
                    f.write(sub_bundle.DumpToString())
                for label, paths, ok in [
                    ('drop_png_declined', [sub_png], False),
                    ('drop_png', [sub_png], True),
                    ('drop_json', [sub_json], True),
                    ('drop_png_and_json', [sub_json, sub_png], True),
                    ('drop_plain_png', [plain_png], True),
                    ('drop_bad_then_good', [plain_png, sub_png], True),
                    ('drop_missing_png', [os.path.join(out_dir, 'exchange_bitmap_missing.png')], True),
                ]:
                    reset()
                    accepted[0] = ok
                    exchange.ImportFromDragDrop(paths)
                    lists[label] = dict(names=names(), messages=take_messages(), questions=take_questions())
            finally:
                accepted[0] = True
                panel.deleteLater()
            out['subscription_list'] = lists
            out['subscription_json'] = json.loads(sub_bundle.DumpToString())
            return out
        finally:
            M.ShowInformation, M.ShowWarning, M.ShowCritical, D.GetYesNo = real
            T.GetNow = old_now

    return c.CallBlockingToQt(c.gui, work)


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
        # (paths in the messages are relative to the fixtures directory)
        import re
        text = json.dumps(result, ensure_ascii=False, indent=2).replace(
            json.dumps(os.path.join(HERE, 'fixtures'))[1:-1], '{dir}'
        )
        # (the reference's temporary copy of a file it reads is random)
        text = re.sub(r'/tmp/hydrus[0-9a-z_]+/serialised_png_[0-9a-z_]+', '{temp}', text)
        with open(os.path.join(HERE, 'fixtures/exchange_bitmap_drop.json'), 'w') as f:
            f.write(text + '\n')


if __name__ == '__main__':
    main()
