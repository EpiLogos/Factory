fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("attempt") {
        return epilogos_factory::attempt_runtime::attempt_cli_main(&args[1..]);
    }
    epilogos_factory::development_field_cli::cli_main()
}
