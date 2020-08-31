#![feature(async_closure)]

use anyhow::Result;
use clap::{crate_version, App, ArgMatches};
use mysql_async::Opts;
use std::collections::HashSet;
use std::process;

use sql_permutation::{
    algo, arg,
    concurrent::{ThreadPool, Worker},
    file, init_log,
};

fn parse_params<'a, 'b>(app: App<'a, 'b>) -> ArgMatches<'a> {
    let app = app
        .arg(arg::log_config_file())
        .arg(arg::clusters())
        .arg(arg::sql_files())
        .arg(arg::init_sql_file())
        .arg(arg::reset_sql_file());
    app.get_matches()
}

/// 检查db的url是否有效及不重复
fn into_mysql_opts(db_urls: Vec<String>) -> Result<Vec<Opts>> {
    let mut result = Vec::with_capacity(db_urls.len());
    let mut target_set = HashSet::with_capacity(db_urls.len());
    for db_url in db_urls {
        let opts = Opts::from_url(&db_url)?;
        let no_dup = target_set.insert((String::from(opts.ip_or_hostname()), opts.tcp_port()));
        if !no_dup {
            return Err(anyhow::Error::msg(format!(
                "Dulicate mysql instance[{}:{}]",
                opts.ip_or_hostname(),
                opts.tcp_port()
            )));
        }
        result.push(opts);
    }
    Ok(result)
}

async fn run<'a>(matches: ArgMatches<'a>) -> Result<()> {
    init_log(matches.value_of(arg::LOG_CONFIG_FILE))?;
    // 读取初始化sql，重置sql，及交错执行的sql
    let init_sqls = file::read_sqls(matches.value_of(arg::INIT_SQL_FILE).unwrap()).await?;
    let reset_sqls = file::read_sqls(matches.value_of(arg::RESET_SQL_FILE).unwrap()).await?;
    let mut sqls_list = vec![];
    let mut sizes = vec![];
    let mut curs = vec![];
    for file in matches.values_of(arg::SQL_FILES).unwrap() {
        let sqls = file::read_sqls_by_line(file).await?;
        curs.push(0);
        sizes.push(sqls.len());
        sqls_list.push(sqls);
    }
    // 解析mysql的参数
    let mysql_opts = into_mysql_opts(arg::normalize_db_urls(&matches))?;
    let mut thread_pool = ThreadPool::new(mysql_opts.len());
    for mysql_opt in mysql_opts {
        thread_pool.add_worker(Worker::new(&init_sqls, &reset_sqls, &sqls_list, mysql_opt));
    }

    let thread_pool_ref = &thread_pool;
    let result =
        algo::interlace_permutation(&mut curs, &sizes, &mut Vec::new(), &async move |result| {
            thread_pool_ref.submit(result).await
        })
        .await;
    if result.is_ok() {
        // 成功处理完成，关闭线程池
        log::info!("interlace permutation finished.");
        thread_pool.shutdown().await;
    }
    log::info!("waiting all workers to be finished.");
    // 等待所有任务结束
    thread_pool.join().await;

    // 打印统计信息及异常
    thread_pool.print_statistic(&sqls_list).await;

    Ok(())
}

#[tokio::main]
async fn main() {
    let app = App::new("SQL Interlace Permutation Executor")
        .version(crate_version!())
        .author("fortime <palfortime@gmail.com>")
        .about("The command for executing all interlace permutations of multiple SQL files.");
    match run(parse_params(app)).await {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Something is wrong:\n{:#?}", e);
            process::exit(1);
        }
    };
}
