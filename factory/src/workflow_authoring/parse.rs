//! A bounded data parser, not a TypeScript interpreter/transpiler/sandbox.
use super::*;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(super) struct Data {
    pub value: Value,
    pub locations: BTreeMap<String, SourceLocation>,
    bytes: usize,
}
impl Data {
    fn new(value: Value, loc: SourceLocation) -> Self {
        Self {
            bytes: serde_json::to_vec(&value).expect("literal JSON").len(),
            value,
            locations: [(String::new(), loc)].into(),
        }
    }
    fn add_locations(&mut self, prefix: &str, other: &Self) -> Result<(), Diagnostic> {
        // Charge expansions before building a large parent. Checking only the
        // final export would allow a short repeated constant to allocate GBs.
        self.bytes = self
            .bytes
            .saturating_add(other.bytes)
            .saturating_add(prefix.len() * 2 + 4);
        if self.bytes > MAX_BUNDLE_BYTES || self.locations.len() + other.locations.len() > MAX_NODES
        {
            return Err(error("authoring.limit", "Expanded data exceeds the bounded workflow size; reduce repeated material or use native source references"));
        }
        for (p, l) in &other.locations {
            self.locations.insert(format!("{prefix}{p}"), l.clone());
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
enum Binding {
    Data(Arc<Data>),
    Builder,
    Type,
}
#[derive(Debug, Clone)]
struct Module {
    exports: BTreeMap<String, Binding>,
    default: Option<Arc<Data>>,
}
#[derive(Debug, Clone)]
enum Kind {
    Word(String),
    String(String),
    Number(String),
    Symbol(char),
    End,
}
#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
    at: usize,
}
struct Loader {
    root: PathBuf,
    retained: Option<BTreeMap<String, ModuleBasis>>,
    modules: BTreeMap<String, ModuleBasis>,
    cache: BTreeMap<String, Module>,
    active: BTreeSet<String>,
    total_bytes: usize,
    nodes: usize,
    expanded_bytes: usize,
    expanded_nodes: usize,
}
fn token_error(file: &str, text: &str, at: usize, message: impl Into<String>) -> Diagnostic {
    let mut d = error("authoring.syntax", message);
    d.location = Some(Box::new(location(file, text, at)));
    d
}
fn tokenize(file: &str, text: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut out = Vec::new();
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if text[i..].starts_with("//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if text[i..].starts_with("/*") {
            let n = text[i + 2..]
                .find("*/")
                .ok_or_else(|| token_error(file, text, i, "Unterminated block comment"))?;
            i += n + 4;
            continue;
        }
        let at = i;
        let kind = if b == b'\'' || b == b'"' {
            let quote = b;
            i += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == quote {
                    i += 1;
                    closed = true;
                    break;
                }
                if bytes[i] == b'\n' || bytes[i] == b'\r' {
                    return Err(token_error(
                        file,
                        text,
                        i,
                        "Use escaped newlines inside string literals",
                    ));
                }
                if bytes[i] == b'\\' {
                    i += 1;
                    if i == bytes.len() {
                        break;
                    }
                    let c = match bytes[i] {
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'b' => '\u{0008}',
                        b'f' => '\u{000c}',
                        b'\\' => '\\',
                        b'\'' => '\'',
                        b'"' => '"',
                        b'/' => '/',
                        b'u' => {
                            let end = i + 5;
                            if end > bytes.len() || !text.is_char_boundary(end) {
                                return Err(token_error(
                                    file,
                                    text,
                                    i,
                                    "Use four hexadecimal digits after \\u",
                                ));
                            }
                            let raw = &text[i + 1..end];
                            let code=u32::from_str_radix(raw,16).ok().and_then(char::from_u32).ok_or_else(|| token_error(file,text,i,"Invalid Unicode scalar escape; use literal Unicode for surrogate pairs"))?;
                            i += 4;
                            code
                        }
                        _ => {
                            return Err(token_error(
                                file,
                                text,
                                i,
                                "Unsupported escape; use a literal character or a JSON escape",
                            ))
                        }
                    };
                    value.push(c);
                    i += 1;
                } else {
                    let c = text[i..].chars().next().unwrap();
                    if c.is_control() {
                        return Err(token_error(file, text, i, "Control character in string"));
                    }
                    value.push(c);
                    i += c.len_utf8();
                }
            }
            if !closed {
                return Err(token_error(file, text, at, "Unterminated string"));
            }
            Kind::String(value)
        } else if b.is_ascii_alphabetic() || b == b'_' || b == b'$' {
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            Kind::Word(text[at..i].into())
        } else if b.is_ascii_digit()
            || b == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)
        {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || b".eE+-".contains(&bytes[i])) {
                i += 1;
            }
            Kind::Number(text[at..i].into())
        } else if b"{}[]():,;=".contains(&b) {
            i += 1;
            Kind::Symbol(b as char)
        } else {
            return Err(token_error(file,text,i,"Unsupported construct. Use literals, prior const data, named deterministic imports and Factory data builders; no code, spread, templates or property access"));
        };
        out.push(Token { kind, at });
        if out.len() > MAX_NODES * 4 {
            return Err(token_error(
                file,
                text,
                at,
                "Token limit exceeded; split or simplify the workflow",
            ));
        }
    }
    out.push(Token {
        kind: Kind::End,
        at: i,
    });
    Ok(out)
}
impl Loader {
    fn charge_expansion(&mut self, data: &Data) -> Result<(), Diagnostic> {
        self.expanded_bytes = self.expanded_bytes.saturating_add(data.bytes);
        self.expanded_nodes = self.expanded_nodes.saturating_add(data.locations.len());
        if self.expanded_bytes > 16 * MAX_BUNDLE_BYTES || self.expanded_nodes > 4 * MAX_NODES {
            return Err(error("authoring.expansion_limit", "Repeated data references exceed the bundle's expansion budget; use native source refs instead of duplicated payloads"));
        }
        Ok(())
    }
    fn load(&mut self, path: &str) -> Result<Module, Diagnostic> {
        if let Some(m) = self.cache.get(path) {
            return Ok(m.clone());
        }
        if !self.active.insert(path.into()) {
            return Err(error(
                "authoring.import_cycle",
                format!("Import cycle at {path}; use an acyclic data module graph"),
            ));
        }
        if self.modules.len() >= MAX_MODULES {
            return Err(error("authoring.limit", "Import module limit exceeded"));
        }
        if !module_path(path) {
            return Err(error(
                "authoring.import_path",
                "Imports must name relative .ts data modules inside the selected root",
            ));
        }
        let bytes = match &self.retained {
            Some(modules) => modules
                .get(path)
                .ok_or_else(|| {
                    error(
                        "authoring.module_unavailable",
                        format!("Retained source omits {path}"),
                    )
                })?
                .content
                .as_bytes()
                .to_vec(),
            None => read_module(&self.root, path)?,
        };
        self.total_bytes += bytes.len();
        if bytes.len() > MAX_MODULE_BYTES || self.total_bytes > MAX_BUNDLE_BYTES {
            return Err(error(
                "authoring.limit",
                "Source bundle exceeds the byte limit",
            ));
        }
        let text =
            String::from_utf8(bytes).map_err(|e| error("authoring.encoding", e.to_string()))?;
        self.modules.insert(
            path.into(),
            ModuleBasis {
                path: path.into(),
                bytes: text.len(),
                digest: blake3::hash(text.as_bytes()).to_hex().to_string(),
                content: text.clone(),
            },
        );
        let tokens = tokenize(path, &text)?;
        let module = Parser {
            file: path,
            text: &text,
            tokens,
            index: 0,
            bindings: BTreeMap::new(),
            loader: self,
        }
        .module()?;
        self.active.remove(path);
        self.cache.insert(path.into(), module.clone());
        Ok(module)
    }
}
struct Parser<'a> {
    file: &'a str,
    text: &'a str,
    tokens: Vec<Token>,
    index: usize,
    bindings: BTreeMap<String, Binding>,
    loader: &'a mut Loader,
}
impl Parser<'_> {
    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }
    fn err(&self, message: impl Into<String>) -> Diagnostic {
        token_error(self.file, self.text, self.current().at, message)
    }
    fn word_is(&self, s: &str) -> bool {
        matches!(&self.current().kind,Kind::Word(w) if w==s)
    }
    fn word(&mut self) -> Result<String, Diagnostic> {
        if let Kind::Word(w) = self.current().kind.clone() {
            self.index += 1;
            Ok(w)
        } else {
            Err(self.err("Expected an identifier"))
        }
    }
    fn eat_word(&mut self, s: &str) -> bool {
        if self.word_is(s) {
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn eat(&mut self, c: char) -> bool {
        if matches!(self.current().kind,Kind::Symbol(x) if x==c) {
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, c: char) -> Result<(), Diagnostic> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(self.err(format!(
                "Expected '{c}'; unsupported executable syntax must be replaced with data"
            )))
        }
    }
    fn insert(&mut self, name: String, b: Binding) -> Result<(), Diagnostic> {
        if self.bindings.insert(name.clone(), b).is_some() {
            return Err(self.err(format!("Duplicate binding {name}")));
        }
        Ok(())
    }
    fn module(&mut self) -> Result<Module, Diagnostic> {
        let mut exports = BTreeMap::new();
        let mut default = None;
        while !matches!(self.current().kind, Kind::End) {
            if self.eat(';') {
                continue;
            }
            if self.eat_word("import") {
                self.import()?;
                continue;
            }
            let export = self.eat_word("export");
            if export && self.eat_word("default") {
                if default.is_some() {
                    return Err(self.err("A module must have only one default workflow export"));
                }
                default = Some(Arc::new(self.expression(0)?));
                self.eat(';');
                continue;
            }
            if !self.eat_word("const") {
                return Err(self.err(
                    "Only import, const data, export const data and export default are supported",
                ));
            }
            let name = self.word()?;
            if self.eat(':') {
                self.type_name()?;
            }
            self.expect('=')?;
            let value = self.expression(0)?;
            let binding = Binding::Data(Arc::new(value));
            self.insert(name.clone(), binding.clone())?;
            if export {
                exports.insert(name, binding);
            }
            self.eat(';');
        }
        Ok(Module { exports, default })
    }
    fn type_name(&mut self) -> Result<(), Diagnostic> {
        let name = self.word()?;
        if !matches!(self.bindings.get(&name), Some(Binding::Type)) {
            return Err(self.err(format!(
                "Unknown type {name}; import a Factory public type with import type"
            )));
        }
        Ok(())
    }
    fn import(&mut self) -> Result<(), Diagnostic> {
        let type_only = self.eat_word("type");
        self.expect('{')?;
        let mut names = Vec::new();
        if !self.eat('}') {
            loop {
                let inline_type = type_only || self.eat_word("type");
                let original = self.word()?;
                let local = if self.eat_word("as") {
                    self.word()?
                } else {
                    original.clone()
                };
                names.push((original, local, inline_type));
                if self.eat('}') {
                    break;
                }
                self.expect(',')?;
                if self.eat('}') {
                    break;
                }
            }
        }
        if names.is_empty() {
            return Err(self.err("Empty or side-effect imports are not supported"));
        }
        if !self.eat_word("from") {
            return Err(self.err(
                "Use a literal named import from the Factory SDK or a relative .ts data module",
            ));
        }
        let Kind::String(specifier) = self.current().kind.clone() else {
            return Err(self.err("Dynamic imports are forbidden"));
        };
        self.index += 1;
        self.eat(';');
        if specifier == SDK {
            for (original, local, is_type) in names {
                let b = if is_type {
                    let declaration = format!("export type {original} =");
                    if !include_str!("../../workflow-sdk/index.d.ts").contains(&declaration) {
                        return Err(self.err(format!(
                            "The installed Factory SDK does not export type {original}"
                        )));
                    }
                    Binding::Type
                } else if ["defineWorkflow", "unit", "barrier", "nesting"]
                    .contains(&original.as_str())
                {
                    Binding::Builder
                } else {
                    return Err(self.err(format!("Unsupported Factory builder {original}")));
                };
                self.insert(local, b)?;
            }
        } else {
            if !specifier.starts_with("./") && !specifier.starts_with("../") {
                return Err(self.err("Only the Factory SDK and literal relative .ts data imports are permitted; no packages, URLs or ambient modules"));
            }
            if type_only || names.iter().any(|(_, _, t)| *t) {
                return Err(self.err("Domain type imports need their native adapter; only Factory types are accepted by this frontend"));
            }
            let parent = Path::new(self.file).parent().unwrap_or(Path::new(""));
            let mut parts = Vec::new();
            for part in parent.join(&specifier).components() {
                match part {
                    Component::Normal(p) => parts.push(p.to_string_lossy().to_string()),
                    Component::CurDir => {}
                    Component::ParentDir => {
                        if parts.pop().is_none() {
                            return Err(self.err("Import escapes the selected source root"));
                        }
                    }
                    _ => return Err(self.err("Invalid import path")),
                }
            }
            let path = parts.join("/");
            let module = self.loader.load(&path)?;
            for (original, local, _) in names {
                let value =
                    module.exports.get(&original).cloned().ok_or_else(|| {
                        self.err(format!("{path} does not export data {original}"))
                    })?;
                self.insert(local, value)?;
            }
        }
        Ok(())
    }
    fn expression(&mut self, depth: usize) -> Result<Data, Diagnostic> {
        self.loader.nodes += 1;
        if depth > MAX_DEPTH || self.loader.nodes > MAX_NODES {
            return Err(self.err("Data nesting or node limit exceeded"));
        }
        let token = self.current().clone();
        let loc = location(self.file, self.text, token.at);
        let mut result = match token.kind {
            Kind::Symbol('{') => {
                self.index += 1;
                let mut data = Data::new(Value::Object(Map::new()), loc);
                if !self.eat('}') {
                    loop {
                        let key=match self.current().kind.clone() {Kind::String(s)|Kind::Word(s)=>{self.index+=1;s},_=>return Err(self.err("Expected a literal property name; computed keys, methods and spreads are forbidden"))};
                        if ["__proto__", "prototype", "constructor"].contains(&key.as_str()) {
                            return Err(self.err("Prototype-related keys are forbidden"));
                        }
                        self.expect(':')?;
                        let value = self.expression(depth + 1)?;
                        let prefix = format!("/{}", key.replace('~', "~0").replace('/', "~1"));
                        data.add_locations(&prefix, &value)?;
                        if data
                            .value
                            .as_object_mut()
                            .unwrap()
                            .insert(key.clone(), value.value)
                            .is_some()
                        {
                            return Err(self.err(format!("Duplicate property {key}")));
                        }
                        if self.eat('}') {
                            break;
                        }
                        self.expect(',')?;
                        if self.eat('}') {
                            break;
                        }
                    }
                }
                data
            }
            Kind::Symbol('[') => {
                self.index += 1;
                let mut data = Data::new(Value::Array(Vec::new()), loc);
                let mut index = 0;
                if !self.eat(']') {
                    loop {
                        let value = self.expression(depth + 1)?;
                        data.add_locations(&format!("/{index}"), &value)?;
                        data.value.as_array_mut().unwrap().push(value.value);
                        index += 1;
                        if self.eat(']') {
                            break;
                        }
                        self.expect(',')?;
                        if self.eat(']') {
                            break;
                        }
                    }
                }
                data
            }
            Kind::String(s) => {
                self.index += 1;
                Data::new(s.into(), loc)
            }
            Kind::Number(s) => {
                self.index += 1;
                let n: serde_json::Number = serde_json::from_str(&s).map_err(|_| {
                    self.err(
                        "Use a finite JSON number; no NaN, Infinity, hexadecimal or expressions",
                    )
                })?;
                Data::new(n.into(), loc)
            }
            Kind::Word(s) if ["true", "false", "null"].contains(&s.as_str()) => {
                self.index += 1;
                Data::new(
                    match s.as_str() {
                        "true" => true.into(),
                        "false" => false.into(),
                        _ => Value::Null,
                    },
                    loc,
                )
            }
            Kind::Word(s) => {
                self.index += 1;
                match self.bindings.get(&s).cloned(){
                Some(Binding::Data(d))=>{self.loader.charge_expansion(&d)?;(*d).clone()},
                Some(Binding::Builder)=>{self.expect('(')?;let d=self.expression(depth+1)?;self.eat(',');self.expect(')')?;if !d.value.is_object(){return Err(self.err("Factory builders require one data object"));}d},
                _=>return Err(self.err(format!("Unresolved data {s}; no globals, calls or forward/executable references are available"))),
            }
            }
            _ => return Err(self.err("Expected literal data or a Factory builder")),
        };
        if self.eat_word("as") && !self.eat_word("const") {
            return Err(
                self.err("Only 'as const' is supported; casts cannot override native validation")
            );
        }
        if self.eat_word("satisfies") {
            self.type_name()?;
        }
        // Repeated constant expansion is bounded as well as syntax size.
        if result.locations.len() > MAX_NODES || result.bytes > MAX_BUNDLE_BYTES {
            return Err(self.err("Expanded data exceeds the workflow size limit"));
        }
        result
            .locations
            .entry(String::new())
            .or_insert_with(|| location(self.file, self.text, token.at));
        Ok(result)
    }
}
pub(super) fn load(
    root: &Path,
    entry: &Path,
) -> Result<(Data, BTreeMap<String, ModuleBasis>, String), Diagnostic> {
    let root = root
        .canonicalize()
        .map_err(|e| error("authoring.root", e.to_string()))?;
    if !root.is_dir() {
        return Err(error("authoring.root", "Source root must be a directory"));
    }
    let name = if entry.is_absolute() {
        entry
            .strip_prefix(&root)
            .map_err(|_| error("authoring.entry", "Entry must be inside the selected root"))?
            .to_path_buf()
    } else {
        entry.to_path_buf()
    };
    let name = name
        .to_str()
        .ok_or_else(|| error("authoring.entry", "Entry must be UTF-8"))?
        .replace(std::path::MAIN_SEPARATOR, "/");
    let mut loader = Loader {
        root,
        retained: None,
        modules: BTreeMap::new(),
        cache: BTreeMap::new(),
        active: BTreeSet::new(),
        total_bytes: 0,
        nodes: 0,
        expanded_bytes: 0,
        expanded_nodes: 0,
    };
    let module = loader.load(&name)?;
    let default = module.default.ok_or_else(|| {
        error(
            "authoring.export",
            "Entry must export default defineWorkflow({...})",
        )
    })?;
    // Detect edits during loading; admission consumes retained bytes, not a later file.
    for (path, old) in &loader.modules {
        let bytes = read_module(&loader.root, path)?;
        if blake3::hash(&bytes).to_hex().as_str() != old.digest {
            return Err(error(
                "authoring.source_changed",
                format!("{path} changed while loading; retry compilation explicitly"),
            ));
        }
    }
    Ok(((*default).clone(), loader.modules, name))
}

