"""Regression checks for the consolidated native CI coverage (not product proof)."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / '.github/workflows'


class NativeWorkflowTests(unittest.TestCase):
    def setUp(self):
        import yaml
        self.workflow = yaml.load((WORKFLOWS / 'factory-rust.yml').read_text(), Loader=yaml.BaseLoader)
        self.steps = self.workflow['jobs']['factory-rust']['steps']
        self.commands = '\n'.join(step.get('run', '') for step in self.steps)

    def test_one_required_native_gate_no_duplicate_branch_push(self):
        self.assertIn('pull_request', self.workflow['on'])
        self.assertFalse(self.workflow['on']['pull_request'])
        self.assertEqual(['main'], self.workflow['on']['push']['branches'])
        self.assertIn("github.event_name == 'pull_request'", self.workflow['concurrency']['cancel-in-progress'])

    def test_replaced_lanes_have_transferred_every_unique_obligation(self):
        for script in ('validate_factory_skills.py', 'validate_agent_capability_intake.py',
                       'validate_routine_continuation.py', 'validate_self_hosting_commission.py'):
            self.assertIn('python3 scripts/' + script, self.commands)
        self.assertIn('cargo fmt --all -- --check', self.commands)
        self.assertIn('cargo clippy --workspace --all-targets --locked -- -D warnings', self.commands)
        self.assertIn('cargo test --workspace --all-targets --locked', self.commands)
        self.assertIn('cargo test --workspace --doc --locked', self.commands)
        self.assertFalse((WORKFLOWS / 'factory-native-skills.yml').exists())
        self.assertFalse((WORKFLOWS / 'prelocal-build.yml').exists())

    def test_cache_names_actual_root_workspace(self):
        caches = [step for step in self.steps if step.get('uses', '').startswith('Swatinem/rust-cache@')]
        self.assertEqual(1, len(caches))
        self.assertEqual('. -> target', caches[0]['with']['workspaces'])
        self.assertEqual('true', caches[0]['with']['cache-on-failure'])

    def test_source_packaging_remains_exact_main_and_test_gated(self):
        job = self.workflow['jobs']['artifact']
        self.assertEqual(['factory-rust', 'factory-native-macos'], job['needs'])
        self.assertIn("github.ref == 'refs/heads/main'", job['if'])
        self.assertTrue(any(step.get('uses') == 'actions/attest@v4' for step in job['steps']))
        body = '\n'.join(step.get('run', '') for step in job['steps'])
        self.assertIn('source-commit.txt', body)
        self.assertIn('Cargo.toml Cargo.lock README.md', body)
        import subprocess
        tracked = subprocess.check_output(['git', '-C', str(ROOT), 'ls-files', '-z']).split(b'\0')
        runtime_caches = [name for name in tracked
                          if b'__pycache__' in name.split(b'/') or name.endswith((b'.pyc', b'.pyo'))]
        self.assertEqual([], runtime_caches, 'Generated Python runtime caches must not be tracked Source')

    def test_main_trigger_covers_union_of_retired_lanes(self):
        paths = set(self.workflow['on']['push']['paths'])
        self.assertTrue({'factory/**', 'contracts/factory/**', 'skills/**', 'scripts/**',
                         '.oi/product.json', 'Cargo.toml', 'Cargo.lock'} <= paths)
        self.assertLessEqual(int(self.workflow['jobs']['factory-rust']['timeout-minutes']), 20)

    def test_actual_macos_native_gate_and_foreign_owner_case_are_required_for_publication(self):
        mac = self.workflow['jobs']['factory-native-macos']
        self.assertEqual('macos-14', mac['runs-on'])
        commands = '\n'.join(step.get('run', '') for step in mac['steps'])
        self.assertIn('cargo test --workspace --all-targets --locked', commands)
        self.assertIn('cargo clippy --workspace --all-targets --locked -- -D warnings', commands)
        self.assertIn('python3 scripts/test_native_publication_privacy.py', commands)
        self.assertIn('python3 scripts/test_native_publication_privacy.py', self.commands)


class NativeEvidenceWorkflowTests(unittest.TestCase):
    def setUp(self):
        import yaml
        self.workflow = yaml.load((WORKFLOWS / 'factory-native-evidence.yml').read_text(), Loader=yaml.BaseLoader)
        self.job = self.workflow['jobs']['native-attempt-evidence']
        self.steps = self.job['steps']

    def test_real_both_platforms_keep_distinct_failure_evidence(self):
        self.assertEqual(['ubuntu-latest', 'macos-14'], self.job['strategy']['matrix']['os'])
        self.assertEqual('false', self.job['strategy']['fail-fast'])
        upload = next(step for step in self.steps if step.get('uses') == 'actions/upload-artifact@v4')
        self.assertEqual("${{ always() && steps.evidence_paths.outcome == 'success' }}", upload['if'])
        self.assertIn('runner.os', upload['with']['name'])

    def test_binary_admission_requires_every_actual_native_leg(self):
        import re
        gates = {step['id'] for step in self.steps if 'id' in step and step['id'] not in ('build', 'evidence_paths')}
        publication = next(step for step in self.steps if step.get('name') == 'Preserve integration-tested native binary')
        guarded = set(re.findall(r"steps\.(\w+)\.outcome == 'success'", publication['if']))
        self.assertEqual(gates, guarded)
        self.assertIn('success()', publication['if'])
        reporting = next(step for step in self.steps if step.get('name', '').startswith('Retain actual step outcomes'))
        self.assertEqual('${{ always() }}', reporting['if'])
        self.assertIn("'full_native_composite_qualified': False", reporting['run'])
        self.assertIn("'Original_Run_credit': False", reporting['run'])

    def test_paired_real_child_and_exact_parent_are_required(self):
        import json
        build = next(step for step in self.steps if step.get('id') == 'paired_build')
        self.assertIn('--features native-receiving-incomplete-child', build['run'])
        self.assertIn('--test native_receiving_incomplete_child', build['run'])
        self.assertIn('--no-run --message-format=json', build['run'])
        self.assertIn("receipt.get('profile', {}).get('test')", build['run'])
        self.assertIn('fci06-build-receipts.json', build['run'])
        self.assertIn('info.st_nlink < 1', build['run'])
        self.assertIn('private.st_nlink != 1', build['run'])
        self.assertIn('os.O_NOFOLLOW | os.O_NONBLOCK', build['run'])
        self.assertIn('manifest_path', build['run'])
        self.assertIn('src_path', build['run'])
        self.assertIn('original compiler artifact changed', build['run'])
        paired = next(step for step in self.steps if step.get('id') == 'paired')
        self.assertEqual('factory-fci06-controlled-human-fixture-not-personal', paired['env']['CENTRAL_NATIVE_TOKEN'])
        self.assertIn('names paired', paired['run'])
        self.assertIn('--exact --ignored --test-threads=1', paired['run'])
        self.assertNotIn('--nocapture', paired['run'])
        self.assertIn('FACTORY_FCI06_ARTIFACT_DIR', paired['run'])
        sets = json.loads((ROOT / 'scripts/native_evidence_case_sets.json').read_text())
        self.assertEqual(['attempt_receiving::paired_inclusion_native_tests::actual_central_post_document_and_secondary_receiving_failures_reach_same_factory_transport'], sets['groups']['paired'])
        prepare = next(step for step in self.steps if step.get('name') == 'Prepare published Central Source for the paired FCI06 gate')
        self.assertIn('01af4e53d3d8a53f86b6d6e37f0fa365c501bdad', prepare['run'])
        self.assertIn('f23381d08812a593bc136cf8ae9b520611ebec40', prepare['run'])
        self.assertIn('fci06-central-source.tar.gz', prepare['run'])

    def test_preparation_selects_its_actual_nonignored_bodies(self):
        preparation = next(step for step in self.steps if step.get('id') == 'integration')
        self.assertIn('--include-ignored --exact --test-threads=1', preparation['run'])
        self.assertNotIn('--ignored', preparation['run'])
        self.assertIn('names preparation', preparation['run'])

    def test_receiving_child_cannot_be_dispatched_as_an_unscoped_ignored_body(self):
        receiving = next(step for step in self.steps if step.get('id') == 'receiving')
        self.assertIn('names receiving', receiving['run'])
        self.assertIn('--exact --ignored', receiving['run'])
        self.assertNotIn('--include-ignored', receiving['run'])
        import json
        sets = json.loads((ROOT / 'scripts/native_evidence_case_sets.json').read_text())
        self.assertEqual(12, len(sets['groups']['capture']))
        self.assertEqual(32, len(sets['groups']['receiving']))
        self.assertEqual(3, len(sets['groups']['consumer-os']))
        self.assertNotIn(sets['integration_child_definition'], sets['groups']['receiving'])
        self.assertEqual(9, len(sets['groups']['preparation']))
        self.assertIn('central_cases::followup::native_central_return_keeps_the_allocated_now_without_including_the_document', sets['groups']['preparation'])
        self.assertEqual(64, sum(len(cases) for cases in sets['groups'].values()))
        self.assertFalse(sets['full_native_composite_qualified'])

    def test_owner_dispatch_requires_six_actual_bodies_and_observed_inputs(self):
        import json
        dispatch = next(step for step in self.steps if step.get('id') == 'owner_dispatch')
        sets = json.loads((ROOT / 'scripts/native_evidence_case_sets.json').read_text())
        expected = [
            'attempt_owner_dispatch::native_failure_tests::actual_timeout_incomplete_pipes_retain_original_private_capture',
            'attempt_owner_dispatch::native_failure_tests::actual_missing_executable_is_typed_failure_with_single_intent_and_no_resend',
            'attempt_owner_dispatch::native_failure_tests::actual_capture_and_post_publish_uncertainty_remain_distinct_without_compensation',
            'attempt_owner_dispatch::native_failure_tests::actual_source_receipt_projection_and_historical_replay_keep_public_schema',
            'attempt_owner_dispatch::publication_tests::actual_fallback_publication_retains_primary_refusal_and_secondary_native_cause',
            'attempt_owner_dispatch::publication_tests::actual_owner_retention_keeps_apply_cause_when_followup_native_read_fails',
        ]
        self.assertEqual(expected, sets['groups']['owner-dispatch'])
        self.assertEqual(6, len(set(expected)))
        self.assertIn('list owner-dispatch', dispatch['run'])
        self.assertIn('names owner-dispatch', dispatch['run'])
        self.assertIn('run owner-dispatch --case "$case"', dispatch['run'])
        self.assertIn('"$FACTORY_NATIVE_LIBRARY_IMAGE" attempt_owner_dispatch:: --list', dispatch['run'])
        self.assertIn('--include-ignored --exact --test-threads=1', dispatch['run'])
        self.assertNotIn('--nocapture', dispatch['run'])
        self.assertNotIn('attempt_owner_delivery', dispatch['run'])
        admission = next(step for step in self.steps if step.get('id') == 'input_admission')
        receiving = next(step for step in self.steps if step.get('id') == 'receiving')
        self.assertLess(self.steps.index(admission), self.steps.index(receiving))
        for required in ('ProjectCentral', 'os.O_NOFOLLOW', 'os.fstat', 'os.geteuid',
                         'FACTORY_NATIVE_EVIDENCE_DIR', 'TMPDIR', 'GITHUB_ENV',
                         'FACTORY_NATIVE_PYTHON_BIN', 'FACTORY_NATIVE_PYTHON_SHA256',
                         'owner-dispatch-admission.json'):
            self.assertIn(required, admission['run'])
        self.assertIn('owner-dispatch-inputs-after.json', dispatch['run'])
        self.assertNotIn('os.mkdir', dispatch['run'], 'the later gate reuses the one admitted root')
        reporting = next(step for step in self.steps if step.get('name', '').startswith('Retain actual step outcomes'))
        self.assertIn("'owner_dispatch'", reporting['run'])
        self.assertIn("'required_paired_FCI06': outcomes['paired']", reporting['run'])
        self.assertFalse(sets['full_native_composite_qualified'])
        self.assertFalse(sets['required_pending_qualification']['compositeQualificationComplete'])
        retention = next(step for step in self.steps if step.get('name', '').startswith('Retain the owned dispatch fixture'))
        self.assertEqual('${{ always() }}', retention['if'])
        self.assertIn("'process_quiescence_claim': False", retention['run'])
        self.assertIn('No signal or recursive deletion', retention['run'])
        census = (ROOT / 'scripts/native_evidence_census.py').read_text()
        self.assertIn('validate_list(text, expected, "attempt_owner_dispatch::")', census)

    def test_current_cancellation_guard_uses_actual_producer_and_compiler_images(self):
        import json
        sets = json.loads((ROOT / 'scripts/native_evidence_case_sets.json').read_text())
        receiving = next(step for step in self.steps if step.get('id') == 'receiving')
        selection = next(step for step in self.steps if step.get('id') == 'snapshot_selection')
        guard = next(step for step in self.steps if step.get('id') == 'cancellation_guard')
        self.assertLess(self.steps.index(receiving), self.steps.index(selection))
        self.assertLess(self.steps.index(selection), self.steps.index(guard))
        build = next(step for step in self.steps if step.get('id') == 'build')
        admitted = next(step for step in self.steps if step.get('id') == 'paired_build')
        self.assertIn('--no-run --message-format=json', build['run'])
        self.assertIn('compiler --role receiving', admitted['run'])
        self.assertLess(self.steps.index(admitted), self.steps.index(receiving))
        self.assertIn('"$FACTORY_NATIVE_RECEIVING_IMAGE" --list', receiving['run'])
        self.assertIn('native_evidence_inputs.py snapshot', selection['run'])
        self.assertIn('names cancellation-guard', guard['run'])
        self.assertIn('--exact --ignored --test-threads=1', guard['run'])
        self.assertNotIn('--nocapture', guard['run'])
        self.assertEqual(['orchestration::native_cancellation_guard_tests::actual_native_snapshot_new_request_cannot_borrow_prior_acceptance'], sets['groups']['cancellation-guard'])
        upload = next(step for step in self.steps if step.get('uses') == 'actions/upload-artifact@v4')
        self.assertNotIn('ProjectCentral/now/tmp/', upload['with']['path'])
        retained = next(step for step in self.steps if step.get('name', '').startswith('Retain bounded ordinary'))
        self.assertEqual('${{ always() }}', retained['if'])
        self.assertIn('native_evidence_inputs.py fixtures', retained['run'])


    def test_every_case_projection_is_declared_before_dispatch_and_reused_by_snapshot(self):
        for group, step_id in (('capture', 'capture'), ('preparation', 'integration'),
                               ('receiving', 'receiving'), ('cancellation-guard', 'cancellation_guard'),
                               ('consumer-os', 'consumer_os'), ('owner-dispatch', 'owner_dispatch'),
                               ('paired', 'paired')):
            command = next(step for step in self.steps if step.get('id') == step_id)['run']
            declaration = 'log-path ' + group + ' --case "$case" --input native-evidence'
            self.assertIn(declaration, command)
            self.assertLess(command.index(declaration), command.index('"$case" --'))
            self.assertIn('tee "$log"', command)
            self.assertIn('run ' + group + ' --case "$case" --input "$log"', command)
            self.assertNotIn('-$case.log', command)
        inputs = (ROOT / 'scripts/native_evidence_inputs.py').read_text()
        self.assertIn('EVIDENCE / portable_log_path("receiving", producer)', inputs)
        self.assertIn('validate_run(producer_bytes.decode("utf-8"), producer)', inputs)
        gate = next(step for step in self.steps if step.get('id') == 'evidence_paths')
        self.assertEqual('${{ always() }}', gate['if'])
        self.assertIn('upload-paths --input native-evidence', gate['run'])
        upload = next(step for step in self.steps if step.get('uses') == 'actions/upload-artifact@v4')
        self.assertLess(self.steps.index(gate), self.steps.index(upload))


    def test_all_builds_finish_before_pin_and_body_dispatch_uses_admitted_original_images(self):
        # This is workflow Source conformance, not an executed native receipt.
        build = next(step for step in self.steps if step.get('id') == 'build')
        pins = next(step for step in self.steps if step.get('id') == 'paired_build')
        consolidated = 'cargo test --locked -p epilogos-factory --lib --test attempt_owner_delivery --test native_lifecycle_receiving --test native_flow_association --test sensing_public --no-run --message-format=json'
        self.assertIn(consolidated, build['run'])
        first_pin = None
        commands = []
        for step_index, step in enumerate(self.steps):
            for line_index, line in enumerate(step.get('run', '').splitlines()):
                text = line.strip()
                if text.startswith(('cargo build ', 'cargo test ')):
                    commands.append((step_index, line_index))
                if ('with os.fdopen(os.open(path, flags)' in text
                        or text.startswith('python3 scripts/native_evidence_inputs.py compiler ')):
                    point = (step_index, line_index)
                    first_pin = point if first_pin is None else min(first_pin, point)
        self.assertIsNotNone(first_pin)
        self.assertEqual(8, len(commands))
        for point in commands:
            self.assertLess(point, first_pin, 'a later compiler must not replace an admitted image')
        roles = (
            ('preparation', 'factory/tests/attempt_owner_delivery.rs', 'attempt_owner_delivery'),
            ('flow', 'factory/tests/native_flow_association.rs', 'native_flow_association'),
            ('sensing', 'factory/tests/sensing_public.rs', 'sensing_public'),
        )
        for role, entry, target in roles:
            selected = ('compiler --role ' + role
                        + ' --log native-evidence/factory-native-test-compiler.jsonl'
                        + ' --root "$PWD" --manifest factory/Cargo.toml --entry ' + entry
                        + ' --target ' + target + ' --kind test --test-profile')
            self.assertIn(selected, pins['run'])
        self.assertIn('reobserve_compiler_image(receipt)', pins['run'])
        self.assertIn("executable = receipt['actual_executable']", pins['run'])
        self.assertIn("native-evidence/compiled-body-images-before.json", pins['run'])
        for step_id, image in (('capture', 'LIBRARY'), ('integration', 'PREPARATION'),
                               ('receiving', 'RECEIVING'), ('cancellation_guard', 'LIBRARY'),
                               ('consumer_os', 'LIBRARY'), ('owner_dispatch', 'LIBRARY'),
                               ('paired', 'LIBRARY'), ('flow', 'FLOW'), ('sensing', 'SENSING')):
            step = next(step for step in self.steps if step.get('id') == step_id)
            self.assertIn('"$FACTORY_NATIVE_' + image + '_IMAGE"', step['run'])
            self.assertNotIn('cargo test ', step['run'])
        flow = next(step for step in self.steps if step.get('id') == 'flow')
        sensing = next(step for step in self.steps if step.get('id') == 'sensing')
        self.assertIn('native-flow-cases.txt)" = 7', flow['run'])
        self.assertIn('sensing-public-cases.txt)" = 6', sensing['run'])
        reobserve = next(step for step in self.steps if step.get('id') == 'input_reobservation')
        for role, _, _ in roles:
            self.assertIn("'" + role + "'", reobserve['run'])



class NativeCensusRefusalTests(unittest.TestCase):
    """Real pure-validator inputs; no fabricated native-owner positive receipt."""
    def setUp(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location('native_evidence_census', ROOT / 'scripts/native_evidence_census.py')
        self.census = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.census)

    def test_missing_duplicate_extra_definitions_are_refused(self):
        for text in ('', 'required: test\nrequired: test\n', 'required: test\nforeign: test\n'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                self.census.validate_list(text, ['required'])

    def test_zero_ignored_wrong_body_and_failed_summaries_are_refused(self):
        fragments = [
            'running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n',
            'running 1 test\ntest required ... ignored\ntest result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s\n',
            'running 1 test\ntest foreign ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n',
            'running 1 test\ntest required ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n',
        ]
        for text in fragments:
            with self.subTest(text=text), self.assertRaises(ValueError):
                self.census.validate_run(text, 'required')

    def test_success_looking_prefix_never_overrides_failed_or_incomplete_capture(self):
        prefix = 'running 1 test\ntest required ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n'
        for text in (prefix + 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n', prefix[:-1], prefix.split(' finished in')[0]+'\n', prefix+'test required ... FAILED\n'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                self.census.validate_run(text, 'required')


class NativeEvidencePathTests(unittest.TestCase):
    """Actual filesystem evidence projection checks, not native-owner pass receipts."""
    def setUp(self):
        NativeCensusRefusalTests.setUp(self)
        import json
        import tempfile
        self.sets = json.loads((ROOT / 'scripts/native_evidence_case_sets.json').read_text())
        scratch = ROOT / 'ProjectCentral/now/tmp'
        scratch.mkdir(parents=True, exist_ok=True)
        self.owned = tempfile.TemporaryDirectory(prefix='native-evidence-path-', dir=scratch)
        self.addCleanup(self.owned.cleanup)
        self.directory = Path(self.owned.name)

    def test_declared_exact_case_has_portable_pre_dispatch_identity_and_unchanged_raw_bytes(self):
        import hashlib
        import json
        case = self.sets['groups']['capture'][-1]
        self.assertIn('::', case)
        log = self.census.declare_log_path(self.directory, 'capture', case, self.sets)
        self.assertFalse(log.exists())
        identity = json.loads(log.with_suffix('.identity.json').read_text())
        self.assertEqual(case, identity['case'])
        self.assertEqual(str(log), identity['actual_log'])
        self.assertTrue(identity['before_dispatch'])
        self.assertFalse(identity['native_body_admitted'])
        self.assertEqual('capture-' + hashlib.sha256(case.encode()).hexdigest() + '.log', log.name)
        # Raw evidence is opaque here; no synthetic libtest text is admitted.
        raw = bytes(range(256)) + b'\x00stdout\nstderr\nlast-exit-result\n'
        log.write_bytes(raw)
        self.assertEqual(raw, log.read_bytes())
        result = self.census.validate_upload_tree(self.directory)
        self.assertTrue(result['portable'])
        self.assertEqual(2, result['ordinary_files'])
        self.assertFalse(result['native_body_admission'])

    def test_duplicate_identity_undeclared_case_and_existing_output_are_refused_before_dispatch(self):
        import copy
        case = self.sets['groups']['capture'][0]
        duplicate = copy.deepcopy(self.sets)
        duplicate['groups']['capture'].append(case)
        with self.assertRaises(ValueError):
            self.census.case_path_mapping(duplicate)
        with self.assertRaises(ValueError):
            self.census.declare_log_path(self.directory, 'capture', 'foreign::case', self.sets)
        log = self.census.declare_log_path(self.directory, 'capture', case, self.sets)
        with self.assertRaises(FileExistsError):
            self.census.declare_log_path(self.directory, 'capture', case, self.sets)
        log.write_bytes(b'original output')
        with self.assertRaises(ValueError):
            self.census.declare_log_path(self.directory, 'capture', case, self.sets)
        self.assertEqual(b'original output', log.read_bytes())
        mapping = self.census.case_path_mapping(self.sets)
        self.assertEqual(64, len(mapping))
        self.assertEqual(64, len(set(mapping.values())))

    def test_nested_actual_unsupported_names_refuse_upload_without_renaming_or_losing_bytes(self):
        nested = self.directory / 'fixture-snapshot'
        nested.mkdir()
        ordinary = nested / 'entry-1'
        ordinary.write_bytes(b'actual retained bytes')
        bad = nested / 'capture-native_process::case.log'
        bad.write_bytes(b'failure evidence remains')
        with self.assertRaises(ValueError):
            self.census.validate_upload_tree(self.directory)
        self.assertEqual(b'failure evidence remains', bad.read_bytes())
        self.assertEqual(b'actual retained bytes', ordinary.read_bytes())
        for component in ('CON', 'entry.', 'entry ', 'a\\b', 'a\nb', 'a?b'):
            with self.subTest(component=component), self.assertRaises(ValueError):
                self.census.validate_portable_component(component)

    @unittest.skipUnless(__import__('os').name == 'posix', 'actual Unix FIFO/alias case')
    def test_actual_alias_and_fifo_are_refused_before_upload_without_body_read(self):
        import os
        ordinary = self.directory / 'ordinary'
        ordinary.write_bytes(b'original')
        alias = self.directory / 'alias'
        alias.symlink_to(ordinary.name)
        with self.assertRaises(ValueError):
            self.census.validate_upload_tree(self.directory)
        alias.unlink()
        fifo = self.directory / 'fifo'
        os.mkfifo(fifo)
        with self.assertRaises(ValueError):
            self.census.validate_upload_tree(self.directory)
        self.assertEqual(b'original', ordinary.read_bytes())


@unittest.skipUnless(__import__('os').name == 'posix', 'held Unix compiler-image mechanism')
class NativeCompilerImageTests(unittest.TestCase):
    """Real executable bytes and filesystem controls; not fabricated CompilerArtifacts."""
    def setUp(self):
        import shutil
        import sys
        import tempfile
        from scripts import native_evidence_inputs
        self.inputs = native_evidence_inputs
        scratch = ROOT / 'ProjectCentral/now/tmp'
        scratch.mkdir(parents=True, exist_ok=True)
        self.owned = tempfile.TemporaryDirectory(prefix='native-compiler-image-', dir=scratch)
        self.addCleanup(self.owned.cleanup)
        self.directory = Path(self.owned.name)
        self.original = self.directory / 'actual-native-image'
        shutil.copyfile(Path(sys.executable).resolve(strict=True), self.original)
        self.original.chmod(0o700)
        self.copy = self.directory / 'retained-copy'

    def descriptor_count(self):
        import os
        import sys
        directory = Path('/proc/self/fd' if sys.platform.startswith('linux') else '/dev/fd')
        self.assertTrue(directory.is_dir(), 'actual descriptor census prerequisite unavailable')
        # os.listdir closes its own directory descriptor on each observation.
        return len(os.listdir(directory))

    def capture(self):
        observation = {'checkpoints': []}
        result = self.inputs.capture_compiler_image(self.original, self.copy, observation)
        return result, observation

    def test_actual_executable_copy_and_current_reobservation_preserve_all_bytes(self):
        import hashlib
        import stat
        descriptors = self.descriptor_count()
        result, observation = self.capture()
        def digest(path):
            value = hashlib.sha256()
            with path.open('rb') as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b''):
                    value.update(block)
            return value.hexdigest()
        self.assertEqual(digest(self.original), result['sha256'])
        self.assertEqual(digest(self.copy), result['sha256'])
        self.assertEqual(self.original.stat().st_size, result['bytes'])
        self.assertEqual(0o700, stat.S_IMODE(self.copy.stat().st_mode))
        self.assertEqual(1, self.copy.stat().st_nlink)
        self.assertTrue(observation['same_original_and_retained_digest'])
        current = self.inputs.observe_compiler_image(self.original, {}, result['identity'], result['sha256'])
        retained = self.inputs.observe_compiler_image(self.copy, {}, result['retained_identity'], result['sha256'], private=True)
        self.assertEqual(current['sha256'], retained['sha256'])
        self.assertEqual(result['bytes'], current['bytes'])
        self.assertEqual(descriptors, self.descriptor_count())

    def test_stable_original_hardlink_is_admitted_but_copy_collision_never_overwrites(self):
        import os
        os.link(self.original, self.directory / 'actual-native-hardlink')
        self.assertEqual(2, self.original.stat().st_nlink)
        result, _ = self.capture()
        self.assertEqual(2, result['identity'][3])
        self.assertEqual(1, self.copy.stat().st_nlink)
        before = self.inputs.observe_compiler_image(self.copy, {}, private=True)
        descriptors = self.descriptor_count()
        with self.assertRaises(FileExistsError):
            self.capture()
        self.assertEqual(before, self.inputs.observe_compiler_image(self.copy, {}, private=True))
        self.assertEqual(descriptors, self.descriptor_count())

    def test_actual_oversize_fifo_directory_and_symlink_refuse_before_body_or_copy(self):
        import os
        observation = {}
        descriptors = self.descriptor_count()
        oversized = self.directory / 'oversized'
        with oversized.open('wb') as stream:
            stream.truncate(self.inputs.MAX_COMPILER_IMAGE + 1)
        oversized.chmod(0o700)
        with self.assertRaises(ValueError):
            self.inputs.capture_compiler_image(oversized, self.copy, observation)
        predicates = observation['checkpoints'][0]['predicates']
        self.assertTrue(predicates['regular'])
        self.assertFalse(predicates['finite_image_capacity'])
        self.assertNotIn('observed_bytes', observation)
        self.assertFalse(self.copy.exists())
        fifo = self.directory / 'fifo'
        os.mkfifo(fifo, 0o700)
        for path in (fifo, self.directory):
            for operation in ('capture', 'observe'):
                for _ in range(3):
                    actual = {}
                    with self.subTest(path=path.name, operation=operation), self.assertRaises(ValueError):
                        if operation == 'capture':
                            self.inputs.capture_compiler_image(path, self.copy, actual)
                        else:
                            self.inputs.observe_compiler_image(path, actual)
                    self.assertFalse(actual['checkpoints'][0]['predicates']['regular'])
                    self.assertNotIn('observed_bytes', actual)
                    self.assertFalse(self.copy.exists())
                    self.assertEqual(descriptors, self.descriptor_count())
        alias = self.directory / 'alias'
        alias.symlink_to(self.original.name)
        actual = {}
        with self.assertRaises(OSError) as error:
            self.inputs.capture_compiler_image(alias, self.copy, actual)
        import errno
        self.assertEqual(errno.ELOOP, error.exception.errno)
        self.assertEqual('open_original', actual['stage'])
        self.assertIn('named_before_open', actual)
        self.assertFalse(self.copy.exists())
        self.assertEqual(descriptors, self.descriptor_count())

    def test_actual_named_replacement_and_growth_are_refused_on_same_held_descriptor(self):
        import os
        import shutil
        descriptors = self.descriptor_count()
        for change in ('replace', 'grow'):
            with self.subTest(change=change):
                path = self.directory / change
                shutil.copyfile(self.original, path)
                path.chmod(0o700)
                observation = {}
                with self.inputs.compiler_image_stream(path, observation, 'actual_open') as held:
                    before = self.inputs.image_checkpoint(held, path, observation, 'actual_before')
                    if change == 'replace':
                        replacement = self.directory / 'replacement'
                        shutil.copyfile(self.original, replacement)
                        replacement.chmod(0o700)
                        os.replace(replacement, path)
                    else:
                        with path.open('ab') as writer:
                            writer.write(b'actual appended bytes')
                            writer.flush()
                            os.fsync(writer.fileno())
                    with self.assertRaises(ValueError):
                        self.inputs.image_digest(held, path, before, observation, 'actual_after_change')
                if change == 'replace':
                    self.assertFalse(observation['checkpoints'][-1]['predicates']['held_named_identity'])
                else:
                    self.assertFalse(observation['read_predicates']['exact_observed_size'])
                self.assertEqual(descriptors, self.descriptor_count())

    def test_actual_retained_mutation_and_wrong_native_form_keep_failure_facts(self):
        import os
        descriptors = self.descriptor_count()
        result, _ = self.capture()
        with self.copy.open('r+b') as writer:
            writer.seek(4)
            previous = writer.read(1)
            writer.seek(4)
            writer.write(bytes([previous[0] ^ 1]))
            writer.flush()
            os.fsync(writer.fileno())
        actual = {}
        with self.assertRaises(ValueError):
            self.inputs.observe_compiler_image(self.copy, actual, result['retained_identity'], result['sha256'], private=True)
        self.assertFalse(actual['expected_identity_matches'])
        non_native = self.directory / 'non-native'
        non_native.write_bytes(b'actual text, not an executable image')
        non_native.chmod(0o700)
        actual = {}
        destination = self.directory / 'non-native-copy'
        with self.assertRaises(ValueError):
            self.inputs.capture_compiler_image(non_native, destination, actual)
        self.assertFalse(actual['native_magic'])
        self.assertEqual(0, destination.stat().st_size)
        self.assertEqual(b'actual text, not an executable image', non_native.read_bytes())
        self.assertEqual(descriptors, self.descriptor_count())


def current_compiler_artifact_conformance(receipt_path):
    """Called only AFTER the real native build, never an availability/skip proof."""
    import json
    from types import SimpleNamespace
    from scripts import native_evidence_inputs as inputs
    receipt_bytes, _ = inputs.held_bytes(receipt_path, inputs.MAX_MANIFEST)
    receipt = json.loads(receipt_bytes)
    if (receipt['role'] != 'factory' or receipt['compiler_artifact']['target']['name'] != 'factory'
            or receipt['compiler_artifact']['target']['kind'] != ['bin']
            or receipt['compiler_artifact']['profile']['test'] is not False):
        raise ValueError('wrong actual compiled Factory image for conformance')
    current = inputs.reobserve_compiler_image(receipt)
    if current['sha256'] != receipt['sha256']:
        raise ValueError('current actual CompilerArtifact image changed')
    root = Path(receipt['source_root'])
    args = SimpleNamespace(role='factory-wrong-profile', log=Path(receipt['compiler_log']), root=root,
                           manifest='factory/Cargo.toml', entry='factory/src/bin/factory.rs',
                           target='factory', kind='bin', test_profile=True)
    try:
        inputs.compiler(args)
    except ValueError:
        refused_bytes, _ = inputs.held_bytes(inputs.EVIDENCE / 'compiler-image-factory-wrong-profile.refusal.json', inputs.MAX_MANIFEST)
        refused = json.loads(refused_bytes)
        predicates = refused['compiler_predicates']
        if (refused['stage'] != 'compiler_association' or refused['admitted'] is not False
                or refused['actual_test_profile'] is not False or refused['requested_test_profile'] is not True
                or predicates['one_selected_artifact'] is not True or predicates['successful_finish'] is not True
                or predicates['selected_profile'] is not False
                or (inputs.EVIDENCE / 'compiler-image-factory-wrong-profile').exists()):
            raise ValueError('native wrong-profile refusal lacks actual owning facts')
    else:
        raise ValueError('actual wrong-profile CompilerArtifact was incorrectly admitted')
    inputs.write_json(inputs.EVIDENCE / 'compiler-image-conformance.json', {
        'schema': 'factory.native-compiler-conformance/v1', 'actual_receipt': str(receipt_path),
        'receipt_sha256': __import__('hashlib').sha256(receipt_bytes).hexdigest(),
        'current_original_sha256': current['sha256'], 'actual_wrong_profile_refused': True,
        'physical_cases': 5, 'native_64_case_credit': False, 'Original_Run_credit': False,
    })


if __name__ == '__main__':
    unittest.main()
