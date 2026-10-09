use clipstrike_core::capture_import::import_package_to_new_project;
use clipstrike_core::capture_package::MediaValidation;
use std::path::Path;

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: clipstrike-import <capture-package-folder> <new-project.cstrike>");
        std::process::exit(2);
    }
    match import_package_to_new_project(Path::new(&args[0]), Path::new(&args[1])) {
        Ok(project) => {
            println!("{}: {} editable clips", project.name, project.clips.len());
            match project
                .capture_sources
                .first()
                .map(|source| source.media_validation)
            {
                Some(MediaValidation::StreamProbed) => eprintln!(
                    "Package reports stream-probed media; ClipStrike has not independently checked audible sound or HLAE origin."
                ),
                _ => eprintln!("STRUCTURAL ONLY: media and audio have not been probed."),
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
