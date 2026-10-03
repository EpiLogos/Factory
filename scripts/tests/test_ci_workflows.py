"""Regression checks for the consolidated native CI coverage (not product proof)."""
from pathlib import Path
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / '.github/workflows'


class NativeWorkflowTests(unittest.TestCase):
    def setUp(self):
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
        self.workflow = yaml.load((WORKFLOWS / 'factory-native-evidence.yml').read_text(), Loader=yaml.BaseLoader)
        self.job = self.workflow['jobs']['native-attempt-evidence']
        self.steps = self.job['steps']

    def test_real_both_platforms_keep_distinct_failure_evidence(self):
        self.assertEqual(['ubuntu-latest', 'macos-14'], self.job['strategy']['matrix']['os'])
        self.assertEqual('false', self.job['strategy']['fail-fast'])
        upload = next(step for step in self.steps if step.get('uses') == 'actions/upload-artifact@v4')
        self.assertEqual('${{ always() }}', upload['if'])
        self.assertIn('runner.os', upload['with']['name'])

    def test_binary_admission_requires_every_actual_native_leg(self):
        import re
        gates = {step['id'] for step in self.steps if 'id' in step and step['id'] != 'build'}
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
        self.assertIn('--exact --test-threads=1', preparation['run'])
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
        self.assertEqual(8, len(sets['groups']['preparation']))
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
        self.assertIn('--lib attempt_owner_dispatch:: -- --list', dispatch['run'])
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
        self.assertIn('--no-run --message-format=json', receiving['run'])
        self.assertIn('compiler --role receiving', receiving['run'])
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


if __name__ == '__main__':
    unittest.main()
