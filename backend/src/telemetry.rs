//! Log em JSON (o Loki lê por linha) e o registrador de métricas Prometheus.

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

const DEFAULT_FILTER: &str = "info,sqlx=warn,tower_http=info,hyper=warn,reqwest=warn";

/// Liga o log. Chamada uma vez, no arranque.
pub fn init_logging() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .flatten_event(true)
                .with_current_span(false),
        )
        .try_init();
}

/// Instala o registrador global de métricas e devolve quem renderiza `/metrics`.
pub fn install_metrics() -> Option<PrometheusHandle> {
    match PrometheusBuilder::new().install_recorder() {
        Ok(handle) => Some(handle),
        Err(error) => {
            tracing::warn!(error = %error, "metricas desligadas");
            None
        }
    }
}
