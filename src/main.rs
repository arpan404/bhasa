fn main() {
    let args: Vec<String> = match std::env::args_os()
        .skip(1)
        .map(|arg| arg.into_string())
        .collect()
    {
        Ok(args) => args,
        Err(_) => {
            eprintln!("error: command-line arguments must be valid UTF-8");
            std::process::exit(2);
        }
    };
    std::process::exit(bhasa::driver::run(&args));
}
