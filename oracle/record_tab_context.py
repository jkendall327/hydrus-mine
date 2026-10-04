#!/usr/bin/env python3
"""Record the real PagesNotebook sort/move methods on deterministic Qt tabs.

Stand-in pages expose the reference summaries; the real notebook methods order
and move them in an offscreen QTabWidget, retaining its current widget. Includes
file-count importer tie breaks, lexical names and stable equal-size/name ties.
"""
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from qtpy import QtWidgets as QW
from hydrus.client.gui.pages.ClientGUIPages import PagesNotebook

app = QW.QApplication([])
rows = [
    {'name': 'page 10', 'files': 2, 'progress': [1, 7], 'size': 80},
    {'name': 'page 2', 'files': 2, 'progress': [2, 7], 'size': 10},
    {'name': 'Alpha', 'files': 5, 'progress': [0, 0], 'size': 80},
    {'name': 'page 2', 'files': 3, 'progress': [0, 0], 'size': 10},
    {'name': 'page 2', 'files': 2, 'progress': [2, 7], 'size': 10},
    {'name': 'alpha', 'files': 0, 'progress': [0, 0], 'size': 0},
]

class Page(QW.QWidget):
    def __init__(self, index):
        super().__init__()
        self.index = index
    def GetName(self): return rows[self.index]['name']
    def GetNumFileSummary(self):
        r = rows[self.index]
        return r['files'], tuple(r['progress'])
    def GetTotalFileSize(self): return rows[self.index]['size']

class Notebook(QW.QTabWidget):
    _SortPagesSetPages = PagesNotebook._SortPagesSetPages
    def GetPages(self): return [self.widget(i) for i in range(self.count())]
    def UpdatePreviousPageIndex(self): pass
    def __init__(self):
        super().__init__()
        self.layoutChanged = self.Signal()
        for i, r in enumerate(rows): self.addTab(Page(i), r['name'])
        self.setCurrentIndex(3)
    class Signal:
        def emit(self, *_): pass

record = {'pages': rows, 'selected': 3, 'sorts': [], 'moves': []}
for by, method in [('files', PagesNotebook._SortPagesByFileCount),
                   ('size', PagesNotebook._SortPagesByFileSize),
                   ('name', PagesNotebook._SortPagesByName)]:
    for ascending in [False, True]:
        notebook = Notebook()
        method(notebook, 'asc' if ascending else 'desc')
        record['sorts'].append({'by': by, 'ascending': ascending,
            'order': [p.index for p in notebook.GetPages()],
            'selected': notebook.currentWidget().index})
for movement, kwargs in [('first', {'new_index': 0}), ('left', {'delta': -1}),
                          ('right', {'delta': 1}), ('last', {'new_index': 5})]:
    for index in [0, 2, 3, 5]:
        notebook = Notebook()
        PagesNotebook._ShiftPage(notebook, index, **kwargs)
        record['moves'].append({'movement': movement, 'index': index,
            'order': [p.index for p in notebook.GetPages()],
            'selected': notebook.currentWidget().index})
Path(__file__).with_name('fixtures').joinpath('tab_context.json').write_text(json.dumps(record, indent=2)+'\n')
