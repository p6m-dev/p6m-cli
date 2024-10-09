use clap::ArgMatches;

pub fn init(matches: &ArgMatches) {
    loggerv::Logger::new()
        .verbosity(matches.get_count("verbosity") as u64)
        .level(true)
        .no_module_path()
        .add_module_path_filter("p6m")
        .module_path(false)
        .base_level(log::Level::Info)
        .init()
        .unwrap();
}
