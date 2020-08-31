use clap::{Arg, ArgMatches};

pub const LOG_CONFIG_FILE: &'static str = "log-config-file";
pub const CLUSTERS: &'static str = "clusters";
pub const SQL_FILES: &'static str = "sql-files";
pub const INIT_SQL_FILE: &'static str = "init-sql-file";
pub const RESET_SQL_FILE: &'static str = "reset-sql-file";

pub const TIDB_DOCKER_COMPOSE_DIR: &'static str = "tidb-docker-compose-dir";

pub fn clusters<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name(CLUSTERS)
        .short("c")
        .help("Specify the urls for connecting to each database cluster. Example: `-c test:test@127.0.0.1:3306 test:test@127.0.0.1:3307`.")
        .multiple(true)
        .takes_value(true)
        .required(true)
}

/// 为了减少参数输入，MySQL的url scheme可以不填，在本函数补存
///
/// # Arguments
///
/// * `matches` 解析后的获取到的参数
pub fn normalize_db_urls<'a>(matches: &ArgMatches<'a>) -> Vec<String> {
    let mut result = vec![];
    for url in matches.values_of(CLUSTERS).unwrap() {
        if !url.starts_with("mysql://") {
            result.push("mysql://".to_owned() + url);
        } else {
            result.push(String::from(url));
        }
    }
    result
}

pub fn sql_files<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name(SQL_FILES)
        .short("s")
        .multiple(true)
        .help("Specify all sql files.")
        .takes_value(true)
        .required(true)
}

pub fn init_sql_file<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name(INIT_SQL_FILE)
        .short("i")
        .help("Specify the sql file used to initiale the database.")
        .takes_value(true)
        .required(true)
}

pub fn reset_sql_file<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name(RESET_SQL_FILE)
        .short("r")
        .help("Specify the sql file used to reset the database between each batch. Be careful, init-sql-file will be executed after reset-sql-file.")
        .takes_value(true)
        .required(true)
}

pub fn log_config_file<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name(LOG_CONFIG_FILE)
        .short("l")
        .help("Specify the log config file.")
        .takes_value(true)
        .default_value("config/log4rs.yml")
}

pub fn tidb_docker_compose_dir<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name(TIDB_DOCKER_COMPOSE_DIR)
        .short("t")
        .help("Specify the folder path storing a docker-compose.yml and a config folder which is using to start a tidb cluster.")
        .takes_value(true)
        .default_value("config/tidb-docker-compose")
}