// Same bounded source reader for initial loading and TOCTOU recheck. Imported
// data never decides a path except through the explicit rooted import grammar.
fn read_module(root: &Path, path: &str) -> Result<Vec<u8>, Diagnostic> {
    if !module_path(path) {
        return Err(error("authoring.import_path", "Invalid rooted module path"));
    }
    let full = root.join(path);
    let check_path = || -> Result<(), Diagnostic> {
        let mut check = root.to_path_buf();
        for part in Path::new(path).components() {
            check.push(part);
            if fs::symlink_metadata(&check)
                .map_err(|e| error("authoring.module_unavailable", format!("{path}: {e}")))?
                .file_type()
                .is_symlink()
            {
                return Err(error(
                    "authoring.redirected_source",
                    format!("{path}: symlinked sources are not permitted"),
                ));
            }
        }
        if full
            .canonicalize()
            .map_err(|e| error("authoring.module_unavailable", e.to_string()))?
            != full
        {
            return Err(error(
                "authoring.redirected_source",
                "Source path changed while loading",
            ));
        }
        Ok(())
    };
    check_path()?;
    #[cfg(unix)]
    let file = {
        use std::os::fd::{AsRawFd, FromRawFd};
        // Walk beneath the selected directory handle without following any
        // component symlink. NONBLOCK avoids opening a substituted FIFO/device.
        let mut parent =
            fs::File::open(root).map_err(|e| error("authoring.root", e.to_string()))?;
        let components = Path::new(path).components().collect::<Vec<_>>();
        for (index, component) in components.iter().enumerate() {
            use std::os::unix::ffi::OsStrExt;
            let name = std::ffi::CString::new(component.as_os_str().as_bytes())
                .map_err(|_| error("authoring.import_path", "NUL in source path"))?;
            let mut flags = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK;
            if index + 1 != components.len() {
                flags |= libc::O_DIRECTORY;
            }
            // SAFETY: parent owns a valid directory fd; name is NUL terminated;
            // a successful openat transfers a new fd to the owned File below.
            let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err(error(
                    "authoring.module_unavailable",
                    format!("{path}: {}", std::io::Error::last_os_error()),
                ));
            }
            // SAFETY: openat returned a fresh, uniquely owned descriptor.
            parent = unsafe { fs::File::from_raw_fd(fd) };
        }
        parent
    };
    #[cfg(not(unix))]
    let file =
        fs::File::open(&full).map_err(|e| error("authoring.module_unavailable", e.to_string()))?;
    let meta = file
        .metadata()
        .map_err(|e| error("authoring.module_unavailable", e.to_string()))?;
    if !meta.is_file() || meta.len() > MAX_MODULE_BYTES as u64 {
        return Err(error(
            "authoring.limit",
            "Source must be a regular file of at most 512 KiB",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_MODULE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error("authoring.module_unavailable", e.to_string()))?;
    if bytes.len() > MAX_MODULE_BYTES {
        return Err(error(
            "authoring.limit",
            "Source exceeded 512 KiB while loading",
        ));
    }
    check_path()?;
    let current =
        fs::metadata(&full).map_err(|e| error("authoring.source_changed", e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.dev() != current.dev() || meta.ino() != current.ino() {
            return Err(error(
                "authoring.source_changed",
                "Source inode changed while loading",
            ));
        }
    }
    if meta.len() != current.len() || meta.modified().ok() != current.modified().ok() {
        return Err(error(
            "authoring.source_changed",
            "Source changed while loading",
        ));
    }
    Ok(bytes)
}

/// Pure replay over immutable native source bytes. A caller cannot attach valid
/// hashes for unrelated source text to a different semantic workflow document.
pub(super) fn replay(basis: &AuthoredBasis) -> Result<Data, Diagnostic> {
    let mut loader = Loader {
        root: PathBuf::new(),
        retained: Some(basis.modules.clone()),
        modules: BTreeMap::new(),
        cache: BTreeMap::new(),
        active: BTreeSet::new(),
        total_bytes: 0,
        nodes: 0,
        expanded_bytes: 0,
        expanded_nodes: 0,
    };
    let module = loader.load(&basis.entry)?;
    if loader.modules != basis.modules {
        return Err(error(
            "authoring.basis_invalid",
            "Retained module graph differs from actual literal imports",
        ));
    }
    let data = module.default.ok_or_else(|| {
        error(
            "authoring.export",
            "Retained entry lacks its default workflow",
        )
    })?;
    if data.locations != basis.locations {
        return Err(error(
            "authoring.location_invalid",
            "Retained locations do not identify the actual source fields",
        ));
    }
    Ok((*data).clone())
}
