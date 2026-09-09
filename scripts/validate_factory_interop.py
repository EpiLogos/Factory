#!/usr/bin/env python3
import json
from pathlib import Path
from jsonschema import Draft202012Validator
from referencing import Registry, Resource

ROOT = Path(__file__).resolve().parents[1]

def load(rel):
    return json.loads((ROOT / rel).read_text(encoding='utf-8'))

def owner_failures(node, where='#'):
    failures = []
    if not isinstance(node, dict):
        return failures
    props = node.get('properties')
    if isinstance(props, dict):
        for name, child in props.items():
            owner = child.get('x-semantic-owner') if isinstance(child, dict) else None
            if not isinstance(owner, str) or not owner.strip():
                failures.append(f'{where}/{name}')
            failures.extend(owner_failures(child, f'{where}/{name}'))
    defs = node.get('$defs')
    if isinstance(defs, dict):
        for name, child in defs.items():
            failures.extend(owner_failures(child, f'{where}/$defs/{name}'))
    items = node.get('items')
    if isinstance(items, dict):
        failures.extend(owner_failures(items, f'{where}/items'))
    for i, child in enumerate(node.get('anyOf', [])):
        failures.extend(owner_failures(child, f'{where}/anyOf/{i}'))
    return failures

def whole_has_unmet_obligations(value):
    whole = value.get('operativeWhole', {})
    required = set(whole.get('requiredObligations', []))
    satisfied = set(whole.get('satisfiedObligations', []))
    return not required.issubset(satisfied)

schema_set = load('contracts/factory/interop/schema-set.json')
fixture_set = load('contracts/factory/fixtures/interop/fixture-set.json')
assert schema_set['schemaSetVersion'] == 'factory.interop/v1'
assert fixture_set['fixtureSetVersion'] == 'factory.interop-fixtures/v1'
assert fixture_set['contractVersion'] == 'factory.interop/v1'
schemas = {entry['instanceKey']: entry for entry in schema_set['schemas']}
sections = {entry['instanceKey']: entry for entry in fixture_set['sections']}
assert schemas.keys() == sections.keys(), 'schema/fixture manifest drift'
for key, entry in schemas.items():
    schema = load(entry['path'])
    fixture = load(sections[key]['path'])
    assert schema['$id'] == entry['id'], f'{key}: schema id drift'
    # The contract uses opaque version identifiers as $id values rather than URL
    # bases. jsonschema 4.10 treats local #/$defs refs as relative URLs when that
    # opaque $id is active. Validate the same schema body without using $id as a
    # retrieval base; the original identifier is checked immediately above.
    validation_schema = dict(schema)
    validation_schema.pop('$id', None)
    Draft202012Validator.check_schema(validation_schema)
    errors = sorted(Draft202012Validator(validation_schema).iter_errors(fixture), key=lambda e: list(e.path))
    assert not errors, f"{key}: " + '; '.join(e.message for e in errors)
    missing = owner_failures(schema)
    assert not missing, f'{key}: fields without one semantic owner: {missing}'

