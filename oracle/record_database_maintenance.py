#!/usr/bin/env python3
"""Record the Database menu's maintenance entries (regenerate, check and repair,
db maintenance) on the basic client: each callback's questions, button labels,
the "Which service?" choices, the database command it writes with its
arguments, and the popups that command's real run sends.

Each entry runs twice: once answering "forget it" (or cancelling the service
choice) and once accepting, choosing the first offered value. The GUI callbacks
run unchanged on the Qt thread; dialogs are replaced by recording functions.
Database writes are caught on the Qt thread and replayed synchronously on the
recorder's thread against the real database, so their popups can be recorded.
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

ENTRIES = [
    ("regenerate", "total pending count, in the pending menu", "_DeleteServiceInfo", {"only_pending": True}),
    ("regenerate", "tag storage mappings cache (all, with deferred siblings & parents calculation)", "_RegenerateTagMappingsCache", {}),
    ("regenerate", "tag storage mappings cache (just pending tags, instant calculation)", "_RegenerateTagPendingMappingsCache", {}),
    ("regenerate", "tag display mappings cache (all, deferred siblings & parents calculation)", "_RegenerateTagDisplayMappingsCache", {}),
    ("regenerate", "tag display mappings cache (just pending tags, instant calculation)", "_RegenerateTagDisplayPendingMappingsCache", {}),
    ("regenerate", "tag display mappings cache (missing file repopulation)", "_RepopulateTagDisplayMappingsCache", {}),
    ("regenerate", "tag siblings lookup cache", "_RegenerateTagSiblingsLookupCache", {}),
    ("regenerate", "tag parents lookup cache", "_RegenerateTagParentsLookupCache", {}),
    ("regenerate", "tag text search cache", "_RegenerateTagCache", {}),
    ("regenerate", "tag text search cache (subtags repopulation)", "_RepopulateTagCacheMissingSubtags", {}),
    ("regenerate", "tag text search cache (searchable subtag maps)", "_RegenerateTagCacheSearchableSubtagsMaps", {}),
    ("regenerate", "local hashes cache", "_RegenerateLocalHashCache", {}),
    ("regenerate", "local tags cache", "_RegenerateLocalTagCache", {}),
    ("regenerate", "service info numbers", "_DeleteServiceInfo", {}),
    ("regenerate", "similar files search tree", "_RegenerateSimilarFilesTree", {}),
    ("check and repair", "fix invalid tags", "_RepairInvalidTags", {}),
    ("check and repair", "fix logically inconsistent mappings", "_FixLogicallyInconsistentMappings", {}),
    ("check and repair", "repopulate truncated mappings tables", "_RepopulateMappingsTables", {}),
    ("check and repair", "resync combined deleted files", "_ResyncCombinedDeletedFiles", {}),
    ("check and repair", "resync tag mappings cache files", "_ResyncTagMappingsCacheFiles", {}),
    ("db maintenance", "analyze", "_AnalyzeDatabase", {}),
    ("db maintenance", "clear/fix orphan file records", "_ClearOrphanFileRecords", {}),
    ("db maintenance", "clear orphan URL mappings", "_ClearOrphanURLMappings", {}),
    ("db maintenance", "clear orphan tables", "_ClearOrphanTables", {}),
    ("db maintenance", "clear orphan hashed serialisables", "_ClearOrphanHashedSerialisables", {}),
    ("db maintenance", "get tables using definitions", "_GetTablesAndColumnsUsingDefinitions", {}),
]


def record(session):
    import hydrus_driver
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusData, HydrusExceptions
    from hydrus.client.gui import ClientGUIDialogsQuick as Q

    controller = session.controller
    gui = controller.gui
    services = controller.services_manager

    def plain(value):
        if isinstance(value, bytes):
            try:
                return "service:" + services.GetName(value)
            except Exception:
                return value.hex()
        if isinstance(value, (list, tuple)):
            return [plain(v) for v in value]
        if isinstance(value, dict):
            return {k: plain(v) for k, v in value.items()}
        if isinstance(value, (str, int, float, bool)) or value is None:
            return value
        return type(value).__name__

    current = {}
    pending_writes = []
    pending_threads = []

    def yes_no(parent, message, yes_label="yes", no_label="no", check_for_cancelled=False, **kwargs):
        current["asked"].append(dict(kind="yes_no", message=message, yes_label=yes_label, no_label=no_label))
        result = QW.QDialog.DialogCode.Accepted if current["accept"] else QW.QDialog.DialogCode.Rejected
        return (result, False) if check_for_cancelled else result

    def yes_yes_no(parent, message, yes_tuples=None, no_label="no", **kwargs):
        current["asked"].append(dict(kind="yes_yes_no", message=message,
                                     yes=[[label, plain(value)] for label, value in yes_tuples], no_label=no_label))
        if not current["accept"]:
            raise HydrusExceptions.CancelledException()
        return yes_tuples[current.get("yes_index", 0)][1]

    def buttons(parent, title, choice_tuples, message="", **kwargs):
        current["asked"].append(dict(kind="buttons", title=title, message=message,
                                     choices=[[text, plain(data), tooltip] for text, data, tooltip in choice_tuples]))
        if not current["accept"]:
            raise HydrusExceptions.CancelledException()
        return choice_tuples[current.get("choice_index", 0)][1]

    def write(command, *args, **kwargs):
        current["writes"].append(dict(command=command, args=plain(list(args)), kwargs=plain(dict(kwargs))))
        pending_writes.append((command, args, kwargs))

    def read(command, *args, **kwargs):
        current["reads"].append(dict(command=command, args=plain(list(args))))
        try:
            return original_read(command, *args, **kwargs)
        except Exception as error:
            current["read_error"] = type(error).__name__
            raise

    def to_thread(callable, *args, **kwargs):
        pending_threads.append((callable, args, kwargs))

    def pub(topic, *args, **kwargs):
        if topic in ("message", "modal_message") and all(args[0] is not job for job in current["popups"]):
            current["popups"].append(args[0])
        elif topic == "clipboard":
            current["clipboard"] = dict(kind=args[0], lines=len(args[1].splitlines()), first=args[1].splitlines()[:3])
        return original_pub(topic, *args, **kwargs)

    original = (Q.GetYesNo, Q.GetYesYesNo, Q.SelectFromListButtons, controller.Write,
                controller.Read, controller.CallToThread, controller.pub)
    original_read, original_pub = controller.Read, controller.pub
    original_write_sync = controller.WriteSynchronous

    events = []
    try:
        Q.GetYesNo, Q.GetYesYesNo, Q.SelectFromListButtons = yes_no, yes_yes_no, buttons
        controller.Write, controller.Read, controller.CallToThread, controller.pub = write, read, to_thread, pub
        runs = []
        for menu, label, method, kwargs in ENTRIES:
            runs.append((menu, label, method, kwargs, dict(accept=False)))
            runs.append((menu, label, method, kwargs, dict(accept=True)))
        # the analyze question's second yes answer, and choosing a single service
        runs.append(("db maintenance", "analyze", "_AnalyzeDatabase", {}, dict(accept=True, yes_index=1)))
        runs.append(("regenerate", "tag text search cache", "_RegenerateTagCache", {}, dict(accept=True, choice_index=1)))
        runs.append(("check and repair", "fix logically inconsistent mappings", "_FixLogicallyInconsistentMappings", {}, dict(accept=True, choice_index=1)))
        for menu, label, method, kwargs, answers in runs:
            current = dict(menu=menu, label=label, method=method, call_kwargs=kwargs,
                           asked=[], writes=[], reads=[], popups=[], **answers)
            pending_writes.clear()
            pending_threads.clear()

            def qt():
                try:
                    getattr(gui, method)(**kwargs)
                except HydrusExceptions.CancelledException:
                    current["cancelled"] = True
                except Exception as error:
                    current["error"] = type(error).__name__

            controller.CallBlockingToQt(gui, qt)
            for command, args, write_kwargs in list(pending_writes):
                original_write_sync(command, *args, **write_kwargs)
            for callable, args, thread_kwargs in list(pending_threads):
                callable(*args, **thread_kwargs)
            time.sleep(0.2)
            current["popups"] = [hydrus_driver.describe_popup(job) for job in current["popups"]]
            current["ran"] = [command for command, _, _ in pending_writes]
            events.append(current)
    finally:
        (Q.GetYesNo, Q.GetYesYesNo, Q.SelectFromListButtons, controller.Write,
         controller.Read, controller.CallToThread, controller.pub) = original
    tag_services = [[s.GetName(), s.GetServiceType()] for s in services.GetServices((5, 0))]
    return dict(tag_services=tag_services, events=events)


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
    output = os.path.join(HERE, "fixtures/database_maintenance.json")
    with open(output, "w") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")
    print("wrote " + output)


if __name__ == "__main__":
    main()
