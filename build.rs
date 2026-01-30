use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("dictionary.rs");

    // Read words from data/words.txt
    let words_path = Path::new("data/words.txt");
    let words_file = File::open(words_path).expect("Could not open data/words.txt");
    let reader = BufReader::new(words_file);

    let words: Vec<String> = reader
        .lines()
        .map_while(Result::ok)
        .map(|line| line.trim().to_lowercase())
        .filter(|word| word.len() == 5 && word.chars().all(|c| c.is_ascii_alphabetic()))
        .collect();

    // Build the phf::Set
    let mut set_builder = phf_codegen::Set::new();
    for word in &words {
        set_builder.entry(word.as_str());
    }

    // Write the generated code
    let mut file = BufWriter::new(File::create(dest_path).unwrap());
    writeln!(
        &mut file,
        "/// Valid 5-letter words for guessing ({} words).",
        words.len()
    )
    .unwrap();
    write!(&mut file, "static VALID_WORDS: phf::Set<&'static str> = ").unwrap();
    writeln!(&mut file, "{};", set_builder.build()).unwrap();

    // Rerun if words.txt changes
    println!("cargo:rerun-if-changed=data/words.txt");
}
