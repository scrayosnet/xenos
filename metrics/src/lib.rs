use std::fmt::Debug;

pub use metrics_macros::metrics;
pub use std::collections::HashMap;

#[derive(Debug)]
pub struct MetricsEvent<'a, T> {
    pub metric: &'static str,
    pub labels: HashMap<&'static str, &'static str>,
    pub time: f64,
    pub result: &'a T,
}
