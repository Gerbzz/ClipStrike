use clipstrike_media::probe::probe_metadata_sync;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: probe_file <uri>");
        return;
    }
    let uri = &args[1];
    match probe_metadata_sync(uri) {
        Ok(metadata) => println!("Metadata: {:?}", metadata),
        Err(err) => println!("Error: {}", err),
    }
}
