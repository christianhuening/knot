//! Prometheus metrics exporter.

use std::net::SocketAddr;

use metrics_exporter_prometheus::PrometheusBuilder;

#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("invalid address: {0}")]
    Address(String),
    #[error("install exporter: {0}")]
    Install(String),
}

/// Install the global metrics recorder and start the HTTP exporter
/// on `addr` (e.g. ":9090" or "0.0.0.0:9090").
pub fn init(addr: &str) -> Result<(), MetricsError> {
    let sa: SocketAddr = normalize_addr(addr)?
        .parse()
        .map_err(|e| MetricsError::Address(format!("{addr}: {e}")))?;

    PrometheusBuilder::new()
        .with_http_listener(sa)
        .install()
        .map_err(|e| MetricsError::Install(e.to_string()))?;
    Ok(())
}

fn normalize_addr(addr: &str) -> Result<String, MetricsError> {
    if let Some(port) = addr.strip_prefix(':')
        && port.parse::<u16>().is_ok()
    {
        return Ok(format!("0.0.0.0:{port}"));
    }
    Ok(addr.to_string())
}

#[cfg(test)]
mod tests {
    use super::normalize_addr;

    #[test]
    fn shorthand_port() {
        assert_eq!(normalize_addr(":9090").unwrap(), "0.0.0.0:9090");
    }

    #[test]
    fn explicit_addr() {
        assert_eq!(normalize_addr("127.0.0.1:9090").unwrap(), "127.0.0.1:9090");
    }
}
