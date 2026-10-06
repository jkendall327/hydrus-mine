#!/usr/bin/env python3
"""Record Database > set a password on the basic client: the texts asked, the
clear question, the mismatch warning and what is stored, for scripted answers
(cancel, blank then no/yes, matching and mismatched entries)."""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

SCRIPTS = [
    ("cancel", [None], None),
    ("blank, keep", [""], False),
    ("blank, clear", [""], True),
    ("matching", ["hunter2", "hunter2"], None),
    ("mismatched", ["hunter2", "hunter3"], None),
    ("second cancelled", ["hunter2", None], None),
]


def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusExceptions
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui import ClientGUIDialogsMessage as M

        controller = session.controller
        current = {}

        def enter(win, message, **kwargs):
            current["asked"].append(dict(kind="text", message=message,
                                         allow_blank=kwargs.get("allow_blank", False),
                                         title=kwargs.get("title", "Enter Text")))
            answer = current["texts"].pop(0)
            if answer is None:
                raise HydrusExceptions.CancelledException()
            return answer

        def yes_no(win, message, **kwargs):
            current["asked"].append(dict(kind="yes_no", message=message,
                                         yes_label=kwargs.get("yes_label", "yes"), no_label=kwargs.get("no_label", "no")))
            return QW.QDialog.DialogCode.Accepted if current["yes"] else QW.QDialog.DialogCode.Rejected

        def critical(win, title, message):
            current["asked"].append(dict(kind="critical", title=title, message=message))

        old = (Q.EnterText, Q.GetYesNo, M.ShowCritical)
        Q.EnterText, Q.GetYesNo, M.ShowCritical = enter, yes_no, critical
        events = []
        try:
            for name, texts, yes in SCRIPTS:
                current = dict(name=name, texts=list(texts), yes=yes, asked=[])
                controller.WriteSynchronous("set_password", "before")
                controller.gui._SetPassword()
                import time
                time.sleep(0.5)
                stored = controller.Read("options")["password"]
                current["stored"] = None if stored is None else stored.hex()
                del current["texts"]
                events.append(current)
        finally:
            Q.EnterText, Q.GetYesNo, M.ShowCritical = old
            controller.WriteSynchronous("set_password", None)
        import hashlib
        return dict(before_hash=hashlib.sha256(b"before").hexdigest(), events=events)

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
    output = os.path.join(HERE, "fixtures/set_password.json")
    with open(output, "w") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")
    print("wrote " + output)


if __name__ == "__main__":
    main()
