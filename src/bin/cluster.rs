use anyhow::{Error, Result};
use clap::{crate_version, App, Arg, ArgMatches, SubCommand};
use std::process::{self, Command};
use std::str::FromStr;

use sql_permutation::{arg, init_log};

const DOCKER_COMPOSE_PROJECT_PREFIX: &str = "cluster";

fn parse_params<'a, 'b>(app: App<'a, 'b>) -> ArgMatches<'a> {
    let cluster_number = Arg::with_name("cluster-number")
        .short("n")
        .help("Specify the number of a cluster.")
        .takes_value(true)
        .required(true);
    let app = app
        .arg(arg::log_config_file())
        .arg(arg::tidb_docker_compose_dir())
        .subcommand(
            SubCommand::with_name("up")
                .about("Create and start tidb clusters")
                .arg(cluster_number.clone()),
        )
        .subcommand(
            SubCommand::with_name("down")
                .about("Stop and destory tidb clusters")
                .arg(cluster_number),
        );

    app.get_matches()
}

fn cluster_number(subcommand_matches: &ArgMatches) -> Result<usize> {
    subcommand_matches
        .value_of("cluster-number")
        .ok_or_else(|| Error::msg("cluster-number does not exist!"))
        .and_then(|s| {
            FromStr::from_str(s).map_err(|_| Error::msg("cluster-number must be number!"))
        })
}

fn spawn_command(command: &mut Command) -> Result<()> {
    let exit_status = command.spawn()?.wait()?;
    match exit_status.code() {
        Some(code) => {
            if code != 0 {
                process::exit(code);
            }
        }
        None => {
            return Err(Error::msg("Process terminated by signal!"));
        }
    }
    Ok(())
}

fn output_command(command: &mut Command) -> Result<String> {
    let output = command.output()?;
    match &output.status.code() {
        Some(code) => {
            if *code != 0 {
                // 打印标准输出及标准错误输出字符串
                eprint!("{}", String::from_utf8_lossy(&output.stderr));
                print!("{}", String::from_utf8_lossy(&output.stdout));
                process::exit(*code);
            }
            return Ok(String::from_utf8(output.stdout)?);
        }
        None => {
            return Err(Error::msg("Process terminated by signal!"));
        }
    }
}

fn check_deps() -> Result<()> {
    // 检查命令是否存在
    output_command(Command::new("docker").arg("-h"))
        .map_err(|_| Error::msg("Command `docker` not found. Please install docker!"))?;
    output_command(Command::new("docker-compose").arg("-h")).map_err(|_| {
        Error::msg("Command `docker-compose` not found. Please install docker-compose!")
    })?;
    // 检查是否有足够权限执行
    let output = Command::new("docker").arg("ps").output()?;
    match &output.status.code() {
        Some(code) => {
            if *code != 0 {
                // 打印标准输出及标准错误输出字符串
                eprint!("{}", String::from_utf8_lossy(&output.stderr));
                print!("{}", String::from_utf8_lossy(&output.stdout));
                return Err(Error::msg(
                    "Doesn't have enough permission! Try to run this command with root.",
                ));
            }
        }
        None => {
            return Err(Error::msg("Process terminated by signal!"));
        }
    }
    Ok(())
}

fn down(subcommand_matches: &ArgMatches, tidb_docker_compose_dir: &str) -> Result<()> {
    let cluster_number = cluster_number(subcommand_matches)?;
    let project_name = format!("{}_{}", DOCKER_COMPOSE_PROJECT_PREFIX, cluster_number);
    log::debug!("project_name: {}", project_name);
    let mut command = Command::new("docker-compose");
    command
        .current_dir(tidb_docker_compose_dir)
        .arg("-p")
        .arg(project_name)
        .arg("down");
    spawn_command(&mut command)?;
    Ok(())
}

fn up(subcommand_matches: &ArgMatches, tidb_docker_compose_dir: &str) -> Result<()> {
    let cluster_number = cluster_number(subcommand_matches)?;
    let project_name = format!("{}_{}", DOCKER_COMPOSE_PROJECT_PREFIX, cluster_number);
    log::debug!("project_name: {}", project_name);
    // 启动集群
    let mut command = Command::new("docker-compose");
    command
        .current_dir(tidb_docker_compose_dir)
        .arg("-p")
        .arg(&project_name)
        .arg("up")
        .arg("-d");
    spawn_command(&mut command)?;
    // 获取tidb的端口，生成mysql的连接
    let tidb_name = format!("{}_tidb_1", project_name);
    log::debug!("tidb_name: {}", tidb_name);
    command = Command::new("docker");
    command.arg("port").arg(tidb_name);
    let stdout = output_command(&mut command)?;
    // 获取4000端口的行
    let mut tidb_target = None;
    for line in stdout.split("\n") {
        if !line.starts_with("4000/tcp") {
            continue;
        }
        tidb_target = line.split("->").nth(1);
    }
    if let Some(tidb_target) = tidb_target {
        println!(
            "tidb cluster started, url: root:@127.0.0.1:{}/",
            tidb_target.trim().replace("0.0.0.0:", "")
        );
    } else {
        return Err(Error::msg(format!(
            "Doesn't find tidb port(4000) mapping!\n{}",
            stdout
        )));
    }
    Ok(())
}

fn run(matches: &ArgMatches) -> Result<()> {
    check_deps()?;
    // 初始化日志配置
    init_log(matches.value_of(arg::LOG_CONFIG_FILE))?;
    let tidb_docker_compose_dir = matches.value_of(arg::TIDB_DOCKER_COMPOSE_DIR).unwrap();
    match &matches.subcommand {
        Some(subcommand) => {
            let subcommand_matches = &subcommand.matches;
            match subcommand.name.as_ref() {
                "up" => up(subcommand_matches, tidb_docker_compose_dir)?,
                "down" => down(subcommand_matches, tidb_docker_compose_dir)?,
                _ => unreachable!("Unknown sub comand: {}", subcommand.name),
            }
        }
        None => {
            return Err(Error::msg(
                "No subcommand provided! Add `--help` to show usage.",
            ));
        }
    }

    Ok(())
}

pub fn main() {
    let app = App::new("Clusters Management Command")
        .version(crate_version!())
        .author("fortime <palfortime@gmail.com>")
        .about("The command for up/down a tidb cluster by docker-compose.");
    let matches = parse_params(app);
    match run(&matches) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{:#?}", e);
            process::exit(1);
        }
    };
}
