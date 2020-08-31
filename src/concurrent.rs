use anyhow::{Error, Result};
use mysql_async::prelude::*;
use mysql_async::{Opts, Pool};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, MutexGuard, Notify, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;

use crate::arg;

pub struct StateMut {
    /// 是否运行中
    running: bool,
    /// 是否由于异常被中止
    abort: bool,
    queue: VecDeque<(OwnedSemaphorePermit, Vec<(usize, usize)>)>,
    waiting_worker_signals: VecDeque<Arc<Notify>>,
    total_statistics: Vec<(String, Statistics)>,
}

impl StateMut {
    fn clear(&mut self) {
        // 清空key，返回produce_permit到信号量
        while let Some((_permit, _)) = self.queue.pop_front() {}
        // 通知所有等待的Worker
        while let Some(signal) = self.waiting_worker_signals.pop_front() {
            signal.notify();
        }
    }

    pub async fn abort(&mut self) {
        self.abort = true;
        self.running = false;
        self.clear();
    }

    pub async fn shutdown(&mut self) {
        self.running = false;
        self.clear();
    }
}

pub struct State {
    state_mut: Mutex<StateMut>,
    produce_permits: Arc<Semaphore>,
}

impl State {
    pub fn new(queue_capacity: usize) -> Self {
        Self {
            state_mut: Mutex::new(StateMut {
                running: true,
                abort: false,
                queue: VecDeque::with_capacity(queue_capacity),
                waiting_worker_signals: VecDeque::new(),
                total_statistics: vec![],
            }),
            produce_permits: Arc::new(Semaphore::new(queue_capacity)),
        }
    }

    pub async fn lock<'a>(&'a self) -> MutexGuard<'a, StateMut> {
        self.state_mut.lock().await
    }
}

pub struct Statistics {
    /// 正常完成时会被清空
    cur_batch: Option<Vec<(usize, usize)>>,
    /// 按索引从1开始，0代表无效，方便打印时不用每次解Option
    cur_batch_idx: usize,
    last_batch: Option<Vec<(usize, usize)>>,
    error: Option<Error>,
    sql_amount: usize,
    batch_amount: usize,
    time: Duration,
    slowest_sql: Option<(usize, usize)>,
    slowest_sql_time: Duration,
    slowest_batch: Option<Vec<(usize, usize)>>,
    slowest_batch_time: Duration,
}

impl Statistics {
    pub fn new() -> Self {
        Statistics {
            cur_batch: None,
            cur_batch_idx: 0,
            last_batch: None,
            error: None,
            sql_amount: 0,
            batch_amount: 0,
            time: Duration::from_nanos(0),
            slowest_sql: None,
            slowest_sql_time: Duration::from_nanos(0),
            slowest_batch: None,
            slowest_batch_time: Duration::from_nanos(0),
        }
    }

    /// # Arguments
    ///
    /// * `error_idx` - 从1开始，0代表无效
    fn print_sql_batch(
        &self,
        sqls_list: &Vec<Vec<String>>,
        batch: &Vec<(usize, usize)>,
        error_idx: Option<usize>,
    ) {
        let mut padding = "";
        // 0代表不存在
        let mut i = 0;
        if let Some(error_idx) = error_idx {
            padding = "       ";
            i = error_idx;
        }
        for (idx, (file_idx, sql_idx)) in batch.iter().enumerate() {
            let mut padding = padding;
            if i == idx + 1 {
                padding = "err -> ";
            }
            log::info!(
                "{}{} at (file {}, row {})",
                padding,
                sqls_list[*file_idx][*sql_idx],
                file_idx + 1,
                sql_idx + 1
            );
        }
    }

