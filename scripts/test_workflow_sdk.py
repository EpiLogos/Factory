#!/usr/bin/env python3
"""Exercise the actual packed Factory SDK, not an unpackaged source-tree copy.

The TypeScript compiler is an explicitly available companion. No installation of
providers, owner configuration or credentials, and no model calls are performed.
"""
from pathlib import Path
import argparse
import json
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SDK = ROOT / 'factory/workflow-sdk'
REQUIRED = {'index.d.ts', 'index.mjs', 'package.json', 'schema.json', 'README.md',
            'test.mjs', 'examples/single.workflow.ts', 'examples/plural.workflow.ts', 'examples/commission.json'}


def run(argv: list[str], cwd: Path) -> None:
    subprocess.run(argv, cwd=cwd, check=True, timeout=120)


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--tsc', default='tsc')
    args = p.parse_args()
    compiler = shutil.which(args.tsc)
    if not compiler:
        raise SystemExit('Required TypeScript compiler is unavailable; typecheck is BLOCKED, not skipped')
    compiler = str(Path(compiler).resolve())
    run(['python3', str(ROOT / 'scripts/generate_workflow_types.py'), '--check'], ROOT)
    with tempfile.TemporaryDirectory(prefix='factory-packed-sdk-') as directory:
        world = Path(directory)
        packed = subprocess.run(['npm', 'pack', '--json', '--ignore-scripts', '--pack-destination', str(world)],
                                cwd=SDK, check=True, stdout=subprocess.PIPE, text=True, timeout=120)
        manifest = json.loads(packed.stdout)[0]
        files = {f['path'] for f in manifest['files']}
        if not REQUIRED <= files:
            raise SystemExit(f'Packed SDK omitted required resources: {sorted(REQUIRED - files)}')
        if any(name.startswith('node_modules/') for name in files):
            raise SystemExit('Compiler installation leaked into the distributable package')
        package = world / 'node_modules/@epilogos/factory-workflow'
        package.mkdir(parents=True)
        # Inspect/extract the *actual* npm tarball with a narrow regular-file law.
        with tarfile.open(world / manifest['filename'], 'r:gz') as tar:
            extracted = set()
            for member in tar:
                relative = Path(member.name)
                if not member.isfile() or relative.parts[0] != 'package' or '..' in relative.parts:
                    raise SystemExit(f'Unexpected package member: {member.name}')
                relative = Path(*relative.parts[1:])
                target = package / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                data = tar.extractfile(member).read()
                with target.open('xb') as stream:
                    stream.write(data)
                extracted.add(str(relative))
            if not REQUIRED <= extracted or extracted != files:
                raise SystemExit('npm manifest does not match the package tarball')
        if (package / 'schema.json').read_bytes() != (ROOT / 'contracts/factory/agent-workflow-source.schema.json').read_bytes():
            raise SystemExit('Packed schema differs from the Factory owner')
        run(['npm', 'test', '--ignore-scripts=false'], package)
        # Typecheck consumer imports exclusively against the extracted package.
        for example in (package / 'examples').glob('*.ts'):
            shutil.copyfile(example, world / example.name)
        (world / 'package.json').write_text('{"type":"module"}\n')
        (world / 'tsconfig.json').write_text(json.dumps({'compilerOptions': {
            'strict': True, 'noEmit': True, 'module': 'NodeNext', 'moduleResolution': 'NodeNext',
            'target': 'ES2022', 'skipLibCheck': False}, 'include': ['*.ts']}))
        run([compiler, '-p', str(world / 'tsconfig.json')], world)
        (world / 'bad.ts').write_text("import {unit} from '@epilogos/factory-workflow'; unit({key: 'missing-obligations'});\n")
        bad = subprocess.run([compiler, '-p', str(world / 'tsconfig.json')], cwd=world,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
        if bad.returncode == 0 or b'bad.ts' not in bad.stdout:
            raise SystemExit('Malformed workflow did not fail the actual TypeScript compiler as expected')
        # A registered domain adapter composes the same packed SDK: the QL Vāk
        # workflows typecheck against the vendored declarations Factory pins,
        # and a C-prime outside QL's vocabulary fails the actual compiler.
        registry = json.loads((ROOT / 'contracts/factory/workflow-domain-adapters.json').read_text())
        for adapter in registry['adapters']:
            installed = world / 'node_modules' / adapter['specifier']
            installed.mkdir(parents=True)
            shutil.copyfile(ROOT / adapter['declarations'], installed / 'index.d.ts')
            (installed / 'package.json').write_text(json.dumps({
                'name': adapter['specifier'], 'types': './index.d.ts',
                'exports': {'.': {'types': './index.d.ts'}}}))
        domain = world / 'domain'
        domain.mkdir()
        (domain / 'package.json').write_text('{"type":"module"}\n')
        (domain / 'tsconfig.json').write_text(json.dumps({'compilerOptions': {
            'strict': True, 'noEmit': True, 'module': 'NodeNext', 'moduleResolution': 'NodeNext',
            'target': 'ES2022', 'skipLibCheck': False}, 'include': ['*.ts']}))
        fixtures = sorted((ROOT / 'factory/tests/fixtures/ql-vak-workflows').glob('*.workflow.ts'))
        if len(fixtures) != 3:
            raise SystemExit('Expected the three QL-authored workflow fixtures')
        for fixture in fixtures:
            shutil.copyfile(fixture, domain / fixture.name)
        run([compiler, '-p', str(domain / 'tsconfig.json')], domain)
        wrong = fixtures[0].read_text().replace('CF: "CF2"', 'CF: "CF9"', 1)
        if wrong == fixtures[0].read_text():
            raise SystemExit('Negative C-prime fixture was not produced')
        (domain / 'wrong-frame.ts').write_text(wrong)
        bad = subprocess.run([compiler, '-p', str(domain / 'tsconfig.json')], cwd=domain,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
        if bad.returncode == 0 or b'wrong-frame.ts' not in bad.stdout:
            raise SystemExit('A C-prime outside the QL vocabulary did not fail the TypeScript compiler')
        # An installed ECMAScript consumer imports through the public exports.
        (world / 'consumer.mjs').write_text("import {unit} from '@epilogos/factory-workflow'; const x={key:'native-source'}; if(unit(x)!==x || !Object.isFrozen(x)) process.exit(1);\n")
        run(['node', 'consumer.mjs'], world)
        print(f'Packed SDK: {len(files)} resources checked; installed imports, positive and negative typechecks passed, including {len(fixtures)} domain-adapter workflows')


if __name__ == '__main__':
    main()
