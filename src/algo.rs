use anyhow::Result;
use futures::future::{BoxFuture, FutureExt};
use std::future::Future;

/// 生成每个队列交错的所有排列，每种交错情况生成完
/// 结果会作为参数调用回调函数
///
/// # Arguments
///
/// * `curs` -  每个队列当前位置
/// * `sizes` -  每个队列的大小
/// * `result` - 存放当前交错的结果
/// * `then` - 处理结果
pub fn interlace_permutation<'a, F, Fut>(
    curs: &'a mut Vec<usize>,
    sizes: &'a Vec<usize>,
    result: &'a mut Vec<(usize, usize)>,
    then: &'a F,
) -> BoxFuture<'a, Result<()>>
where
    F: Fn(Vec<(usize, usize)>) -> Fut + Send + Sync,
    Fut: Future<Output = Result<()>> + Send,
{
    async move {
        for i in 0..sizes.len() {
            // 保存最开始的位置
            let size = sizes[i];
            if curs[i] < size {
                // 把当前队列的索引及当前队列的位置放到结果里
                result.push((i, curs[i]));
                curs[i] += 1;
                interlace_permutation(curs, sizes, result, then).await?;
                // 还原
                curs[i] -= 1;
                result.pop();
            }
        }
        if !curs
            .iter()
            .zip(sizes.iter())
            .any(|(&cur, &size)| cur < size)
        {
            // 所有cur都不小于size，得到结果。
            // 复制结果，并交由回调函数处理
            return then(result.clone()).await;
        }
        Ok(())
    }
    .boxed()
}
