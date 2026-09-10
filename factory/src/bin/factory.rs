fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
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
        return match epilogos_factory::attempt_application::execute_attempt_cli(args, None) {
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
