#!/usr/bin/env python3
"""Dump the thumbnail menu's manage > maintenance entries: each file
maintenance job in the reference's human order, with its menu label and
description (`ClientFilesMaintenance`)."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

from hydrus.client.files import ClientFilesMaintenance as M

out = [
    dict(code=job, label=M.regen_file_enum_to_str_lookup[job], description=M.regen_file_enum_to_description_lookup[job])
    for job in M.ALL_REGEN_JOBS_IN_HUMAN_ORDER
]
path = os.path.join(HERE, "fixtures/regen_jobs.json")
with open(path, "w") as stream:
    json.dump(out, stream, indent=2)
    stream.write("\n")
print("wrote " + path)
