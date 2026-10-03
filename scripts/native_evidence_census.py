"""Validate complete actual libtest captures; never infer an owner effect from exit0."""
import argparse
import json
import hashlib
import os
import stat
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


GROUPS = ("capture", "receiving", "consumer-os", "preparation",
          "owner-dispatch", "paired", "cancellation-guard")
MAX_EVIDENCE_ENTRIES = 8192


def portable_log_path(group, case):
    if group not in GROUPS or not isinstance(case, str) or not case:
        raise ValueError("undeclared evidence group or empty case identity")
    # Only the disposable filename changes. The exact case remains in its sidecar
    # and in the genuine libtest body; this digest grants no semantic identity.
    digest = hashlib.sha256(case.encode("utf-8", errors="strict")).hexdigest()
    return Path(group + "-" + digest + ".log")


def case_path_mapping(sets):
    mapping = {}
    paths = set()
    cases = set()
    for group in GROUPS:
        expected = sets["groups"][group]
        if not expected or len(expected) != len(set(expected)):
            raise ValueError("empty or duplicate expected native census")
        for case in expected:
            path = portable_log_path(group, case)
            if case in cases or path in paths:
                raise ValueError("duplicate native identity or evidence filename collision")
            cases.add(case)
            paths.add(path)
            mapping[(group, case)] = path
    return mapping


def declare_log_path(directory, group, case, sets):
    mapping = case_path_mapping(sets)
    if (group, case) not in mapping:
        raise ValueError("case is outside the declared native census")
    if not stat.S_ISDIR(directory.lstat().st_mode):
        raise ValueError("actual evidence directory is not ordinary")
    log = directory / mapping[(group, case)]
    try:
        log.lstat()
    except FileNotFoundError:
        pass
    else:
        raise ValueError("native case log already exists; refusing replacement")
    sidecar = log.with_suffix(".identity.json")
    record = {"schema": "factory.native-case-log-projection/v1",
              "group": group, "case": case, "actual_log": str(log),
              "before_dispatch": True, "native_body_admitted": False,
              "Original_Run_credit": False}
    # Exclusive creation refuses both duplicate dispatch and a substituted alias.
    # It retains the declared case-to-path relation before Cargo is launched.
    with sidecar.open("x", encoding="utf-8") as stream:
        stream.write(json.dumps(record, indent=2) + "\n")
    return log


def validate_portable_component(name):
    forbidden = set('":<>|*?\\')
    if (not name or any(c in forbidden or ord(c) < 32 for c in name)
            or name.endswith((".", " "))
            or name.split(".", 1)[0].upper() in
            {"CON", "PRN", "AUX", "NUL", *["COM" + str(n) for n in range(1, 10)],
             *["LPT" + str(n) for n in range(1, 10)]}):
        raise ValueError("actual evidence path component is not portable")


def validate_upload_tree(directory):
    if not stat.S_ISDIR(directory.lstat().st_mode):
        raise ValueError("actual evidence root is not ordinary")
    count = 0
    files = 0

    def walk(parent, depth):
        nonlocal count, files
        if depth > 64:
            raise ValueError("finite evidence path depth exceeded")
        with os.scandir(parent) as entries:
            for entry in entries:
                count += 1
                if count > MAX_EVIDENCE_ENTRIES:
                    raise ValueError("finite evidence path census exceeded")
                validate_portable_component(entry.name)
                form = entry.stat(follow_symlinks=False).st_mode
                if stat.S_ISDIR(form):
                    walk(Path(entry.path), depth + 1)
                elif stat.S_ISREG(form):
                    files += 1
                else:
                    raise ValueError("actual upload entry is not an ordinary file or directory")
    walk(directory, 0)
    if not files:
        raise ValueError("no actual ordinary evidence file")
    return {"schema": "factory.native-evidence-portability/v1", "root": str(directory),
            "observed_entries": count, "ordinary_files": files, "portable": True,
            "native_body_admission": False, "Original_Run_credit": False,
            "limit": "Finite path/form checkpoint; no atomic namespace or process-quiescence claim"}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("operation", choices=["names", "list", "run", "log-path", "upload-paths"])
    p.add_argument("group", nargs="?", choices=GROUPS)
    p.add_argument("--input", type=Path)
    p.add_argument("--case")
    a = p.parse_args()
    if a.operation == "upload-paths":
        if a.group is not None or a.input is None:
            p.error("upload-paths requires only the actual --input evidence directory")
        print(json.dumps(validate_upload_tree(a.input)))
        return
    if a.group is None:
        p.error("the declared native group is required")
    sets = json.loads(CASE_SETS.read_text())
    if a.operation == "log-path":
        if a.input is None or a.case is None:
            p.error("actual evidence --input and declared --case are required")
        print(declare_log_path(a.input, a.group, a.case, sets))
        return
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
