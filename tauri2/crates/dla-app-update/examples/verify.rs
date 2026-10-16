//! Checks a release file's signature against a public update key, the way
//! the app does before it keeps an update:
//!
//!     cargo run -p dla-app-update --example verify -- FILE FILE.sig KEY.pub

use std::{env, fs, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [file, signature, key] = args.as_slice() else {
        eprintln!("usage: verify FILE FILE.sig KEY.pub");
        return ExitCode::from(2);
    };
    let read = |path: &str| fs::read_to_string(path).map_err(|error| format!("{path}: {error}"));
    let result = read(signature).and_then(|signature| {
        let key = read(key)?;
        dla_app_update::verify_signature(Path::new(file), &signature, &key)
            .map_err(|error| error.to_string())
    });
    match result {
        Ok(()) => {
            println!("verified: {file}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("NOT verified: {file}: {error}");
            ExitCode::FAILURE
        }
    }
}
