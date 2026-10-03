"""Finite native qualification inputs; observations never grant owner authority."""
import argparse
import hashlib
import json
import itertools
import os
from pathlib import Path
import stat

EVIDENCE = Path("native-evidence")
REGULAR_FLAGS = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC
DIRECTORY_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
MAX_IMAGE = 128 * 1024 * 1024
MAX_LOG = 16 * 1024 * 1024
MAX_SNAPSHOT = 16 * 1024 * 1024
MAX_MANIFEST = 64 * 1024


def identity(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_nlink,
            info.st_uid, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def held_bytes(path, limit, single_link=True, parent_fd=None):
    path = Path(path)
    coordinate = path if parent_fd is None else path.name
    named_now = lambda: os.stat(coordinate, dir_fd=parent_fd, follow_symlinks=False)
    with os.fdopen(os.open(coordinate, REGULAR_FLAGS, dir_fd=parent_fd), "rb") as stream:
        before = os.fstat(stream.fileno())
        named = named_now()
        if (not stat.S_ISREG(before.st_mode) or before.st_nlink < 1
                or (single_link and before.st_nlink != 1)
                or before.st_uid != os.geteuid() or before.st_size > limit
                or identity(before) != identity(named)):
            raise ValueError("native input form, affiliation or finite capacity refused")
        data = stream.read(limit + 1)
        if len(data) != before.st_size or len(data) > limit:
            raise ValueError("native input changed or exceeded finite observation")
        stream.seek(0)
        if (stream.read(limit + 1) != data
                or identity(before) != identity(os.fstat(stream.fileno()))
                or identity(before) != identity(named_now())):
            raise ValueError("native input changed during held observation")
    return data, before


def private_copy(path, data, mode=0o600):
    flags = os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC
    with os.fdopen(os.open(path, flags, mode), "w+b") as stream:
        stream.write(data)
        stream.flush()
        os.fchmod(stream.fileno(), mode)
        os.fsync(stream.fileno())
        info = os.fstat(stream.fileno())
        stream.seek(0)
        if (stream.read(len(data) + 1) != data or info.st_nlink != 1
                or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != mode
                or identity(info) != identity(os.fstat(stream.fileno()))
                or identity(info) != identity(Path(path).stat(follow_symlinks=False))):
            raise ValueError("retained native copy differs from held original observation")


def write_json(path, value):
    private_copy(path, (json.dumps(value, indent=2) + "\n").encode())


def compiler(args):
    log_data, _ = held_bytes(args.log, MAX_LOG)
    messages = [json.loads(line) for line in log_data.decode("utf-8").splitlines() if line]
    finishes = [m for m in messages if m.get("reason") == "build-finished"]
    artifacts = [m for m in messages if m.get("reason") == "compiler-artifact"
                 and m.get("target", {}).get("name") == args.target
                 and m.get("target", {}).get("kind") == [args.kind]
                 and m.get("executable")]
    if len(finishes) != 1 or finishes[0].get("success") is not True or len(artifacts) != 1:
        raise ValueError("no unique complete actual CompilerArtifact")
    artifact = artifacts[0]
    if artifact.get("profile", {}).get("test") is not args.test_profile:
        raise ValueError("actual compiler profile differs from the selected native role")
    root = args.root.resolve(strict=True)
    manifest = root / args.manifest
    entry = root / args.entry
    if (Path(artifact.get("manifest_path", "")).resolve(strict=True) != manifest
            or Path(artifact.get("target", {}).get("src_path", "")).resolve(strict=True) != entry):
        raise ValueError("actual compiler manifest or entry Source differs from selected checkout")
    executable = Path(artifact["executable"])
    if not executable.is_absolute() or executable.resolve(strict=True) != executable:
        raise ValueError("compiler image is not its actual canonical locator")
    executable.relative_to(root)
    image, info = held_bytes(executable, MAX_IMAGE, single_link=False)
    if (not image or not info.st_mode & 0o111
            or not image.startswith((b"\x7fELF", b"\xcf\xfa\xed\xfe",
                                     b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe"))):
        raise ValueError("actual compiler image is not bounded native ELF/Mach-O")
    retained = EVIDENCE / ("compiler-image-" + args.role)
    private_copy(retained, image, 0o700)
    actual_after, after = held_bytes(executable, MAX_IMAGE, single_link=False)
    if actual_after != image or identity(after) != identity(info):
        raise ValueError("original compiler image changed across retained copy")
    manifest_data, _ = held_bytes(manifest, MAX_LOG, single_link=False)
    entry_data, _ = held_bytes(entry, MAX_LOG, single_link=False)
    source_files = {name: hashlib.sha256(held_bytes(EVIDENCE / name, MAX_IMAGE)[0]).hexdigest()
                    for name in ("source-sha.txt", "source-tree.txt", "source-manifest.txt",
                                 "source.tar.gz", "Cargo.toml", "Cargo.lock")
                    if (EVIDENCE / name).exists()}
    if len(source_files) != 6:
        raise ValueError("Factory Source/lock/archive evidence incomplete")
    write_json(EVIDENCE / ("compiler-image-" + args.role + ".json"), {
        "schema": "factory.native-compiler-image/v1", "role": args.role,
        "compiler_artifact": artifact, "compiler_log_sha256": hashlib.sha256(log_data).hexdigest(),
        "actual_executable": str(executable), "identity": identity(info),
        "bytes": len(image), "sha256": hashlib.sha256(image).hexdigest(),
        "retained_copy": str(retained), "manifest_sha256": hashlib.sha256(manifest_data).hexdigest(),
        "entry_source_sha256": hashlib.sha256(entry_data).hexdigest(),
        "factory_source_files_sha256": source_files, "original_Run_credit": False,
        "standing": "Actual compiler/image observation; not execution or installed baseline",
        "limits": "Held/named pre/post observations; no atomic fd-exec, universal ACL or concurrent-writer exclusion",
    })


def admitted_root():
    data, _ = held_bytes(EVIDENCE / "owner-dispatch-admission.json", MAX_MANIFEST)
    admission = json.loads(data)
    root = Path(admission["root"])
    if not root.is_absolute() or root.resolve(strict=True) != root:
        raise ValueError("original admitted fixture root no longer canonical")
    fd = os.open(root, DIRECTORY_FLAGS)
    try:
        validate_root(root, fd, admission)
    except BaseException:
        os.close(fd)
        raise
    return root, fd, admission


def validate_root(root, fd, admission):
    current = os.fstat(fd)
    named = root.stat(follow_symlinks=False)
    if (not stat.S_ISDIR(current.st_mode) or root.resolve(strict=True) != root
            or (current.st_dev, current.st_ino, current.st_uid, stat.S_IMODE(current.st_mode))
            != (admission["root_device"], admission["root_inode"], admission["root_uid"], 0o700)
            or (named.st_dev, named.st_ino) != (current.st_dev, current.st_ino)):
        raise ValueError("original fixture root affiliation changed")


def entry_names(fd, limit):
    with os.scandir(fd) as entries:
        names = [entry.name for entry in itertools.islice(entries, limit + 1)]
    if len(names) > limit:
        raise ValueError("finite directory entry observation exceeded")
    return sorted(names)


def snapshot(_args):
    root, fd, admission = admitted_root()
    candidates = []
    observations = []
    producer = "actual_late_return_before_termination_retains_current_stop_across_restart"
    producer_log = EVIDENCE / ("receiving-" + producer + ".log")
    # Use the SAME existing census, not a passing metadata assertion.
    from native_evidence_census import validate_run
    producer_bytes, _ = held_bytes(producer_log, MAX_LOG)
    validate_run(producer_bytes.decode("utf-8"), producer)
    try:
        names = entry_names(fd, 1024)
        for name in sorted(names):
            if not name.startswith("native-cancellation-"):
                continue
            directory = root / name
            info = directory.stat(follow_symlinks=False)
            if not stat.S_ISDIR(info.st_mode) or directory.resolve(strict=True) != directory:
                raise ValueError("actual cancellation fixture is not an ordinary in-root directory")
            child_fd = os.open(name, DIRECTORY_FLAGS, dir_fd=fd)
            try:
                child = os.fstat(child_fd)
                if identity(child) != identity(info):
                    raise ValueError("actual fixture name changed during selection")
                members = entry_names(child_fd, 1024)
                for member in sorted(members):
                    if not member.endswith(".manifest.json"):
                        continue
                    manifest_path = directory / member
                    data, manifest_info = held_bytes(manifest_path, MAX_MANIFEST, parent_fd=child_fd)
                    manifest = json.loads(data)
                    if manifest.get("schema") != "factory.native-cancellation-engine-basis/v1":
                        continue
                    if type(manifest.get("moveSubject")) is not bool:
                        raise ValueError("native basis subject-selection fact malformed")
                    if manifest["moveSubject"]:
                        continue
                    path = Path(manifest["snapshotPath"])
                    if (path.parent != directory or path.resolve(strict=True) != path
                            or path.with_suffix(".manifest.json") != manifest_path
                            or Path(manifest["sourcePath"]).parent != directory
                            or manifest.get("status") != "late_result"
                            or manifest.get("originalRunCredit") is not False
                            or type(manifest.get("bytes")) is not int
                            or not 0 < manifest["bytes"] <= MAX_SNAPSHOT
                            or not isinstance(manifest.get("sha256"), str)):
                        raise ValueError("actual original native basis affiliation or scalar facts refused")
                    body, source_info = held_bytes(path, MAX_SNAPSHOT, parent_fd=child_fd)
                    digest = hashlib.sha256(body).hexdigest()
                    if digest != manifest["sha256"] or len(body) != manifest["bytes"]:
                        raise ValueError("actual retained snapshot does not match native producer manifest")
                    provider = json.loads(body)
                    if not isinstance(provider.get("state"), dict):
                        raise ValueError("actual native provider wrapper has no state")
                    facts = {"snapshot": str(path), "snapshot_sha256": digest,
                             "snapshot_identity": identity(source_info), "manifest": str(manifest_path),
                             "manifest_sha256": hashlib.sha256(data).hexdigest(),
                             "manifest_identity": identity(manifest_info), "producer_manifest": manifest}
                    candidates.append(facts)
                if ((child.st_dev, child.st_ino) !=
                        (directory.stat(follow_symlinks=False).st_dev,
                         directory.stat(follow_symlinks=False).st_ino)):
                    raise ValueError("actual cancellation fixture changed before selection acknowledgement")
                observations.append({"directory": str(directory), "device": child.st_dev,
                                     "inode": child.st_ino, "observed_members": sorted(members)})
            finally:
                os.close(child_fd)
        if len(candidates) != 1:
            raise ValueError("no unique actual non-moved producer snapshot; synthetic fallback forbidden")
        validate_root(root, fd, admission)
        selected = candidates[0]
        # Reobserve actual selected bytes before exporting their exact locator/hash.
        body, info = held_bytes(selected["snapshot"], MAX_SNAPSHOT)
        if (hashlib.sha256(body).hexdigest() != selected["snapshot_sha256"]
                or identity(info) != tuple(selected["snapshot_identity"])):
            raise ValueError("selected actual native snapshot changed before consumer handover")
        write_json(EVIDENCE / "cancellation-snapshot-selection.json", {
            "schema": "factory.actual-native-cancellation-selection/v1",
            "selected": selected, "actual_fixture_observations": observations,
            "producer_case": "actual_late_return_before_termination_retains_current_stop_across_restart",
            "completed_producer_log": str(producer_log),
            "completed_producer_log_sha256": hashlib.sha256(producer_bytes).hexdigest(),
            "original_Run_credit": False, "authority": "Controlled native input observation only",
        })
        for key, value in (("FACTORY_NATIVE_CANCELLATION_SNAPSHOT", selected["snapshot"]),
                           ("FACTORY_NATIVE_CANCELLATION_SNAPSHOT_SHA256", selected["snapshot_sha256"])):
            if "\n" in value or "\r" in value:
                raise ValueError("ambiguous actual native environment value")
            with open(os.environ["GITHUB_ENV"], "a") as env:
                env.write(f"{key}={value}\n")
    finally:
        os.close(fd)


def fixtures(_args):
    # Original fixtures remain intact even when a reader/child retirement is unknown.
    destination = EVIDENCE / "fixture-snapshot"
    destination.mkdir(mode=0o700)
    result = {"schema": "factory.native-fixture-snapshot/v1", "entries": [],
              "process_quiescence_claim": False, "recursive_cleanup": False,
              "original_Run_credit": False, "complete": False, "retained_bytes": 0}
    try:
        root, fd, admission = admitted_root()
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as cause:
        result["admission_failure"] = {"type": type(cause).__name__, "errno": getattr(cause, "errno", None)}
        write_json(destination / "inventory.json", result)
        raise
    count = 0

    def walk(parent_fd, relative, depth):
        nonlocal count
        if depth > 64:
            raise ValueError("finite fixture depth exceeded")
        for name in entry_names(parent_fd, 4096):
            count += 1
            if count > 4096:
                raise ValueError("finite fixture entry census exceeded")
            validate_root(root, fd, admission)
            info = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
            member = relative / name
            record = {"relative": str(member), "identity": identity(info),
                      "form": stat.S_IFMT(info.st_mode), "body_observed": False}
            result["entries"].append(record)
            if stat.S_ISDIR(info.st_mode):
                child_fd = os.open(name, DIRECTORY_FLAGS, dir_fd=parent_fd)
                try:
                    held = os.fstat(child_fd)
                    if (held.st_dev, held.st_ino) != (info.st_dev, info.st_ino):
                        raise ValueError("fixture directory substituted before snapshot")
                    walk(child_fd, member, depth + 1)
                    named = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
                    if (named.st_dev, named.st_ino) != (held.st_dev, held.st_ino):
                        raise ValueError("fixture directory affiliation changed")
                finally:
                    os.close(child_fd)
            elif stat.S_ISREG(info.st_mode):
                if info.st_nlink != 1 or info.st_size > MAX_SNAPSHOT or info.st_uid != os.geteuid():
                    record["omission"] = "ordinary file outside finite single-link owner observation"
                    continue
                if result["retained_bytes"] + info.st_size > MAX_IMAGE:
                    raise ValueError("finite fixture snapshot capacity exceeded")
                body_fd = os.open(name, REGULAR_FLAGS, dir_fd=parent_fd)
                with os.fdopen(body_fd, "rb") as stream:
                    held = os.fstat(stream.fileno())
                    if identity(held) != identity(info):
                        raise ValueError("fixture ordinary file substituted")
                    body = stream.read(MAX_SNAPSHOT + 1)
                    stream.seek(0)
                    if (len(body) != info.st_size or len(body) > MAX_SNAPSHOT
                            or stream.read(MAX_SNAPSHOT + 1) != body
                            or identity(held) != identity(os.fstat(stream.fileno()))
                            or identity(held) != identity(os.stat(name, dir_fd=parent_fd, follow_symlinks=False))):
                        raise ValueError("fixture ordinary body changed during observation")
                validate_root(root, fd, admission)
                copied = "entry-" + str(count)
                private_copy(destination / copied, body)
                result["retained_bytes"] += len(body)
                record.update({"body_observed": True, "copy_relative": copied,
                               "sha256": hashlib.sha256(body).hexdigest(), "bytes": len(body)})
            else:
                record["omission"] = "symlink/FIFO/device/socket/nonregular metadata only"
    try:
        walk(fd, Path("."), 0)
        validate_root(root, fd, admission)
        result["complete"] = True
    except (OSError, ValueError) as cause:
        result["observation_failure"] = {"type": type(cause).__name__, "errno": getattr(cause, "errno", None),
                                         "message": str(cause)}
        raise
    finally:
        os.close(fd)
        write_json(destination / "inventory.json", result)


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    image = commands.add_parser("compiler")
    for name in ("role", "manifest", "entry", "target", "kind"):
        image.add_argument("--" + name, required=True)
    image.add_argument("--log", type=Path, required=True)
    image.add_argument("--root", type=Path, required=True)
    image.add_argument("--test-profile", action="store_true")
    commands.add_parser("snapshot")
    commands.add_parser("fixtures")
    args = parser.parse_args()
    {"compiler": compiler, "snapshot": snapshot, "fixtures": fixtures}[args.command](args)


if __name__ == "__main__":
    main()
