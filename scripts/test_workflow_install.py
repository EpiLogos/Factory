#!/usr/bin/env python3
"""Run the installed Factory authoring front door in a disposable no-provider World.

Use an independently installed binary (--binary), not cargo run. This verifies
packaging, discovery, generic Commission and immutable source navigation only;
it deliberately does not claim ACP/model execution or creative competence.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error('Factory must be an installed regular executable')
    fingerprint = hashlib.sha256(binary.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory(prefix='factory-installed-authoring-') as temporary:
        root = Path(temporary)
        home = root / 'home'
        home.mkdir()
        # Empty PATH proves Rust compilation/commissioning does not execute a TS
        # transpiler, shell, model provider or helper discovered in the source tree.
        env = {'HOME': str(home), 'PATH': '', 'LANG': 'C.UTF-8', 'TZ': 'UTC'}
        records = []

        def call(*argv: str, expected: int = 0):
            result = subprocess.run([str(binary), *argv], cwd=root, env=env,
                                    stdin=subprocess.DEVNULL, capture_output=True, timeout=20)
            records.append({'command': list(argv[:2]), 'exit': result.returncode})
            if result.returncode != expected:
                raise RuntimeError(f'{argv[:2]}: {result.returncode}: {result.stderr.decode(errors="replace")}')
            data = result.stdout if expected == 0 else result.stderr
            return json.loads(data)

        capabilities = call('capabilities', '--json')
        expected_commands = {'workflow.check', 'workflow.compile', 'workflow.commission',
                             'workflow.inspect', 'workflow.locate', 'workflow.source',
                             'workflow.schema', 'workflow.sdk'}
        assert expected_commands <= set(capabilities['commands'])
        assert 'factory.workflow-inspection/v1' in capabilities['nativeContracts']
        call('workflow', 'sdk', 'sdk', '--json')
        call('workflow', 'sdk', 'sdk', '--json', expected=2)
        schema = call('workflow', 'schema', '--json')
        assert schema == json.loads((root / 'sdk/schema.json').read_bytes())
        for name in ('single.workflow.ts', 'plural.workflow.ts'):
            source = Path('sdk/examples') / name
            compiled = call('workflow', 'compile', str(source), '--json')
            checked = call('workflow', 'check', str(source), '--json')
            assert checked['valid'] and compiled['compiled']['units']
            assert checked['source']['semanticDigest'] == compiled['source']['source']['digest']
        path = root / 'sdk/examples/single.workflow.ts'
        original = path.read_text()
        receipt = call('workflow', 'commission', 'state.json', 'sdk/examples/commission.json',
                       'sdk/examples/single.workflow.ts', '--json')
        run = receipt['commission']['commission']['runRef']
        reading = call('workflow', 'inspect', 'state.json', run, '--json')
        assert reading['totalAttempts'] == 0
        assert reading['source']['authoring']['entry'] == 'single.workflow.ts'
        source_ref = reading['source']['ref']
        original_digest = reading['source']['semanticDigest']
        found = call('workflow', 'locate', 'state.json', source_ref, '--json')
        assert found['matches'] and found['matches'][0]['runRef'] == run
        path.write_text('// edited after commissioning; history stays fixed\n' + original)
        held = call('workflow', 'source', 'state.json', run, 'single.workflow.ts', '--json')
        assert held['module']['content'] == original
        assert held['sourceDigest'] == original_digest
        state_bytes = (root / 'state.json').read_bytes()
        call('workflow', 'commission', 'state.json', 'sdk/examples/commission.json',
             'sdk/examples/single.workflow.ts', '--json', expected=2)
        assert (root / 'state.json').read_bytes() == state_bytes
        assert hashlib.sha256(binary.read_bytes()).hexdigest() == fingerprint
        print(json.dumps({'standing': 'passed', 'scope': 'installed-generic-authoring-not-ACP',
                          'binary_sha256': fingerprint, 'operations': records,
                          'provider_launches': 0, 'semantic_digest': original_digest}, indent=2))


if __name__ == '__main__':
    main()
