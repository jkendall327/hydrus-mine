#!/usr/bin/env python3
"""Record the reference domain validation process's actual automatic header questions.
Capture each real JobStatus popup and answer it immediately, with stored validations.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

def record(session):
    def qt():
        from hydrus.client.networking import ClientNetworkingDomain as D, ClientNetworkingContexts as C
        controller = session.controller
        manager = D.NetworkDomainManager()
        contexts = [C.GLOBAL_NETWORK_CONTEXT, C.NetworkContext(2, 'example.com')]
        manager.SetNetworkContextsToCustomHeaderDicts({
            contexts[0]: {'X-Test': ('global-value', D.VALID_UNKNOWN, 'global reason')},
            contexts[1]: {'Authorization': ('token', D.VALID_UNKNOWN, 'login reason')}})
        out = {'questions': [], 'validations': []}
        old_pub = controller.pub
        def pub(topic, job, *args, **kwargs):
            if topic == 'message':
                out['questions'].append(job.GetIfHasVariable('popup_yes_no_question'))
                job.SetVariable('popup_yes_no_answer', len(out['questions']) == 1)
            else:
                old_pub(topic, job, *args, **kwargs)
        old_validation = manager.SetHeaderValidation
        def validate(context, key, value):
            out['validations'].append([context.ToString(), key, value])
            old_validation(context, key, value)
        manager.SetHeaderValidation = validate
        controller.pub = pub
        try:
            process = manager.GenerateValidationPopupProcess(contexts)
            process.Start()
            out['done'] = process.IsDone()
        finally:
            controller.pub = old_pub
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, qt)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output_path = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output_path, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'header_approval.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'header_approval.json'), 'w') as f:
        json.dump(result, f, indent=2)
        f.write('\n')
    print('recorded automatic pending-header questions')

if __name__ == '__main__': main()
