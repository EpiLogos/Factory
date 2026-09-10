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
        self.assertEqual('factory-rust', job['needs'])
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


if __name__ == '__main__':
    unittest.main()