developmental_schema = load('contracts/factory/developmental-read.schema.json')
commission_schema = load('contracts/factory/commission.schema.json')
commission_request_schema = load('contracts/factory/commission-request.schema.json')
registry = Registry().with_resource(
    'https://github.com/EpiLogos/agent-system-design/contracts/factory/commission.schema.json',
    Resource.from_contents(commission_schema),
).with_resource(
    'https://github.com/EpiLogos/agent-system-design/contracts/factory/commission-request.schema.json',
    Resource.from_contents(commission_request_schema),
)
telemetry_fixture = load('contracts/factory/fixtures/execution-telemetry-reading.json')
validation_schema = dict(developmental_schema)
validation_schema.pop('$id', None)
Draft202012Validator.check_schema(validation_schema)
errors = sorted(
    Draft202012Validator(validation_schema, registry=registry).iter_errors(telemetry_fixture),
    key=lambda error: list(error.path),
)
assert not errors, 'execution telemetry: ' + '; '.join(error.message for error in errors)
model_usage = telemetry_fixture['modelUsage']
# Actuation issue #39 / PR #40 source-derived conformance evidence. Its owner test states that
# the shape/fields came from a real transcript while identifiers are fixture-local;
# this fixture does not claim the values are observed Factory execution history.
assert model_usage['owner'] == 'actuation'
assert model_usage['availability'] == 'available'
assert len(model_usage['observations']) == 1
owner_ref = model_usage['observations'][0]
assert owner_ref['revision'] == '5fefe920790b1beec05c258c9d65328539ba4e44'
assert owner_ref['contractSchemaDigest'] == '42215b3f06dffe5bfba53b0f51db6400d5b8739098c4fb1275f7a006579615cb'
usage = owner_ref['modelUsage']
assert usage['schema'] == 'actuation.model-usage/v1'
assert usage['model'] == {'standing': 'normalized-from-native', 'name': 'claude-fable-5', 'variant': 'standard'}
assert usage['tokens'] == {'standing': 'normalized-from-native', 'input': 2, 'output': 64000}
assert usage['cache'] == {'standing': 'normalized-from-native', 'read_input': 26788, 'creation_input': 53010}
assert usage['timing']['latency'] == {'standing': 'not-reported'}
assert usage['cost'] == {'standing': 'not-reported'}
assert usage['outcome'] == {'state': 'partial', 'standing': 'normalized-from-native', 'reason': 'max_tokens'}
assert usage['provenance']['raw_evidence_refs'] == ['trace:claude-code:fixture-line-1']
material_usage = telemetry_fixture['materialUsage']
assert material_usage['owner'] == 'workcell'
assert material_usage['availability'] == 'available'
assert len(material_usage['observations']) == 1
material_ref = material_usage['observations'][0]
assert material_ref['revision'] == '0b93a4af54ff3d547941d4af342e6a738c22e7af'
assert material_ref['contractSchemaDigest'] == '4e0e1cf8848ed1faf74bcc362976b47f81aad99b33eea458a789cb688f1b3d5f'
resource = material_ref['resourceUsage']
assert resource['schema'] == 'workcell.resource-usage/v1'
assert resource['usage_ref'] == material_ref['ref']
assert resource['ok'] is True and resource['status'] == 'ok'
assert resource['provider']['collection'] == 'bounded-on-demand'
assert resource['provider']['privacy'] == {
    'argv_collected': False,
    'environment_collected': False,
}
assert resource['metrics']['cpu_time']['standing'] == 'observed'
assert resource['metrics']['memory_rss']['standing'] == 'observed'
assert resource['metrics']['cpu_utilisation']['standing'] == 'derived'
for name in ('memory_peak_rss', 'gpu_utilisation', 'vram', 'network_bytes', 'storage_io_bytes'):
    assert resource['metrics'][name]['standing'] == 'unsupported'
    assert 'value' not in resource['metrics'][name]
for expected in (
    telemetry_fixture['provenance']['correlationRef'],
    telemetry_fixture['runRef'],
    telemetry_fixture['workflowUnitRef'],
    telemetry_fixture['executionRef'],
    *telemetry_fixture['condition']['workcellBindingRefs'],
):
    assert expected in resource['external_correlation_refs']
for invented_metric in ('tokens', 'cost', 'cpu', 'memory', 'gpu', 'network', 'storage'):
    assert invented_metric not in telemetry_fixture

anti = load(fixture_set['antiFixturesPath'])
by_id = {item['id']: item for item in anti['antiFixtures']}
plausible = by_id['plausible-artifact-partial-evidence-as-full-closure']
representative = by_id['representative-evidence-without-coverage-contract']
assert plausible['mustReject'] is True
assert plausible['value']['claimedClosure'] == 'full'
assert plausible['value']['claim']['confidence'] > 0.9
assert whole_has_unmet_obligations(plausible['value'])
assert representative['mustReject'] is True
assert representative['value']['claimedClosure'] == 'full'
assert representative['value']['evidenceMode'] == 'representative'
assert whole_has_unmet_obligations(representative['value'])
assert not representative['value']['samplingSufficiencyDeclared'] or not representative['value']['coverageConditionEvidenced']

print(
    f'JSON Schema CR-001 interop PASS ({len(schemas)} schemas; '
    'execution telemetry and whole-relative anti-fixtures present)'
)
