fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("owner") {
        return epilogos_factory::native_owner::native_owner_cli_main(&args[1..]);
    }
    let attempt_args = if args.first().map(String::as_str) == Some("attempt") {
        Some(&args[1..])
    } else if args.first().map(String::as_str) == Some("development")
        && args.get(1).map(String::as_str) == Some("attempt")
    {
        Some(&args[2..])
    } else {
        None
    };
    if let Some(args) = attempt_args {
        let result = if args.first().map(String::as_str) == Some("owner-action") {
            epilogos_factory::attempt_owner_cli::execute_attempt_owner_cli(&args[1..], None)
                .map_err(|error| error.to_string())
        } else {
            epilogos_factory::attempt_application::execute_attempt_cli(args, None)
                .map_err(|error| error.to_string())
        };
        return match result {
            Ok(output) => {
                println!("{output}");
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("factory attempt: {error}");
                std::process::ExitCode::from(2)
            }
        };
    }
    epilogos_factory::development_field_cli::cli_main()
}
