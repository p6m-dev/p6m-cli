use std::fs;

use clap::ArgMatches;
use globset::{Glob, GlobSetBuilder};
use log::{debug, error, info, trace, warn};
use walkdir::{DirEntry, WalkDir};

pub fn execute(matches: &ArgMatches) -> Result<(), anyhow::Error> {
    match matches.subcommand() {
        Some(("ide-files", subargs)) => purge_ide_files(subargs),
        Some(("maven", subargs)) => purge_maven(subargs),
        Some((command, _)) => error!("Unimplemented purge command: '{}'", command),
        None => error!("Unspecified purge command"),
    }

    Ok(())
}

pub fn purge_ide_files(matches: &ArgMatches) {
    let mut ide_files_glob_builder = GlobSetBuilder::new();
    ide_files_glob_builder.add(Glob::new("*.iml").unwrap());
    ide_files_glob_builder.add(Glob::new("**/.idea").unwrap());
    ide_files_glob_builder.add(Glob::new(".project").unwrap());
    ide_files_glob_builder.add(Glob::new(".classpath").unwrap());
    ide_files_glob_builder.add(Glob::new("**/.settings").unwrap());

    let dry_run = matches.get_flag("dry-run");

    if dry_run {
        warn!("Dry Run: No files will be deleted...");
    }
    let ide_files_glob = ide_files_glob_builder.build().unwrap();

    let mut it = WalkDir::new(".").follow_links(false).into_iter();
    loop {
        let entry = match it.next() {
            None => break,
            Some(Err(err)) => panic!("Error: {}", err),
            Some(Ok(entry)) => entry,
        };

        let path = entry.path();
        if ide_files_glob.is_match(path) {
            info!("Removing {}", path.display());
            if !dry_run {
                if path.is_file() {
                    fs::remove_file(path)
                        .unwrap_or_else(|_| panic!("Error removing {}", path.display()));
                } else if path.is_dir() {
                    fs::remove_dir_all(path)
                        .unwrap_or_else(|_| panic!("Error removing directory {}", path.display()));
                }
            }
            if path.is_dir() {
                it.skip_current_dir();
            }
            continue;
        }

        if is_hidden(&entry) {
            if entry.file_type().is_dir() {
                debug!("Skipping: {}", entry.path().display());
                it.skip_current_dir();
            }
            continue;
        }

        trace!("Considering: {}", entry.path().display());
    }
}

fn purge_maven(matches: &ArgMatches) {
    if let Some(path) = matches.get_one::<String>("path") {
        if let Some(home_dir) = dirs::home_dir() {
            let purge_dir = &mut home_dir.clone();
            purge_dir.push(".m2/repository");
            if path.starts_with('.') || path.starts_with('/') {
                error!("Invalid purge path '{}'.", path);
                return;
            }
            purge_dir.push(path.replace('.', "/"));
            if purge_dir.exists() {
                info!("Purging Maven cache directory: {:?}", purge_dir.as_os_str());
                fs::remove_dir_all(&purge_dir)
                    .unwrap_or_else(|_| panic!("Error deleting {:?}", purge_dir));
            } else {
                warn!("Maven cache directory does not exist: {:?}", purge_dir);
            }
        } else {
            error!("Unable to obtain the location of your home directory!");
        }
    }
}

fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.') && !s.eq("."))
        .unwrap_or(false)
}
