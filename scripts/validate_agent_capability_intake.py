#!/usr/bin/env python3
"""Conformance lane for the composed agent-capability intake (#188).

Binds this repository's intake evidence to the exact Central and ai-kit
revisions pinned from O:I `suite/mainline.json`. Deterministic offline mode is
what CI runs: it proves the recorded fixture envelopes cite exactly the pinned
revisions and carry internally consistent owner identities. `--live` compares
the pins against O:I main on GitHub and fails loudly on pin drift with
re-record instructions.

No live checkout, Central ground or AIKit home is ever read or written by this
lane; fixtures were captured by executing the pinned revisions in isolation.
"""
import argparse
import json
import re
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INTAKE_DIR = Path('contracts/factory/fixtures/intake')

PIN_SCHEMA = 'factory.agent-capability-intake-pins/v1'
FIXTURE_SCHEMA = 'factory.agent-capability-intake-fixture/v1'
CENTRAL_PROFILE_SCHEMA = 'central.agent-profile/v1'
MAINLINE_URL = 'https://raw.githubusercontent.com/EpiLogos/O-I/main/suite/mainline.json'
GENERATION_ID = re.compile(r'^gen_[0-9a-z]+$')
AIKIT_WITHHELD_KINDS = {'not-selected', 'unavailable'}
AIKIT_UNAVAILABLE_REASONS = {
    'not-in-catalog',
    'retired-standing',
    'trust-required',
    'quarantined',
    'denied-by-policy',
    'platform-unsupported',
    'no-supported-target',
    'blocked',
    'dependency-unavailable',
}


def load(rel):
    return json.loads((ROOT / rel).read_text(encoding='utf-8'))


def fail(message):
    print(f'agent-capability intake conformance: FAIL: {message}', file=sys.stderr)
    sys.exit(1)


def check_pins():
    pins = load(INTAKE_DIR / 'conformance-pins.json')
    if pins['schema'] != PIN_SCHEMA:
        fail(f'pins schema {pins["schema"]} is not {PIN_SCHEMA}')
    for product in ('central', 'ai-kit'):
        pin = pins['pins'][product]
        if not re.fullmatch(r'[0-9a-f]{40}', pin['revision']):
            fail(f'{product} pin {pin["revision"]} is not a full commit SHA')
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', pins['observed_at']):
        fail(f'pins observed_at {pins["observed_at"]} is not a date')
    if pins['source_of_truth']['path'] != 'suite/mainline.json':
        fail('pin source_of_truth must be O:I suite/mainline.json')
    return pins


def check_authored_fixture(pins):
    fixture = load(INTAKE_DIR / 'central-authored-envelope.json')
    if fixture['schema'] != FIXTURE_SCHEMA:
        fail(f'authored fixture schema {fixture["schema"]} is not {FIXTURE_SCHEMA}')
    captured = fixture['captured_from']
    if captured['product'] != 'central':
        fail('authored fixture must be captured from central')
    pinned = pins['pins']['central']['revision']
    if captured['revision'] != pinned:
        fail(
            'authored fixture cites Central revision '
            f'{captured["revision"]} but the lane pins {pinned}. '
            'Re-capture the envelope from the pinned revision (see '
            'rebind_instructions in conformance-pins.json).'
        )
    profile = fixture['envelope']['profile']
    if profile['schema'] != CENTRAL_PROFILE_SCHEMA:
        fail(f'authored profile schema {profile["schema"]} is not {CENTRAL_PROFILE_SCHEMA}')
    for field in ('ref', 'revision', 'agent_ref', 'scope', 'world_ref'):
        value = profile.get(field)
        if not isinstance(value, str) or not value.strip():
            fail(f'authored profile is missing {field}')
    if profile['scope'] not in ('personal', 'project'):
        fail(f'authored profile scope {profile["scope"]} is not a Central store scope')
    if not profile['skill_set_refs']:
        fail('authored profile assigns no SkillSet; the three-way relation needs one')
    return profile


