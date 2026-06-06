/*!
 * Logger ?周期性汇总器
 *
 * 功能描述：独立的 tokio 任务，按配置间隔调用各模块的统计接口? * 生成 AggregatedStats 事件，通过全局记录器输出? */

use tokio::time::{interval, Duration};

use super::config::AggregatorType;
use super::event::AggregatedStats;
use super::file_logger::record_event;

/// Spawn the aggregator task. Returns immediately; the task runs in background.
/// The `fetch_stats` closure is called periodically to get fresh statistics.
pub fn spawn_aggregator(
    interval_secs: u64,
    _enabled: Vec<AggregatorType>,
    fetch_stats: impl Fn() -> AggregatedStats + Send + 'static,
) {
    if interval_secs == 0 {
        return;
    }
    let period = Duration::from_secs(interval_secs);
    tokio::spawn(async move {
        let mut tick = interval(period);
        tick.tick().await; // Skip immediate first tick
        loop {
            tick.tick().await;
            let stats = fetch_stats();
            record_event(super::event::SystemEvent::AggregatedStats(stats));
        }
    });
}
