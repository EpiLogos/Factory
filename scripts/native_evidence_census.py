"""Validate complete actual libtest captures; never infer an owner effect from exit0."""
import argparse
import json
from pathlib import Path
import re

CASE_SETS = Path(__file__).with_name("native_evidence_case_sets.json")
MAX_LOG_BYTES = 16 * 1024 * 1024


def complete_text(path):
    with path.open("rb") as stream:
        data = stream.read(MAX_LOG_BYTES + 1)
    if len(data) > MAX_LOG_BYTES:
        raise ValueError("native log exceeds the admission bound; retained file is not qualified")
    return data.decode("utf-8", errors="strict")


def validate_list(text, expected, prefix=None):
    names = re.findall(r"^(.+): test$", text, re.MULTILINE)
    selected = [name for name in names if prefix is None or name.startswith(prefix)]
    if len(selected) != len(set(selected)) or set(selected) != set(expected):
        raise ValueError(f"native definition census mismatch: expected {len(expected)}, actual {len(selected)}")


def validate_run(text, case):
    # Default libtest capture keeps body diagnostics separate from status lines.
    # The caller preserves Cargo's real nonzero status via pipefail as well.
    if not text.endswith("\n"):
        raise ValueError("unterminated native run capture")
    statuses = re.findall(r"^test (.+) \.\.\. (.+)$", text, re.MULTILINE)
    headers = re.findall(r"^running (\d+) tests?$", text, re.MULTILINE)
    result_lines = [line for line in text.splitlines() if line.startswith("test result:")]
    pattern = r"test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; \d+ filtered out; finished in \d+\.\d+s"
    if statuses != [(case, "ok")] or headers != ["1"] or len(result_lines) != 1 or re.fullmatch(pattern, result_lines[0]) is None:
        raise ValueError("native exact case did not execute exactly one successful, unignored complete body")
    # --exact intentionally filters the other compiled definitions. Their complete
    # independent --list census is checked before any individual case is admitted.


def main():
    p = argparse.ArgumentParser()
    p.add_argument("operation", choices=["names", "list", "run"])
    p.add_argument("group", choices=["capture", "receiving", "consumer-os", "preparation", "owner-dispatch", "paired", "cancellation-guard"])
    p.add_argument("--input", type=Path)
    p.add_argument("--case")
    a = p.parse_args()
    sets = json.loads(CASE_SETS.read_text())
    expected = sets["groups"][a.group]
    if not expected or len(expected) != len(set(expected)):
        raise ValueError("empty or duplicate expected native census")
    if a.operation == "names":
        print("\n".join(expected))
        return
    if a.input is None:
        p.error("actual retained --input is required")
    text = complete_text(a.input)
    if a.operation == "list":
        if a.group == "receiving":
            validate_list(text, expected + [sets["integration_child_definition"]])
        elif a.group == "preparation":
            validate_list(text, expected)
        elif a.group == "paired":
            validate_list(text, expected, "attempt_receiving::paired_inclusion_native_tests::")
        elif a.group == "cancellation-guard":
            validate_list(text, expected, "orchestration::native_cancellation_guard_tests::")
        elif a.group == "capture":
            validate_list(text, expected, "native_process::")
        elif a.group == "owner-dispatch":
            validate_list(text, expected, "attempt_owner_dispatch::")
        else:
            selected = [line for line in text.splitlines() if "capture_consumer_os_tests::" in line or "actual_capture_retention_tests::" in line]
            validate_list("\n".join(selected), expected)
    else:
        if a.case not in expected:
            p.error("case is outside the independently declared native census")
        validate_run(text, a.case)
    print(json.dumps({"operation": a.operation, "group": a.group, "case": a.case, "actual_log": str(a.input), "census_admitted": True, "Original_Run_credit": False}))


if __name__ == "__main__":
    main()
