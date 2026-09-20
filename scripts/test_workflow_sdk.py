#!/usr/bin/env python3
"""Typecheck the installed SDK and native source examples, then test npm packing.

Uses an explicitly available tsc (CI supplies the existing locked UI compiler).
No provider, owner configuration, credentials or model call is involved.
"""
from pathlib import Path
import argparse, json, os, shutil, subprocess, tempfile
ROOT=Path(__file__).resolve().parents[1]
SDK=ROOT/'factory/workflow-sdk'
def run(argv,cwd):
    subprocess.run(argv,cwd=cwd,check=True)
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--tsc',default='tsc');a=p.parse_args()
    compiler=shutil.which(a.tsc)
    if not compiler:raise SystemExit('Required TypeScript compiler is unavailable; SDK typecheck is BLOCKED, not skipped')
    compiler=str(Path(compiler).resolve())
    run(['python3',str(ROOT/'scripts/generate_workflow_types.py'),'--check'],ROOT)
    run(['node','--test','test.mjs'],SDK)
    with tempfile.TemporaryDirectory(prefix='factory-sdk-') as directory:
        world=Path(directory)
        package=world/'node_modules/@epilogos/factory-workflow'
        shutil.copytree(SDK,package)
        for example in (SDK/'examples').glob('*.ts'):shutil.copyfile(example,world/example.name)
        (world/'package.json').write_text('{"type":"module"}\n')
        (world/'tsconfig.json').write_text(json.dumps({'compilerOptions':{'strict':True,'noEmit':True,'module':'NodeNext','moduleResolution':'NodeNext','target':'ES2022','skipLibCheck':False},'include':['*.ts']}))
        run([compiler,'-p',str(world/'tsconfig.json')],world)
        # A malformed declared contract must actually fail the compiler.
        (world/'bad.ts').write_text("import {unit} from '@epilogos/factory-workflow'; unit({key: 'missing-obligations'});\n")
        if subprocess.run([compiler,'-p',str(world/'tsconfig.json')],cwd=world,stdout=subprocess.PIPE,stderr=subprocess.PIPE).returncode==0:
            raise SystemExit('Malformed workflow unexpectedly typechecked')
        (world/'bad.ts').unlink()
        packed=subprocess.run(['npm','pack','--json','--ignore-scripts','--pack-destination',str(world)],cwd=SDK,check=True,stdout=subprocess.PIPE,text=True)
        manifest=json.loads(packed.stdout)[0]
        files={f['path'] for f in manifest['files']}
        if not {'index.d.ts','index.mjs','package.json','examples/single.workflow.ts','examples/plural.workflow.ts'} <= files:raise SystemExit('Packed SDK omitted a required resource')
        print('Installed TypeScript and package resource checks passed')
if __name__=='__main__':main()
