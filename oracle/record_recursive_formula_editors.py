#!/usr/bin/env python3
"""Record context/static/nested/zipper editors, recursive child test data and queues.
Uses real reference Qt controls for scalar fields, nested main/sub changes,
first-successful-example subformula test data, zipper add/edit/cancel/reorder/
delete, and fresh defaults in the six-kind chooser. New inputs are synthetic.
"""
import json
import os
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from hydrus.client.gui.parsing import ClientGUIParsingFormulae as G
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client import ClientStrings as S
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W, ClientGUIDialogsQuick as D
        from qtpy import QtWidgets as QW
        root = session.controller.gui
        out = []
        context = {'url': 'https://formula.example/post', 'note': ' first\n second '}
        processor = S.StringProcessor()
        processor.SetProcessingSteps([S.StringConverter(conversions=[(S.STRING_CONVERSION_APPEND_TEXT, '!')])])
        for collapse in (True, False):
            p = G.EditContextVariableFormulaPanel(root, collapse, P.ParseFormulaContextVariable(string_processor=processor), P.ParsingTestData(context, ['unused']))
            p._variable_name.setText('note')
            p._name.setText('context note')
            f = p.GetValue()
            out.append({'case': 'context_formula', 'collapse': collapse, 'context': context, 'variable': f.GetVariableName(), 'name': f.GetName(), 'before': P.ParseFormulaContextVariable(variable_name='note').Parse(context, '', collapse), 'results': f.Parse(context, '', collapse)})
            p._variable_name.setText('absent')
            out.append({'case': 'context_missing', 'results': p.GetValue().Parse(context, '', collapse)})
            p.deleteLater()
            p = G.EditStaticFormulaPanel(root, collapse, P.ParseFormulaStatic(string_processor=processor), P.ParsingTestData(context, ['unused']))
            p._static_text.setText(' first\n second ')
            p._num_to_do.setValue(3)
            p._name.setText('three constants')
            f = p.GetValue()
            out.append({'case': 'static_formula', 'collapse': collapse, 'text': f.GetStaticText(), 'count': f.GetNumToDo(), 'name': f.GetName(), 'results': f.Parse(context, '', collapse), 'minimum': p._num_to_do.minimum(), 'maximum': p._num_to_do.maximum()})
            p.deleteLater()
        text = '<html><script>{"posts":["alpha","beta"]}</script></html>'
        main = P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='script')], content_to_fetch=P.HTML_CONTENT_STRING)
        sub = P.ParseFormulaJSON(parse_rules=[(P.JSON_PARSE_RULE_TYPE_DICT_KEY, S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='posts')), (P.JSON_PARSE_RULE_TYPE_ALL_ITEMS, None)])
        nested = P.ParseFormulaNested(main_formula=main, sub_formula=sub, string_processor=processor)
        p = G.EditNestedFormulaPanel(root, True, nested, P.ParsingTestData(context, [text]))
        p._test_panel._SetExampleData(text)
        p._name.setText('embedded json')
        out.append({'case': 'nested_formula', 'text': text, 'context': context, 'name': p.GetValue().GetName(), 'results': p.GetValue().Parse(context, text, True), 'sub_texts': p._GetSubTestData().texts})
        # The selector owns staged values. Closing a child without acceptance
        # does not replace it; acceptance replaces exactly the selected child.
        p._main_formula_panel._current_formula = P.ParseFormulaStatic(static_text='{"posts":["changed"]}')
        out.append({'case': 'nested_main_edit', 'sub_texts': p._GetSubTestData().texts, 'results': p.GetValue().Parse(context, text, True)})
        p._sub_formula_panel._current_formula = P.ParseFormulaContextVariable(variable_name='url')
        out.append({'case': 'nested_sub_edit', 'results': p.GetValue().Parse(context, text, True)})
        p.deleteLater()
        for label, examples in [('first_nonempty', ['<html></html>', text]), ('error_fallback', ['not json', '{"posts":"later"}']), ('empty', ['<html></html>'])]:
            first = main if label != 'error_fallback' else P.ParseFormulaJSON()
            p = G.EditNestedFormulaPanel(root, True, P.ParseFormulaNested(main_formula=first), P.ParsingTestData(context, examples))
            # Supply the documented child-data callback with multiple examples;
            # the ordinary raw-data panel itself displays only the first text.
            p._test_panel.GetTestData = lambda examples=examples: P.ParsingTestData(context,examples)
            data = p._GetSubTestData()
            out.append({'case': 'nested_sub_test', 'type': label, 'texts': examples, 'sub_texts': data.texts, 'context': data.parsing_context})
            p.deleteLater()
        answers = []
        class Dialog(QW.QDialog):
            def __init__(self, parent, *args, **kwargs): super().__init__(parent)
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def SetPanel(self, panel): self.panel = panel
            def exec(self):
                answer = answers.pop(0)
                if answer is None: return QW.QDialog.DialogCode.Rejected
                self.panel._current_formula = P.ParseFormulaStatic(static_text=answer)
                return QW.QDialog.DialogCode.Accepted
        W.DialogEdit = Dialog
        D.GetYesNo = lambda *args, **kwargs: QW.QDialog.DialogCode.Accepted
        p = G.EditZipperFormulaPanel(root, True, P.ParseFormulaZipper(formulae=[P.ParseFormulaStatic(static_text='first')]), P.ParsingTestData(context, [text]))
        q = p._formulae
        for action, answer in [('add','second'), ('edit','replacement'), ('up',None), ('down',None), ('cancel_add',None), ('cancel_edit',None), ('delete',None)]:
            if action in ('add','edit','cancel_add','cancel_edit'):
                answers.append(answer)
                if action in ('edit','cancel_edit'):
                    q._listbox.clearSelection()
                    q._listbox.item(1).setSelected(True)
                    q._Edit()
                else: q._Add()
            elif action == 'up': q._Up()
            elif action == 'down': q._Down()
            else: q._Delete()
            out.append({'case': 'zipper_queue', 'action': action, 'texts': [f.GetStaticText() for f in q.GetData()], 'selected': [i for i in range(q._listbox.count()) if q._listbox.item(i).isSelected()]})
        p.deleteLater()
        formulae = [P.ParseFormulaStatic(static_text='left', num_to_do=2), P.ParseFormulaContextVariable(variable_name='url')]
        p = G.EditZipperFormulaPanel(root, True, P.ParseFormulaZipper(formulae=formulae, string_processor=processor), P.ParsingTestData(context, [text]))
        p._sub_phrase.setText('\\1 -> \\2')
        p._name.setText('paired')
        f = p.GetValue()
        out.append({'case': 'zipper_formula', 'phrase': f.GetSubstitutionPhrase(), 'name': f.GetName(), 'context': context, 'results': f.Parse(context, text, True)})
        p.deleteLater()
        for kind, label in [('nested','NESTED'),('zipper','ZIPPER'),('context','CONTEXT VARIABLE'),('static','STATIC')]:
            D.SelectFromList = lambda parent, title, tuples, label=label: next(f for text,f in tuples if text == 'change to a new '+label+' formula')
            p = G.EditFormulaPanel(root, P.ParseFormulaHTML(name='reset', string_processor=processor), lambda: P.ParsingTestData(context,[text]))
            p._ChangeFormulaType()
            f = p.GetValue()
            row = {'case': 'change_recursive_type', 'type': kind, 'name': f.GetName(), 'processing': f.GetStringProcessor().GetProcessingStrings()}
            if kind == 'context': row['variable'] = f.GetVariableName()
            if kind == 'static': row.update(text=f.GetStaticText(), count=f.GetNumToDo())
            if kind == 'zipper': row.update(phrase=f.GetSubstitutionPhrase(), children=len(f.GetFormulae()))
            if kind == 'nested': row.update(main_type=type(f.GetMainFormula()).__name__, sub_type=type(f.GetSubFormula()).__name__)
            out.append(row)
            p.deleteLater()
        # Retain a visual reference of the completed nested panel.
        p = G.EditNestedFormulaPanel(root, True, nested, P.ParsingTestData(context,[text]))
        p.resize(1000,700)
        p.show()
        QW.QApplication.processEvents()
        p.grab().save(os.path.join(HERE,'fixtures','recursive_formula_editors.png'))
        p.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def child(path):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(path, 'w') as f: json.dump(result, f, indent=2, ensure_ascii=False)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import tempfile
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp,'recording.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f: result = json.load(f)
    with open(os.path.join(HERE,'fixtures','recursive_formula_editors.json'),'w') as f:
        json.dump(result,f,indent=2,ensure_ascii=False)
        f.write('\n')
    print('recorded',len(result),'recursive/scalar formula states')


if __name__ == '__main__': main()
