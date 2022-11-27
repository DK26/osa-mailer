use std::env;

// https://stackoverflow.com/questions/65356683/how-to-mutate-serde-json-value-by-adding-additional-fields

mod entries;
mod errors;

const ENTRY_DIR: &str = "entries";
const ENTRY_EXT: &str = ".json";

fn main() -> anyhow::Result<()> {
    let current_exe =
        env::current_exe().expect("Unable to get the current binary file from the OS.");
    let current_exe_dir = current_exe
        .parent()
        .expect("Unable to get current binary file directory");

    let entries_path = current_exe_dir.join(ENTRY_DIR);

    let entry_parse_results = entries::load_entries(entries_path, ENTRY_EXT);

    eprintln!("Entry parsing errors: {:?}", entry_parse_results.err);

    let entries_pool = entry_parse_results.ok;

    let emails_map = entries::map_emails(entries_pool); // Each E-Mail ID with its E-mail contents, in order

    let res = entries::compose_emails(&emails_map);

    println!("res = {res:#?}");

    Ok(())
}
