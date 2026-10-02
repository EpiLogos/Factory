fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("workflow") {
        return epilogos_factory::workflow_authoring::cli::main(&args);
    }
    // The configuration plane contract (C0 §6) puts its structured failure
    // document on stdout while the process still exits non-zero. These command
    // heads retain their own configuration failure contract and entrypoint.
    if epilogos_factory::configuration::is_config_command(args.first().map(String::as_str)) {
        return epilogos_factory::configuration::config_main(&args);
    }
    // World inhabitation refusals are three-part documents; under --json they
    // go to stdout with a non-zero exit, like the configuration plane's.
    if epilogos_factory::inhabitation_cli::is_inhabitation_command(&args) {
        return epilogos_factory::inhabitation_cli::main(&args);
    }
    match epilogos_factory::attempt_cli::execute(&args, None) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            if args.iter().any(|arg| arg == "--json") {
                if let Some(result) = error.native_publication_failure() {
                    println!("{result}");
                }
            }
            eprintln!("factory: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