    pub fn print(&self, sqls_list: &Vec<Vec<String>>, abort: bool) {
        if let Some(error) = &self.error {
            log::info!("Error happend!\n{}", error);
            match &self.cur_batch {
                None => {
                    log::info!(
                        "Error happened before handling any batch. Please check {} and {}",
                        arg::INIT_SQL_FILE,
                        arg::RESET_SQL_FILE
                    );
                }
                Some(cur_batch) => {
                    log::info!("Error happened while handling batch:");
                    self.print_sql_batch(sqls_list, cur_batch, Some(self.cur_batch_idx));
                }
            };
            return;
        }
        if abort {
            match &self.last_batch {
                None => {
                    log::info!("No batch has been handled in this database.");
                }
                Some(last_batch) => {
                    log::info!("Last handled batch:");
                    self.print_sql_batch(sqls_list, last_batch, None);
                }
            };
        } else {
            if self.sql_amount == 0 {
                log::info!("No SQL executed in this database.");
                return;
            }
            log::info!(
                "Total time: {:?}, Total batch executed: {}, Total SQL executed: {}",
                self.time,
                self.batch_amount,
                self.sql_amount
            );
            log::info!(
                "Average time(per batch): {:?}, Average time(per SQL): {:?}",
                self.time / self.batch_amount as u32,
                self.time / self.sql_amount as u32,
            );
            if let Some((file_idx, sql_idx)) = &self.slowest_sql {
                log::info!(
                    "Slowest SQL time: {:?} - {} at (file {}, row {})",
                    self.slowest_sql_time,
                    sqls_list[*file_idx][*sql_idx],
                    file_idx + 1,
                    sql_idx + 1
                );
            }
            if let Some(slowest_batch) = &self.slowest_batch {
                log::info!(
                    "Slowest batch time: {:?}\nSlowest batch:",
                    self.slowest_batch_time,
                );
                self.print_sql_batch(sqls_list, slowest_batch, None);
            }
        }
    }
}

pub struct Worker {
    init_sqls: String,
    reset_sqls: String,
    sqls_list: Vec<Vec<String>>,
    mysql_pool: Pool,
    mysql_target: String,
    signal: Arc<Notify>,
}

impl Worker {
    pub fn new(
        init_sqls: &String,
        reset_sqls: &String,
        sqls_list: &Vec<Vec<String>>,
        mysql_opts: Opts,
    ) -> Self {
        // 由于线程及sql的量不大，且为常熟，直接拷贝
        Worker {
            init_sqls: init_sqls.clone(),
            reset_sqls: reset_sqls.clone(),
            sqls_list: sqls_list.clone(),
            mysql_target: format!(
                "{}:{}/{}",
                mysql_opts.ip_or_hostname(),
                mysql_opts.tcp_port(),
                mysql_opts.db_name().unwrap_or("")
            ),
            mysql_pool: Pool::new(mysql_opts),
            signal: Arc::new(Notify::new()),
        }
    }

    async fn recv(&self, state: &Arc<State>) -> Option<Vec<(usize, usize)>> {
        loop {
            let mut state_mut = state.lock().await;
            if state_mut.abort {
                return None;
            }
            match state_mut.queue.pop_front() {
                Some((permit, r)) => {
                    // 把permit放回信号量
                    drop(permit);
                    return Some(r);
                }
                None => {
                    if !state_mut.running {
                        // 已经完成且消费完，返回None
                        return None;
                    } else {
                        // 放signal到等待队列
                        state_mut.waiting_worker_signals.push_back(Arc::clone(&self.signal));
                        // 放弃锁，等signal
                        drop(state_mut);
                        self.signal.notified().await;
                        log::trace!("signal received. continue!");
                        // 有信号，重试
                        continue;
                    }
                }
            }
        }
    }

