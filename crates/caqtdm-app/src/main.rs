fn main() {
    let args: Vec<String> = std::env::args().collect();
    match caqtdm_app::execute(&args) {
        Ok(text) => {
            if !text.is_empty() {
                println!("{text}");
            }
        }
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
