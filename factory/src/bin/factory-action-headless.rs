use epilogos_factory::action_projection::{
    execute_projected_factory_action, FactoryActionProjectionRequest,
};
use epilogos_factory::build::FactoryBuildSelection;
use epilogos_factory::build_provider::FactoryBuildFileProvider;
use epilogos_factory::core::run::{ProjectRef, RunRef};
use std::io::{self, Read};
use std::str::FromStr;

fn main() {
    if let Err(error) = run() {
        if let Some(result) = epilogos_factory::native_publication_failure(error.as_ref()) {
            println!("{result}");
        }
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let state_path = args.next().ok_or(
        "usage: factory-action-headless <state-path> <project-ref> <run-ref> < request.json",
    )?;
    let project_ref = ProjectRef::from_str(&args.next().ok_or("missing project-ref")?)?;
    let run_ref = RunRef::from_str(&args.next().ok_or("missing run-ref")?)?;
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }

    let selection = FactoryBuildSelection {
        project_ref,
        run_ref,
    };
    let mut provider = FactoryBuildFileProvider::open(state_path, selection)?;

    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: FactoryActionProjectionRequest = serde_json::from_str(&input)?;
    let receipt = execute_projected_factory_action(&mut provider, &request)?;
    println!("{}", serde_json::to_string(&receipt)?);
    Ok(())
}
