#!/usr/bin/env python3
"""Record real Qt clear/cull questions and the actual viewing-statistics DB rules.

The GUI callbacks execute on a copied basic client's Qt thread. Only database
dispatch is intercepted to route its exact commands to the actual reference DB
module on an isolated SQLite corpus; no conversion or culling rule is replaced.
"""
import json
import os
import sqlite3
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientConstants as CC
        from hydrus.client.db.ClientDBFilesViewingStats import ClientDBFilesViewingStats
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui import ClientGUIDialogsMessage as M

        controller = session.controller
        options = controller.new_options
        keys = ["file_viewing_statistics_" + name + "_time_ms" for name in (
            "media_min", "media_max", "preview_min", "preview_max")]
        initial = [options.GetNoneableInteger(key) for key in keys]
        conn = sqlite3.connect(":memory:")
        conn.execute("CREATE TABLE file_viewing_stats (hash_id INTEGER, canvas_type INTEGER, last_viewed_timestamp_ms INTEGER, views INTEGER, viewtime_ms INTEGER, PRIMARY KEY(hash_id,canvas_type))")
        db = ClientDBFilesViewingStats(conn.cursor())
        rows = [[1, CC.CANVAS_MEDIA_VIEWER, 123000, 100, 100000],
                [2, CC.CANVAS_MEDIA_VIEWER, 456000, 10, 100000000],
                [3, CC.CANVAS_PREVIEW, 789000, 100, 100000],
                [4, CC.CANVAS_PREVIEW, 999000, 10, 100000000],
                [5, CC.CANVAS_CLIENT_API, 111000, 100, 100000000],
                [6, CC.CANVAS_MEDIA_VIEWER, 222000, 2, 4000],
                [7, CC.CANVAS_PREVIEW, 333000, 2, 10000],
                [8, CC.CANVAS_MEDIA_VIEWER, 444000, 0, 12345]]

        def state():
            return [list(row) for row in conn.execute("SELECT * FROM file_viewing_stats ORDER BY hash_id,canvas_type")]

        def seed():
            conn.execute("DELETE FROM file_viewing_stats")
            conn.executemany("INSERT INTO file_viewing_stats VALUES(?,?,?,?,?)", rows)

        old_yes, old_info, old_write = Q.GetYesNo, M.ShowInformation, controller.WriteSynchronous
        current = {}

        def yes(parent, text, **kwargs):
            current["asked"].append(dict(message=text, **kwargs))
            return QW.QDialog.DialogCode.Accepted if current["accept"] else QW.QDialog.DialogCode.Rejected

        def info(parent, text, **kwargs):
            current["information"].append(dict(message=text, **kwargs))

        def write(command, *args, **kwargs):
            current["commands"].append(command)
            if command == "cull_file_viewing_statistics":
                db.CullFileViewingStatistics()
            elif command == "content_updates":
                updates = [u for _, us in args[0].IterateContentUpdates() for u in us]
                current["updates"] = [list(u.ToTuple()) for u in updates]
                db.ClearAllStats()
            else:
                raise AssertionError(command)

        Q.GetYesNo, M.ShowInformation, controller.WriteSynchronous = yes, info, write
        events = []
        try:
            for operation, accept, bounds in [
                ("cull", False, [2000, 60000, 5000, 10000]),
                ("cull", True, [2000, 60000, 5000, 10000]),
                ("cull", True, [None, None, None, None]),
                ("cull", True, [0, 0, 0, 0]),
                ("cull", True, [60001, 60000, 5000, 10000]),
                ("cull", True, [2000, 60000, 10001, 10000]),
                ("clear", False, [2000, 60000, 5000, 10000]),
                ("clear", True, [2000, 60000, 5000, 10000]),
            ]:
                seed()
                for key, value in zip(keys, bounds):
                    options.SetNoneableInteger(key, value)
                current = dict(operation=operation, accept=accept, bounds=bounds,
                               asked=[], information=[], commands=[], before=state())
                try:
                    if operation == "clear":
                        controller.gui._ClearFileViewingStats()
                    else:
                        controller.gui._CullFileViewingStats()
                except Exception as error:
                    current["error"] = str(error)
                current["after"] = state()
                events.append(current)
        finally:
            Q.GetYesNo, M.ShowInformation, controller.WriteSynchronous = old_yes, old_info, old_write
            for key, value in zip(keys, initial):
                options.SetNoneableInteger(key, value)
            conn.close()
        return dict(initial_bounds=initial, rows=rows, events=events)

    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        value = hydrus_driver.run_client(record_api.unpack_fixture("basic"), record)
        with open(output, "w") as stream:
            json.dump(value, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        output = os.path.join(directory, "recording.json")
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), "--child", output)
        with open(output) as stream:
            value = json.load(stream)
    output = os.path.join(HERE, "fixtures/viewing_maintenance.json")
    with open(output, "w") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")
    print("wrote " + output)


if __name__ == "__main__":
    main()
