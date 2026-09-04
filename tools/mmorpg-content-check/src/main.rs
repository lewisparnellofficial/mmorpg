fn main() {
    match mmorpg_content_check::run() {
        Ok(summary) => {
            println!("{summary}");
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
