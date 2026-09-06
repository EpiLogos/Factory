#!/usr/bin/env python3
"""Real Pi/AIKit ACP proposal followed by separately authorised materialisation.

Requires Python blake3, explicit native binaries and configured Pi provider.
No provider reply is fabricated. This phase never enables Pi's mutation tools.
Use the separate Workcell proof helper on the retained candidate afterward.
"""
import argparse
import hashlib
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import tempfile
import time

import blake3

ROOT = Path(__file__).resolve().parents[1]
NAMES = ("factory-development", "factory-operation", "factory-bounded-work")


def execute(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any((output / name).exists() for name in ("acceptance.json", "candidate.json", "required-context.json")):
        raise RuntimeError("Refusing to overwrite retained ACP evidence; choose a fresh output directory")
    root = Path(tempfile.mkdtemp(prefix="factory-acp-work-")).resolve()
    central_root = root / "Central"
    subprocess.run([str(args.ctrl), "--json", "--root", str(central_root), "init"],
                   capture_output=True, text=True, check=True, timeout=30)
    work = central_root / "Work/factory-bounded-acceptance"
    (work / "ProjectCentral/user").mkdir(parents=True)
    (work / "ProjectCentral/project.json").write_text(json.dumps({
        "schema": "central.project/v1", "project_id": "factory-bounded-acceptance",
        "human_source": "ProjectCentral/user",
        "wiki": {"profile": "okf-wiki/v1", "source": "ProjectCentral/agents/wiki/wiki.json"},
    }))
    home = root / "aikit"
    env = dict(os.environ, AIKIT_HOME=str(home), CENTRAL_ROOT=str(central_root))
    env.pop("AIKIT_ISOLATION", None)
    env.pop("AIKIT_CONTEXT_ID", None)
    env.pop("AIKIT_SESSION_ID", None)
    source = ROOT / "skills"
    pricing = work / "pricing.py"
    pricing.write_text("def inclusive_total(prices, tax_rate):\n    return sum(prices) + tax_rate\n")
    baseline = subprocess.run(
        [sys.executable, "-c", "import runpy; print(runpy.run_path('pricing.py')['inclusive_total']([10,10],0.1))"],
        cwd=work, capture_output=True, text=True, check=True, timeout=10,
    )
    assert float(baseline.stdout) == 20.1 and float(baseline.stdout) != 22.0
    (output / "baseline.json").write_text(json.dumps({
        "observed": float(baseline.stdout), "required": 22.0,
        "artifact_sha256": hashlib.sha256(pricing.read_bytes()).hexdigest(),
        "exit_code": baseline.returncode,
    }, indent=2) + "\n")
    commission = work / "ProjectCentral/user/commission.md"
    commissioned = (
        "Temporary user-authorised acceptance commission. Propose a repair to pricing.py: "
        "inclusive_total must apply tax to the sum of prices. Preserve its two-argument API. "
        "One model proposal; no model tools, delegation, external posts or autonomous retries. "
        "The native Workcell proof may validate and apply this candidate once in this disposable fixture. "
        "Return evidence; neither a model statement nor this temporary seed is adopted human law.\n"
    )
    commission.write_text(commissioned)

    def cli(binary, *command, envelope=False):
        completed = subprocess.run(
            [str(binary), "-C", str(work), *command], env=env,
            text=True, capture_output=True, timeout=90,
        )
        if completed.returncode:
            raise RuntimeError(f"Native command {command[0]} failed: {completed.stderr} {completed.stdout}")
        value = json.loads(completed.stdout)
        if envelope:
            assert value["ok"], value
            return value["data"]
        return value

    def aikit(*command):
        return cli(args.aikit, "--json", *command, envelope=True)

    aikit("source", "add-directory", "factory-native", str(source))
    aikit("source", "sync", "factory-native")
    aikit("source", "promote", "factory-native", "--trust")
    snapshot = aikit("source", "show", "factory-native")
    registry = Path(snapshot["active_registry"])
    registry.relative_to(root)
    ids = [f"skill/factory-native/{name}" for name in NAMES]
    aikit("set", "create", "factory-bounded-development", *ids)
    for identity in ids:
        aikit("enable", identity, "--scope", "global")
    selected = aikit("set", "show", "factory-bounded-development")
    assert selected["complete"] and set(selected["projected"]) == set(ids)
    # Native source/profile and body selection precede provider construction.
    # These are temporary commissioned declarations, not adopted human ground.
    aikit("project", "bind", "factory-bounded-acceptance", "--directory", str(work),
          "--no-default-skill-sets")
    commission_ref = "central:source:project:factory-bounded-acceptance:ProjectCentral/user/commission.md"
    method_path = source / "factory-bounded-work/method.json"
    method = aikit("method", "resolve", "--source", str(method_path))
    definition_path = ROOT / "agents/factory-mode/profile.json"
    expression_path = ROOT / "agents/factory-mode/intent.md"
    definition = json.loads(definition_path.read_text())
    assert expression_path.read_text().strip()
    assert definition["skill_refs"] == ids and definition["method_refs"] == [method["method"]["id"]]
    profile = {**definition,
        "ref": "profile/factory-bounded-acceptance",
        "revision": "commission-v1", "source_profile_ref": definition["ref"],
        "scope": "project", "world_ref": "project:factory-bounded-acceptance",
        "ratified_world_refs": ["project:factory-bounded-acceptance"],
        "governance_refs": [*definition["governance_refs"], commission_ref],
        "knowledge_source_refs": [*definition["knowledge_source_refs"], commission_ref],
        "provenance_refs": [*definition["provenance_refs"],
            "central:source:project:Software-Factory:agents/factory-mode/profile.json", commission_ref],
    }
    saved = subprocess.run([str(args.ctrl), "--json", "--root", str(central_root),
        "action", "run", "agent-profile.save", json.dumps({"scope": "project",
        "project": "factory-bounded-acceptance", "profile": profile})],
        env=env, text=True, capture_output=True, timeout=30)
    assert saved.returncode == 0, (saved.stdout, saved.stderr)
    profile_receipt = json.loads(saved.stdout)
    assert profile_receipt["ok"], profile_receipt

    def actuation(*command):
        completed = subprocess.run([str(args.actuation), *command, "--json"],
            env=env, cwd=work, capture_output=True, text=True, check=True, timeout=60)
        return json.loads(completed.stdout)

    binding = {"schema": "actuation.agency/v1",
        "binding_ref": "world-binding/factory-bounded-acceptance",
        "agent_ref": profile["agent_ref"], "agency_ref": "agency/factory-bounded-acceptance",
        "world_ref": profile["world_ref"], "scope_ref": profile["world_ref"],
        "purpose_ref": commission_ref, "bounds_refs": [commission_ref],
        "authority_refs": [commission_ref]}
    binding_path = output / "world-binding.json"
    binding_path.write_text(json.dumps({"binding": binding}, indent=2) + "\n")
    agency = actuation("agency", str(binding_path))
    model_ref = "model/" + args.model
    instantiation_input = {
        "schema": "actuation.instantiation/v1",
        "actuation_ref": "actuation/factory-bounded-acceptance",
        "agency_ref": binding["agency_ref"], "world_binding_ref": binding["binding_ref"],
        "harness_ref": "harness/pi", "agent_session_ref": "agent-session/factory-bounded-acceptance",
        "model_relation": {"schema": "actuation.instantiation/v1", "model_ref": model_ref,
            "inference_surface": {"contract_ref": "acp/v1",
                "facts": {"requested_pi_model": args.model, "provider_readiness_observed": False}}},
        "access_profile": {"schema": "actuation.instantiation/v1",
            "inference": {"allowed": ["invoke"], "denied": []},
            "control": {"allowed": [], "denied": ["tools", "delegation"]},
            "interior": {"depth": "outputs", "allowed": [], "denied": []}},
        "bounds_refs": [commission_ref], "evidence_refs": [commission_ref],
        "observed_at": datetime.now(timezone.utc).isoformat(),
    }
    input_path = output / "instantiation-input.json"
    input_path.write_text(json.dumps(instantiation_input, indent=2) + "\n")
    instantiation = actuation("instantiation", "record", str(input_path))
    assert Path(instantiation["harness_receipts"]["executable"]).resolve() == args.pi.resolve(), instantiation
    (work / ".aikit").mkdir(exist_ok=True)
    (work / ".aikit/actuation-model-bearing.json").write_text(json.dumps(instantiation))
    composition = aikit("compose")
    composed = composition["composed_inputs"]
    assert composed["authored_basis"]["profile_source"]["ref"] == profile["ref"], composed
    assert composed["authored_basis"]["profile_source"]["source_profile_ref"] == definition["ref"], composed
    assert composed["authored_basis"]["profile_source"]["purpose"] == definition["purpose"], composed
    assert definition["knowledge_source_refs"][0] in composed["authored_basis"]["profile_source"]["knowledge_source_refs"], composed
    assert composed["selected_harness"] == "harness/pi" and composed["selected_model"] == model_ref, composed
    # Re-resolve after composition input exists, retaining the actual native
    # ContextResolution rather than promoting a METHOD description to evidence.
    method = aikit("method", "resolve", "--source", str(method_path))
    assert all(item["active"] for item in method["skill_states"]), method
    receipts = {"agent-profile.json": profile_receipt, "agency.json": agency,
        "instantiation.json": instantiation, "composition.json": composition,
        "method-resolution.json": method}
    for filename, value in receipts.items():
        (output / filename).write_text(json.dumps(value, indent=2) + "\n")
    setup = {"fixture_root": str(work), "aikit_home": str(home), "central_root": str(central_root),
        "commission_path": str(commission), "authority_ref": commission_ref,
        "method_resolution": str(output / "method-resolution.json"),
        "agent_definition": {"profile": str(definition_path), "expression": str(expression_path),
            "source_profile_ref": definition["ref"], "agent_ref": definition["agent_ref"],
            "profile_sha256": hashlib.sha256(definition_path.read_bytes()).hexdigest(),
            "expression_sha256": hashlib.sha256(expression_path.read_bytes()).hexdigest()},
        "requested_model": args.model, "selected_model_ref": model_ref,
        "actual_provider_model_observed": False,
        "catalog_resolution": {name: composition["plan"][name]["state"] for name in ("agent", "agency", "model", "harness")},
        "standing": "native source composition and harness detection; no provider effect yet"}
    (output / "setup.json").write_text(json.dumps(setup, indent=2) + "\n")
    if args.setup_only:
        print(json.dumps(setup, indent=2))
        return
    bodies = [registry / "capsules/skill/factory-native" / name / "payload/SKILL.md" for name in NAMES]
    bodies.append(bodies[-1].parent / "references/repair.md")
    profile_source = (work / profile_receipt["data"]["source_path"]).resolve()
    profile_source.relative_to(work)
    primary_paths = [profile_source, binding_path, work / ".aikit/actuation-model-bearing.json",
                     work / "ProjectCentral/project.json"]
    paths = [commission, *bodies, method_path, definition_path, expression_path,
             *primary_paths, *(output / name for name in receipts)]
    required = {
        "sources": [{
            "source": f"source/acceptance/{index}",
            "revision": ("commission-v1" if index == 0 else snapshot["active_snapshot"]
                         if path in bodies else "blake3:" + blake3.blake3(path.read_bytes()).hexdigest()),
            "path": str(path),
            "content_digest": "blake3:" + blake3.blake3(path.read_bytes()).hexdigest(),
        } for index, path in enumerate(paths)],
    }
    (output / "required-context.json").write_text(json.dumps(required, indent=2) + "\n")
    launcher = root / "pi-tool-less"
    selected_pi_model = composed["selected_model"].removeprefix("model/")
    assert selected_pi_model == instantiation["model_relation"]["inference_surface"]["facts"]["requested_pi_model"]
    argv = [str(args.pi), "--offline", "--model", selected_pi_model, "--no-tools", "--no-extensions",
            "--no-skills", "--no-context-files", "--no-prompt-templates", "--no-session"]
    launcher.write_text("#!/bin/sh\nexec " + shlex.join(argv) + ' "$@"\n')
    launcher.chmod(0o700)
    env["PI_ACP_PI_COMMAND"] = str(launcher)
    space = "session-space/factory-bounded-acceptance"
    session = "agent-session/factory-bounded-acceptance"

    def apply(preview):
        path = root / "preview.json"
        path.write_text(json.dumps(preview))
        return cli(args.session_space, "apply", "--preview-json", "@" + str(path))

    apply(cli(args.session_space, "create", space, "--label", "Factory bounded real acceptance"))
    apply(cli(args.session_space, "stage", "--space", space, "--intent-json", json.dumps({
        "operation": "attach-agent-session", "attachment": {
            "agent_session": session, "purpose": "Real bounded Factory proposal",
            "provenance": ["Explicit temporary acceptance commission"],
        },
    })))
    provider = {"id": "pi-acp-bounded", "label": "Pi ACP bounded proposal",
                "argv": args.bridge_argv, "required_context": required}
    cli(args.session_space, "encounter-configure", "--provider-json", json.dumps(provider))
    sock = root / "ipc/owner.sock"
    log = (output / "resident.log").open("w")
    # The resident may outlive a rebuild of its on-disk CLI. Attribute this
    # launch to the observed executable, not whatever bytes exist at return.
    session_space_digest = hashlib.sha256(args.session_space.read_bytes()).hexdigest()
    server = subprocess.Popen(
        [str(args.session_space), "-C", str(work), "encounter-serve", "--socket", str(sock)],
        env=env, stdout=log, stderr=log, start_new_session=True,
    )

    def request(action, raw=False, **fields):
        value = cli(args.session_space, "encounter", "--socket", str(sock),
                    "--request-json", json.dumps({"action": action, **fields}))
        if raw:
            return value
        assert value["ok"], value
        return value["data"]

    try:
        deadline = time.monotonic() + 20
        while not sock.exists():
            assert server.poll() is None and time.monotonic() < deadline, "Resident startup failed"
            time.sleep(.05)
        opened = request("open", space=space, agent_session=session, provider=provider["id"], cwd=str(work))
        observation = opened.get("model_observation")
        assert observation and observation["current_model_id"] == selected_pi_model, ("Requested model differs from ACP provider report", observation)
        assert selected_pi_model in [item["modelId"] for item in observation["available_models"]], observation
        (output / "model-observation.json").write_text(json.dumps(observation, indent=2) + "\n")
        # Drain startup independently; neither a banner nor initial status is a response.
        cursor = 0
        for _ in range(5):
            page = request("read", agent_session=session, after=cursor, limit=128)
            cursor = page["next_cursor"]
            time.sleep(.15)
        # Required-source absence/staleness must preserve the draft and never
        # submit the user message to the already resident native session.
        draft = request("draft", agent_session=session, basis=0, text="Must remain unsubmitted")
        commission.unlink()
        missing = request("prompt", raw=True, agent_session=session, draft_revision=draft["revision"])
        assert not missing["ok"] and missing["error"]["code"] == "encounter.context_unavailable", missing
        commission.write_text(commissioned + "Changed after admission.\n")
        stale = request("prompt", raw=True, agent_session=session, draft_revision=draft["revision"])
        assert not stale["ok"] and stale["error"]["code"] == "encounter.context_stale", stale
        retained = request("read", agent_session=session, after=cursor, limit=128)
        assert retained["draft"] == draft
        assert all(row["event"]["kind"] != "user-message" for row in retained["events"])
        cursor = retained["next_cursor"]
        commission.write_text(commissioned)
        # Text is taken from the real immutable AIKit payload, with its identity
        # and revision disclosed. This is explicit ACP context delivery, not a
        # claim that Pi discovered/reloaded a filesystem Skill catalogue.
        rendered = "\n\n".join(
            f"SOURCE {pin['source']} REVISION {pin['revision']}\n{path.read_text()}"
            for pin, path in zip(required["sources"], paths)
        )
        prompt = rendered + "\n\n" + (
            "Perform the proposed repair part of this commissioned Method. The native controller owns "
            "the Run and will execute/verify the returned candidate through Workcell; you have no tools. "
            "The actual fixture pricing.py currently contains:\n" + pricing.read_text() +
            "\nObserved failing case: inclusive_total([10, 10], 0.1) returns 20.1, required 22.0. "
            "Also preserve empty prices and zero tax behavior. Return exactly one JSON object "
            "with sole key source containing the complete replacement Python source. "
            "Use exactly def inclusive_total(prices, tax_rate) with one return expression: "
            "arithmetic, numeric literals, the two arguments and sum(prices) only. "
            "No imports, decorators, comments, side effects or Markdown. This is a proposed candidate; "
            "do not claim that you ran checks or that the whole Run is complete."
        )
        (output / "delivered-context.txt").write_text(prompt)
        draft = request("draft", agent_session=session, basis=draft["revision"], text=prompt)
        request("prompt", agent_session=session, draft_revision=draft["revision"])
        text = ""
        events = []
        terminal = None
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline and terminal is None:
            page = request("read", agent_session=session, after=cursor, limit=128)
            events.extend(page["events"])
            for item in page["events"]:
                host = item["event"].get("event", {})
                if "Signal" in host:
                    kind = host["Signal"]["kind"]
                    if kind["kind"] == "agent-message-chunk":
                        text += kind["text"]
                if "TurnEnded" in host:
                    terminal = host["TurnEnded"]["stop"]
            cursor = page["next_cursor"]
            if not page["more"]:
                time.sleep(.1)
        assert terminal and "Completed" in terminal, terminal
        candidate = json.loads(text.strip())
        assert set(candidate) == {"source"} and isinstance(candidate["source"], str)
        candidate_file = output / "candidate.json"
        candidate_file.write_text(json.dumps(candidate, indent=2) + "\n")
        (output / "events.json").write_text(json.dumps(events, indent=2) + "\n")
        assert pricing.read_text().endswith("return sum(prices) + tax_rate\n"), "Tool-less proposal mutated fixture"
        result = {
            "result": "passed", "fixture_root": str(work), "candidate_json": str(candidate_file),
            "canonical_session": session, "native_session": opened["native_session_id"],
            "snapshot": snapshot["active_snapshot"], "members": ids,
            "missing_required_source_refused": True, "stale_required_source_refused": True,
            "refused_draft_preserved": True, "explicit_acp_context_delivery": True,
            "delivered_context_sha256": hashlib.sha256(prompt.encode()).hexdigest(),
            "required_context_sha256": hashlib.sha256((output / "required-context.json").read_bytes()).hexdigest(),
            "model": args.model, "provider_reported_model": observation, "model_tools": "disabled", "candidate_is_proposal": True,
            "authority_ref": commission_ref,
            "composition_setup": setup, "native_method_resolution": str(output / "method-resolution.json"),
            "session_space_binary_sha256": session_space_digest,
            "session_space_file_changed_during_run": hashlib.sha256(args.session_space.read_bytes()).hexdigest() != session_space_digest,
            "scope": "Real resolved Skills → required source preflight → resident ACP → actual Pi candidate. Material application, Factory evidence and human acceptance remain separate.",
        }
        (output / "acceptance.json").write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result, indent=2))
    finally:
        cleanup = {"schema": "aikit.owner-shutdown-acceptance/v1", "owner_pid": server.pid}
        cleanup_error = None
        try:
            if server.poll() is not None:
                raise RuntimeError("Owner exited before a native provider-cleanup acknowledgement")
            health = request("health")
            assert health["pid"] == server.pid, "Socket identifies another owner; refusing shutdown"
            ack = request("shutdown", expected_pid=health["pid"])
            assert ack.get("shutdown") is True and ack["pid"] == server.pid, ack
            server.wait(timeout=15)
            assert server.returncode == 0, "Owner failed after native shutdown acknowledgement"
            cleanup.update(result="passed", acknowledgement=ack, owner_exit_code=server.returncode)
        except Exception as failure:
            cleanup_error = failure
            cleanup.update(result="failed", reason=str(failure),
                           scope="Native cleanup unconfirmed; independent provider groups may remain")
            # Last-resort containment targets only the Popen-owned server PID.
            # It cannot establish cleanup of separate provider process groups.
            if server.poll() is None:
                server.terminate()
                try:
                    server.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait(timeout=10)
                cleanup["fallback"] = "terminated-only-owned-server-pid"
        finally:
            (output / "owner-shutdown.json").write_text(json.dumps(cleanup, indent=2) + "\n")
            acceptance_path = output / "acceptance.json"
            if acceptance_path.exists():
                retained_result = json.loads(acceptance_path.read_text())
                retained_result["owner_shutdown"] = cleanup
                if cleanup_error is not None:
                    retained_result["result"] = "failed-owner-cleanup"
                acceptance_path.write_text(json.dumps(retained_result, indent=2) + "\n")
            log.close()
        if cleanup_error is not None:
            raise RuntimeError("Native encounter cleanup failed; inspect owner-shutdown.json; no provider cleanup is claimed") from cleanup_error


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ctrl", type=Path, required=True)
    parser.add_argument("--actuation", type=Path, required=True)
    parser.add_argument("--setup-only", action="store_true", help="Verify native source composition without starting ACP or a model")
    parser.add_argument("--aikit", type=Path, required=True)
    parser.add_argument("--session-space", type=Path, required=True)
    parser.add_argument("--pi", type=Path, required=True)
    parser.add_argument("--bridge-argv", type=json.loads, required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--output", type=Path, required=True)
    execute(parser.parse_args())