def check_effective_fixture(pins):
    fixture = load(INTAKE_DIR / 'aikit-effective-envelope.json')
    if fixture['schema'] != FIXTURE_SCHEMA:
        fail(f'effective fixture schema {fixture["schema"]} is not {FIXTURE_SCHEMA}')
    captured = fixture['captured_from']
    if captured['product'] != 'ai-kit':
        fail('effective fixture must be captured from ai-kit')
    pinned = pins['pins']['ai-kit']['revision']
    if captured['revision'] != pinned:
        fail(
            'effective fixture cites ai-kit revision '
            f'{captured["revision"]} but the lane pins {pinned}. '
            'Re-capture the envelope from the pinned revision (see '
            'rebind_instructions in conformance-pins.json).'
        )
    envelope = fixture['envelope']
    set_name = envelope['set']['name']
    if not set_name or '@' in set_name or '/' in set_name:
        fail(f'set name {set_name!r} cannot round-trip the aikit.skillset ref grammar')
    generation_id = envelope['generation']['generation_id']
    if not GENERATION_ID.fullmatch(generation_id):
        fail(f'generation id {generation_id} is not an AIKit GenerationId (gen_...)')
    projected = envelope.get('projected', [])
    withheld = envelope.get('withheld', [])
    total = envelope.get('members')
    if total is None or total != len(projected) + len(withheld):
        fail(
            f'envelope member accounting is dishonest: members={total}, '
            f'projected={len(projected)}, withheld={len(withheld)}'
        )
    saw_retired = saw_unresolved = False
    for member in withheld:
        capability = member.get('capability')
        if not capability:
            fail('withheld member is missing capability')
        reason_tag = member.get('withheld_reason', {})
        kind = reason_tag.get('withheld')
        if kind not in AIKIT_WITHHELD_KINDS:
            fail(f'withheld member {capability} carries unknown withheld kind {kind}')
        if kind == 'unavailable':
            unavailable = reason_tag.get('reason')
            if unavailable not in AIKIT_UNAVAILABLE_REASONS:
                fail(
                    f'withheld member {capability} carries unknown unavailable '
                    f'reason {unavailable}'
                )
            if unavailable == 'retired-standing':
                if not reason_tag.get('retirement_reason', '').strip():
                    fail(f'retired member {capability} must carry the owner reason')
                saw_retired = True
            if unavailable == 'not-in-catalog':
                saw_unresolved = True
        if not member.get('reason', '').strip():
            fail(f'withheld member {capability} must carry the resolver own reason')
    if not saw_retired:
        fail('the recorded evidence carries no retired member; the K2 honesty proof needs one')
    if not saw_unresolved:
        fail('the recorded evidence carries no unresolved member; the intake proof needs one')
    return envelope


def check_live(pins):
    try:
        with urllib.request.urlopen(MAINLINE_URL, timeout=30) as response:
            mainline = json.loads(response.read().decode('utf-8'))
    except Exception as error:  # noqa: BLE001 - report, do not half-fail
        fail(f'--live could not read {MAINLINE_URL}: {error}')
    drift = []
    for product in ('central', 'ai-kit'):
        live = next(
            (entry['revision'] for entry in mainline['products'] if entry['id'] == product),
            None,
        )
        pinned = pins['pins'][product]['revision']
        if live != pinned:
            drift.append(
                f'{product}: lane pins {pinned}, O:I mainline.json (observed_at '
                f'{mainline["observed_at"]}) pins {live}'
            )
    if drift:
        fail(
            'pin drift detected:\n  ' + '\n  '.join(drift) + '\n'
            'The intake lane binds to recorded envelopes, never live heads. '
            'Re-capture the fixtures from the new pinned revisions and update '
            'conformance-pins.json per its rebind_instructions.'
        )
    print(
        'live pin check: no drift '
        f'(observed_at {mainline["observed_at"]})'
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        '--live',
        action='store_true',
        help='also compare the pins against O:I suite/mainline.json on GitHub',
    )
    arguments = parser.parse_args()
    pins = check_pins()
    profile = check_authored_fixture(pins)
    envelope = check_effective_fixture(pins)
    authored_set = profile['skill_set_refs'][0].removeprefix('skill-set:')
    if authored_set != envelope['set']['name']:
        fail(
            f'authored SkillSet assignment {authored_set} does not name the '
            f'effective set {envelope["set"]["name"]}: the three-way relation is broken'
        )
    print(
        'agent-capability intake conformance: pinned central@'
        f'{pins["pins"]["central"]["revision"][:12]} ai-kit@'
        f'{pins["pins"]["ai-kit"]["revision"][:12]} '
        f'(observed_at {pins["observed_at"]}); fixtures consistent.'
    )
    if arguments.live:
        check_live(pins)


if __name__ == '__main__':
    main()