    pub async fn run_with_error(
        &self,
        state: &Arc<State>,
        statistics: &mut Statistics,
    ) -> Result<()> {
        // 先执行一次初始化，避免第一次reset出错
        let mut conn = self.mysql_pool.get_conn().await?;
        conn.query_drop(&self.init_sqls).await?;
        drop(conn);

        loop {
            let result = match self.recv(state).await {
                Some(r) => r,
                None => {
                    statistics.last_batch = statistics.cur_batch.take();
                    statistics.cur_batch_idx = 0;
                    break;
                }
            };

            log::debug!("One batch generated!");
            // 循环里重新获取连接
            let mut conn = self.mysql_pool.get_conn().await?;
            // 先执行重置的sql，可以解决前一次执行程序产生了
            // 脏数据的情况
            conn.query_drop(&self.reset_sqls).await?;
            // 再重新初始化sql
            conn.query_drop(&self.init_sqls).await?;

            statistics.cur_batch.replace(result);
            statistics.batch_amount += 1;
            // 按索引从1开始
            statistics.cur_batch_idx = 0;
            let mut batch_time = Duration::from_nanos(0);
            for (file_idx, sql_idx) in statistics.cur_batch.as_ref().unwrap() {
                statistics.sql_amount += 1;
                statistics.cur_batch_idx += 1;
                let sql = &self.sqls_list[*file_idx][*sql_idx];
                log::debug!("{:?}", sql);
                let begin = Instant::now();
                conn.query_drop(sql).await?;
                let sql_time = begin.elapsed();
                if sql_time > statistics.slowest_sql_time {
                    statistics.slowest_sql_time = sql_time;
                    statistics.slowest_sql.replace((*file_idx, *sql_idx));
                }
                batch_time += sql_time;
            }
            // 统计执行时间
            statistics.time += batch_time;
            if batch_time > statistics.slowest_batch_time {
                statistics.slowest_batch_time = batch_time;
                statistics
                    .slowest_batch
                    .replace(statistics.cur_batch.as_ref().unwrap().clone());
            }
            // 返回连接到池子，避免由于等待sql过久，导致连接没有
            // 保活而失效
            drop(conn);
        }
        Ok(())
    }

    pub async fn run(self, state: Arc<State>) {
        let mut statistics = Statistics::new();
        match self.run_with_error(&state, &mut statistics).await {
            Ok(_) => {}
            Err(e) => statistics.error = Some(e),
        };
        let is_error = statistics.error.is_some();

        let mut state_mut = state.lock().await;
        if is_error {
            // 发生异常设置状态
            state_mut.abort().await;
        }
        // 存放统计数据
        state_mut
            .total_statistics
            .push((self.mysql_target, statistics));
    }
}

pub struct ThreadPool {
    state: Arc<State>,
    _max_buffer_size: usize,
    worker_handles: Vec<JoinHandle<()>>,
}

impl ThreadPool {
    pub fn new(max_buffer_size: usize) -> Self {
        ThreadPool {
            state: Arc::new(State::new(max_buffer_size)),
            _max_buffer_size: max_buffer_size,
            worker_handles: vec![],
        }
    }

    /// 把生成好的sql索引批次提交到线程池
    pub async fn submit(&self, sql_idx_batch: Vec<(usize, usize)>) -> Result<()> {
        // 先获取permit，再放到队列
        let permit = self.state.produce_permits.clone().acquire_owned().await;
        // 获取锁
        let mut state_mut = self.state.lock().await;
        if !state_mut.running {
            // 线程池被关闭
            return Err(Error::msg("ThreadPool is not running"));
        }
        state_mut.queue.push_back((permit, sql_idx_batch));
        // 通知一个阻塞的Worker
        if let Some(signal) = state_mut.waiting_worker_signals.pop_front() {
            signal.notify();
        }
        log::trace!("sql idx batch pushed.");
        Ok(())
    }

    pub fn add_worker(&mut self, worker: Worker) {
        let state = Arc::clone(&self.state);
        let handle = tokio::spawn(async move {
            worker.run(state).await;
        });
        // 增加用于生产的permit
        self.state.produce_permits.add_permits(1);
        self.worker_handles.push(handle);
    }

    pub async fn shutdown(&self) {
        self.state.lock().await.shutdown().await;
    }

    pub async fn join(&mut self) {
        // 等待其他任务完成
        for handle in self.worker_handles.drain(..) {
            match handle.await {
                Ok(_) => {}
                Err(e) => {
                    log::warn!("waiting handle failed:\n{}", e);
                }
            }
        }
    }

    pub async fn print_statistic(&self, sqls_list: &Vec<Vec<String>>) {
        let state_mut = self.state.lock().await;
        for (mysql_target, statistics) in state_mut.total_statistics.iter() {
            log::info!(
                "=============start statistics of database[{}]=============",
                mysql_target
            );
            statistics.print(sqls_list, state_mut.abort);
            log::info!(
                "=============end statistics of database[{}]===============",
                mysql_target
            );
        }
    }
}
