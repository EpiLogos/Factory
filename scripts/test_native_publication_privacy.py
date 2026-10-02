#!/usr/bin/env python3
"""Run one real foreign-owner publication case on an ephemeral hosted runner.

The normal native suite runs without privilege. Only this exact, owned TempDir
fixture needs root to make source and stage have different real uid/gid values.
"""
import json
import os
from pathlib import Path
import subprocess


CASE = ('native_file_transaction::unix::tests::'
        'actual_foreign_uid_gid_privacy_is_preserved_by_privileged_native_publication')


def main():
    root = Path(__file__).resolve().parents[1]
    uid, gid = os.getuid(), os.getgid()
    if uid == 0 or gid == 0:
        raise RuntimeError('gate requires an actual unprivileged runner uid and gid')
    built = subprocess.run(
        ['cargo', 'test', '--package', 'epilogos-factory', '--lib', '--no-run', '--locked',
         '--message-format=json'], cwd=root, text=True, capture_output=True, check=False)
    if built.returncode:
        raise RuntimeError(f'native test build failed ({built.returncode}):\n{built.stderr}')
    executables = []
    for line in built.stdout.splitlines():
        message = json.loads(line)
        if (message.get('reason') == 'compiler-artifact'
                and message.get('target', {}).get('name') == 'epilogos_factory'
                and message.get('target', {}).get('kind') == ['lib']
                and message.get('profile', {}).get('test') is True
                and message.get('executable')):
            executables.append(Path(message['executable']).resolve(strict=True))
    if len(executables) != 1:
        raise RuntimeError(f'expected one actual Cargo library-test executable, got {executables}')
    executable = executables[0]
    configured_target = Path(os.environ.get('CARGO_TARGET_DIR', root / 'target'))
    if not configured_target.is_absolute():
        configured_target = root / configured_target
    executable.relative_to(configured_target.resolve(strict=True))
    if not executable.is_file():
        raise RuntimeError('native test executable is not a regular file')
    # No project service, login, model or broad test suite is invoked as root.
    # Identities are the actual runner's; the test refuses to run without them.
    observed = subprocess.run(
        ['sudo', '-n', 'env', f'FACTORY_NATIVE_TEST_SOURCE_UID={uid}',
         f'FACTORY_NATIVE_TEST_SOURCE_GID={gid}', str(executable), '--ignored',
         '--exact', CASE, '--nocapture'], cwd=root, text=True, capture_output=True,
        timeout=30, check=False)
    print(observed.stdout, end='')
    print(observed.stderr, end='')
    if observed.returncode or 'running 1 test' not in observed.stdout \
            or '1 passed; 0 failed' not in observed.stdout:
        raise RuntimeError(f'actual foreign-owner publication case failed ({observed.returncode})')


if __name__ == '__main__':
    main()
