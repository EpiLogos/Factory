//! Factory's tests write every file they exec through one helper,
//! `support/executable.rs`. A test that grants an executable mode in-process
//! instead holds a write handle a parallel test's fork can inherit, and the
//! exec then fails with ETXTBSY (os error 26); see that file. This guard
//! scans `factory/tests` and refuses any executable mode granted outside the
//! helper: a Rust permission call with an octal literal carrying an execute
//! bit, or a `chmod` that grants one. A directory that must keep its search
//! bit while losing write is made read-only (`Permissions::set_readonly`).
//! The scan cannot see an executable rewritten in place (`fs::write` over it)
//! or copied (`fs::copy` carries its mode); those go through the helper too.

use std::fs;
use std::path::{Path, PathBuf};

const HELPER: &str = "support/executable.rs";

/// The octal literal at the start of `text` (`0o` prefix optional), if any.
fn octal(text: &str) -> Option<u32> {
    let text = text.trim_start();
    let text = text.strip_prefix("0o").unwrap_or(text);
    let digits: String = text
        .chars()
        .take_while(|c| c.is_digit(8) || *c == '_')
        .filter(|c| *c != '_')
        .collect();
    if digits.is_empty() {
        return None;
    }
    u32::from_str_radix(&digits, 8).ok()
}

/// Whether `line` grants an execute bit: through a Rust permission call with
/// an octal literal, or through a `chmod` in a shell string.
fn grants_execute(line: &str) -> bool {
    let rust_calls = ["from_mode(", "set_mode(", ".mode("];
    for call in rust_calls {
        for (at, _) in line.match_indices(call) {
            if octal(&line[at + call.len()..]).is_some_and(|mode| mode & 0o111 != 0) {
                return true;
            }
        }
    }
    for (at, _) in line.match_indices("chmod") {
        let mode: String = line[at + "chmod".len()..]
            .trim_start()
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '\\' | ';' | ')'))
            .collect();
        let symbolic = mode.contains(['+', '=']) && mode.contains('x');
        if symbolic || octal(&mode).is_some_and(|mode| mode & 0o111 != 0) {
            return true;
        }
    }
    false
}

fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn guard_reads_modes_and_chmod_as_written() {
    for granting in [
        "fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();",
        "permissions.set_mode(0o755);",
        "OpenOptions::new().mode(0o0_711).create(true)",
        "\"cat > \\\"$t\\\" && chmod 755 \\\"$t\\\"\"",
        "chmod u+x \"$1\"",
        "chmod +x script",
    ] {
        assert!(grants_execute(granting), "{granting}");
    }
    for inert in [
        "fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();",
        ".mode(0o600)",
        "world.mode(\"lost\");",
        "/bin/chmod 000 \"$1\" || exit 66",
        "chmod u-w \"$1\"",
        "readonly.set_readonly(true);",
    ] {
        assert!(!grants_execute(inert), "{inert}");
    }
}

#[test]
fn test_executables_are_written_only_through_the_shared_helper() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let helper = root.join(HELPER);
    let this = root.join(file!().rsplit('/').next().unwrap());
    let mut files = Vec::new();
    sources(&root, &mut files);
    files.sort();
    assert!(
        files.contains(&helper),
        "the shared helper {HELPER} is missing"
    );
    let mut offending = Vec::new();
    for file in files
        .iter()
        .filter(|file| **file != helper && **file != this)
    {
        let text = fs::read_to_string(file).unwrap();
        for (number, line) in text.lines().enumerate() {
            if grants_execute(line) {
                offending.push(format!(
                    "{}:{}: {}",
                    file.strip_prefix(&root).unwrap().display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        offending.is_empty(),
        "these lines grant an executable mode outside {HELPER}; write the file with \
         write_executable instead (a write handle held in-process meets a parallel \
         fork and the exec fails with ETXTBSY):\n{}",
        offending.join("\n")
    );
}
