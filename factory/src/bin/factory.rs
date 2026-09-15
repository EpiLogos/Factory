fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The configuration plane contract (C0 §6) puts its structured failure
    // document on stdout while the process still exits non-zero. The attempt
    // CLI flattens errors into strings for stderr, so these two command heads
    // are routed to their own entry before that chain.
    if epilogos_factory::configuration::is_config_command(args.first().map(String::as_str)) {
        return epilogos_factory::configuration::config_main(&args);
    }
    match epilogos_factory::attempt_cli::execute(&args, None) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("factory: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
