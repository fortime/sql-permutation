# 介绍
本项目提供两个工具：
一个是cluster命令，通过docker-compose起tidb的集群（仅供测试使用），并支持通过集群号来支持一台机器起多个集群。
一个是interlace命令，读取多个SQL文件，生成这些文件的所有交错排列，然后执行。

# 依赖
* 编译工具：rustc - 1.48.0-nightly (d006f5734 2020-08-28)；cargo - cargo 1.47.0-nightly (51b66125b 2020-08-19)。
* docker工具：docker - 19.03.12-ce、docker-compose - 1.26.2。

# 编译
项目使用rust开发，由于使用了async closure特性，所以需要使用nightly的toolchain。
在本项目的根目录使用以下命令进行编译：
```sh
cargo build --release
```
编译后，会在target/release目录得到两个可执行文件：cluster、interlace。由于cluster命令或者需要root权限，不建议直接通过`cargo run`执行。

# 样例
## 启动一个tidb集群
命令：
```sh
sudo target/release/cluster up -n 0
```
命令说明：*-n 0*指定要启动的集群的序号为0，取值是正整数。命令执行后，会输出可以用于*interlace*命令的数据库url。
输出样例：
```text
Creating network "cluster_0_default" with the default driver
Creating cluster_0_pushgateway_1 ... done
Creating cluster_0_prometheus_1  ... done
Creating cluster_0_pd2_1         ... done
Creating cluster_0_pd1_1         ... done
Creating cluster_0_grafana_1     ... done
Creating cluster_0_pd0_1         ... done
Creating cluster_0_tikv2_1       ... done
Creating cluster_0_tikv1_1       ... done
Creating cluster_0_tikv0_1       ... done
Creating cluster_0_tidb_1        ... done
tidb cluster started, url: root:@127.0.0.1:32792/
```
## 停止一个tidb集群
命令：
```sh
sudo target/release/cluster down -n 0
```
命令说明：*-n 0*指定要停止的集群的序号为0，以*up*相对应。
输出样例：
```text
Stopping cluster_0_tidb_1        ... done
Stopping cluster_0_tikv1_1       ... done
Stopping cluster_0_tikv0_1       ... done
Stopping cluster_0_tikv2_1       ... done
Stopping cluster_0_pd0_1         ... done
Stopping cluster_0_grafana_1     ... done
Stopping cluster_0_pd1_1         ... done
Stopping cluster_0_pd2_1         ... done
Stopping cluster_0_prometheus_1  ... done
Stopping cluster_0_pushgateway_1 ... done
Removing cluster_0_tidb_1        ... done
Removing cluster_0_tikv1_1       ... done
Removing cluster_0_tikv0_1       ... done
Removing cluster_0_tikv2_1       ... done
Removing cluster_0_pd0_1         ... done
Removing cluster_0_grafana_1     ... done
Removing cluster_0_pd1_1         ... done
Removing cluster_0_pd2_1         ... done
Removing cluster_0_prometheus_1  ... done
Removing cluster_0_pushgateway_1 ... done
Removing network cluster_0_default
```
## 交错排列
### 样例准备
* 创建目录
```sh
mkdir -p workspace/sql
```
* 初始化SQL文件
```sql
create database if not exists test_db;

create table if not exists test_db.user (
    id int not null,
    name varchar(32) not null,
    primary key(id)
);
```
把以上内容保存到文件*workspace/sql/init.sql*。
* 重置SQL文件
```sql
drop database test_db;
```
把以上内容保存到文件*workspace/sql/reset.sql*。
* 待执行SQL文件A
```sql
insert into test_db.user values(1, "test-a");
update test_db.user set name='test-a-update1' where id = 1;
```
把以上内容保存到文件*workspace/sql/sql-file-a.sql*。
* 待执行SQL文件B
```sql
insert into test_db.user values(2, "test-b");
insert into test_db.user values(3, "test-b-1");
update test_db.user set name='test-b-update1' where id = 2;
```
把以上内容保存到文件*workspace/sql/sql-file-b.sql*。
### 命令说明
* 命令样例（无并发）：
```sh
target/release/interlace -i workspace/sql/init.sql -r workspace/sql/reset.sql -s workspace/sql/sql-file-* -c root:@127.0.0.1:32792/
```
* 命令样例（三并发）：
```sh
target/release/interlace -i workspace/sql/init.sql -r workspace/sql/reset.sql -s workspace/sql/sql-file-* -c root:@127.0.0.1:32792/ root:@127.0.0.1:33882/ root:@127.0.0.1:33968/
```
*interlace*会默认使用*config/log4rs.yml*日志文件，会把命令执行后的统计数据输出到*logs/statisitcs.log*文件。
* 输出样例
```text
2020-08-30T18:18:21.825128028+08:00 INFO sql_permutation::concurrent - =============start statistics of database[127.0.0.1:3306/]=============
2020-08-30T18:18:21.825323694+08:00 INFO sql_permutation::concurrent - Total time: 153.903058ms, Total batch executed: 9, Total SQL executed: 45
2020-08-30T18:18:21.825481143+08:00 INFO sql_permutation::concurrent - Average time(per batch): 17.100339ms, Average time(per SQL): 3.420067ms
2020-08-30T18:18:21.825655044+08:00 INFO sql_permutation::concurrent - Slowest SQL time: 7.544419ms - insert into test_db.user values(2, "test-b"); at (file 2, row 1)
2020-08-30T18:18:21.825820130+08:00 INFO sql_permutation::concurrent - Slowest batch time: 21.156049ms
Slowest batch:
2020-08-30T18:18:21.825984194+08:00 INFO sql_permutation::concurrent - insert into test_db.user values(2, "test-b"); at (file 2, row 1)
2020-08-30T18:18:21.826150317+08:00 INFO sql_permutation::concurrent - insert into test_db.user values(1, "test-a"); at (file 1, row 1)
2020-08-30T18:18:21.826312782+08:00 INFO sql_permutation::concurrent - insert into test_db.user values(3, "test-b-1"); at (file 2, row 2)
2020-08-30T18:18:21.826488212+08:00 INFO sql_permutation::concurrent - update test_db.user set name='test-b-update1' where id = 2; at (file 2, row 3)
2020-08-30T18:18:21.826641907+08:00 INFO sql_permutation::concurrent - update test_db.user set name='test-a-update1' where id = 1; at (file 1, row 2)
2020-08-30T18:18:21.826795647+08:00 INFO sql_permutation::concurrent - =============end statistics of database[127.0.0.1:3306/]===============
2020-08-30T18:18:21.826947432+08:00 INFO sql_permutation::concurrent - =============start statistics of database[192.168.1.103:4000/]=============
2020-08-30T18:18:21.827159612+08:00 INFO sql_permutation::concurrent - Total time: 4.698729897s, Total batch executed: 1, Total SQL executed: 5
2020-08-30T18:18:21.827313071+08:00 INFO sql_permutation::concurrent - Average time(per batch): 4.698729897s, Average time(per SQL): 939.745979ms
2020-08-30T18:18:21.827471120+08:00 INFO sql_permutation::concurrent - Slowest SQL time: 1.689467248s - insert into test_db.user values(1, "test-a"); at (file 1, row 1)
2020-08-30T18:18:21.827625306+08:00 INFO sql_permutation::concurrent - Slowest batch time: 4.698729897s
Slowest batch:
2020-08-30T18:18:21.827790427+08:00 INFO sql_permutation::concurrent - insert into test_db.user values(1, "test-a"); at (file 1, row 1)
2020-08-30T18:18:21.827962770+08:00 INFO sql_permutation::concurrent - insert into test_db.user values(2, "test-b"); at (file 2, row 1)
2020-08-30T18:18:21.828142690+08:00 INFO sql_permutation::concurrent - update test_db.user set name='test-a-update1' where id = 1; at (file 1, row 2)
2020-08-30T18:18:21.828323733+08:00 INFO sql_permutation::concurrent - insert into test_db.user values(3, "test-b-1"); at (file 2, row 2)
2020-08-30T18:18:21.828507678+08:00 INFO sql_permutation::concurrent - update test_db.user set name='test-b-update1' where id = 2; at (file 2, row 3)
2020-08-30T18:18:21.828696814+08:00 INFO sql_permutation::concurrent - =============end statistics of database[192.168.1.103:4000/]===============
```
