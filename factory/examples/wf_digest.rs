// Temporary operator helper: print the canonical source digest of a workflow source document.
use epilogos_factory::workflow::{workflow_source_digest, WorkflowSource};

fn main() {
    let path = std::env::args().nth(1).expect("workflow source JSON path");
    let text = std::fs::read_to_string(&path).expect("read workflow source");
    let source: WorkflowSource = serde_json::from_str(&text).expect("parse workflow source");
    println!("{}", workflow_source_digest(&source).expect("digest"));
}
