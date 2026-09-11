fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
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
