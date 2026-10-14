#!/usr/bin/env python3
"""Help > debug > memory actions > run fast / slow memory maintenance, on the real client.

The real Help QActions are triggered. The fast one publishes
`memory_maintenance_pulse`, which every cache subscribes to: the real image and
thumbnail caches (primed with scripted entries under a held monotonic clock)
trim what is over their limit and what has idled past their timeout. The slow
one does not touch them; it deletes closed pages over an hour old, but only when
the last page change was over 30 minutes ago (`ClientController.MaintainMemorySlow`).
"""
import json
import sys
import tempfile
import time
import types
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
MINUTE = 60
# The scripted entries, a to d, one second apart: their sizes in each cache
# and each cache's limit (bytes). They are the sizes of
# oracle/fixtures/image_cache/w20.png, w14.png, c.png and w3.png (20, 14, 10 and
# 3 pixels wide, one high) as hydrus-rs accounts them in both its image cache
# (the pre-decode RGB estimate, width x height x 3) and its thumbnail cache
# (the thumbnail's RGB bytes), so that the GUI test can replay the recording
# with real files. The limit lets the first three entries sit together, the
# fourth push the oldest out, and the three left stay under it.
SIZES = {
    'image': dict(a=60, b=42, c=30, d=9),
    'thumbnail': dict(a=60, b=42, c=30, d=9),
}
LIMITS = {'image': 125, 'thumbnail': 125}


def record(session):
    def drive():
        from qtpy import QtWidgets as W
        from hydrus.core import HydrusTime
        from hydrus.client.caches import ClientCachesBase as Base
        c = session.controller
        gui = c.gui

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        class Stub(Base.CacheableObject):
            def __init__(self, size):
                self.size = size

            def GetEstimatedMemoryFootprint(self):
                return self.size

            def IsFinishedLoading(self):
                return True

        topics = []
        original_pub = c.pub

        def pub(topic, *args, **kwargs):
            topics.append(topic)
            return original_pub(topic, *args, **kwargs)

        clock = [0.0]
        now = [int(time.time())]
        original_time = Base.time
        originals = (HydrusTime.GetNow, HydrusTime.GetNowMS)
        menu, label = gui._InitialiseMenuInfoHelp()
        fast = find(menu, ['debug', 'memory actions', 'run fast memory maintenance'])
        slow = find(menu, ['debug', 'memory actions', 'run slow memory maintenance'])
        out = dict(
            sizes=SIZES, limits=LIMITS,
            fast=dict(path=[label.replace('&', ''), 'debug', 'memory actions', fast.text()], tooltip=fast.toolTip()),
            slow=dict(path=[label.replace('&', ''), 'debug', 'memory actions', slow.text()], tooltip=slow.toolTip()),
        )
        caches = {'image': c.images_cache._data_cache, 'thumbnail': c.thumbnails_cache._data_cache}
        saved = {name: (cache.GetSizeLimit(), cache._timeout) for name, cache in caches.items()}

        def trigger(action):
            topics.clear()
            action.trigger()
            W.QApplication.processEvents()
            # the pubsub thread delivers the pulse
            for _ in range(15):
                time.sleep(0.1)
                W.QApplication.processEvents()
            return list(topics)

        def state(name, **extra):
            return dict(name=name, now=clock[0], **extra, **{
                n: dict(keys=[str(k) for k in cache.GetAllKeys()], bytes=cache._total_estimated_memory_footprint)
                for n, cache in caches.items()})

        try:
            c.pub = pub
            Base.time = types.SimpleNamespace(monotonic=lambda: clock[0])
            for name, cache in caches.items():
                cache.Clear()
                cache.SetCacheSizeAndTimeout(LIMITS[name], 300)
            for key, at in [('a', 0), ('b', 1), ('c', 2), ('d', 3)]:
                clock[0] = at
                for name, cache in caches.items():
                    cache.AddData(key, Stub(SIZES[name][key]))
            steps = [state('primed')]
            steps[-1]['pubs'] = []
            for name, action in [('slow', slow), ('fast', fast)]:
                pubs = trigger(action)
                steps.append(dict(state('after ' + name), pubs=pubs))
            clock[0] = 302.5
            for name, action in [('slow', slow), ('fast', fast)]:
                pubs = trigger(action)
                steps.append(dict(state('past the timeout, after ' + name), pubs=pubs))
            out['caches'] = steps

            # closed pages: the slow one deletes those over an hour old, if the last page change was over 30 minutes ago
            HydrusTime.GetNow = lambda: now[0]
            HydrusTime.GetNowMS = lambda: now[0] * 1000
            destroyed = []

            class Page(W.QWidget):
                def __init__(self, name):
                    super().__init__()
                    self.page_name = name

                def GetNameForMenu(self):
                    return self.page_name

                def CleanBeforeDestroy(self):
                    destroyed.append(self.page_name)

            scenarios = []
            for name, closed_ago, last_change_ago in [
                ('old pages, changed long ago', [61, 30, 90], 31),
                ('old pages, changed recently', [61, 30, 90], 29),
                ('nothing old enough', [59, 30], 31),
                ('exactly an hour', [60], 31),
            ]:
                destroyed.clear()
                gui._closed_pages = [(now[0] - m * MINUTE, Page(f'closed {m}')) for m in closed_ago]
                c.SetTimestampMS('last_page_change', (now[0] - last_change_ago * MINUTE) * 1000)
                pubs = trigger(slow)
                scenarios.append(dict(
                    name=name,
                    closed_minutes_ago=closed_ago,
                    last_change_minutes_ago=last_change_ago,
                    published_delete=('delete_old_closed_pages' in pubs),
                    kept=[page.page_name for _, page in gui._closed_pages],
                    destroyed=sorted(destroyed),
                    change_touched=c.GetTimestampMS('last_page_change') >= now[0] * 1000,
                ))
                for _, page in gui._closed_pages:
                    page.deleteLater()
            # (the fast one does not delete closed pages)
            destroyed.clear()
            gui._closed_pages = [(now[0] - 90 * MINUTE, Page('closed 90'))]
            c.SetTimestampMS('last_page_change', (now[0] - 90 * MINUTE) * 1000)
            trigger(fast)
            out['fast_leaves_closed_pages'] = dict(kept=[p.page_name for _, p in gui._closed_pages], destroyed=list(destroyed))
            gui._closed_pages = []
            out['closed_pages'] = scenarios
            return out
        finally:
            c.pub = original_pub
            Base.time = original_time
            HydrusTime.GetNow, HydrusTime.GetNowMS = originals
            gui._closed_pages = []
            for name, cache in caches.items():
                cache.Clear()
                cache.SetCacheSizeAndTimeout(*saved[name])
            menu.deleteLater()
            W.QApplication.processEvents()

    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        database = record_api.unpack_fixture('basic')
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(database, record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/debug_memory_maintenance.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_memory_maintenance.json')


if __name__ == '__main__':
    main()
