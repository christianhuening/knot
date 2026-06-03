# Grafana dashboard

`knot.json` is a starter dashboard for an operator running knot. It expects a Prometheus datasource and the metric names emitted by `knot-server` (see `crates/knot-obs/src/metrics.rs` for the registered names).

## Import

In Grafana 9+:

1. **Dashboards → New → Import**
2. Paste the contents of `knot.json` (or upload the file)
3. Pick the Prometheus datasource at the import prompt
4. Click **Import**

The dashboard has a single template variable `instance` (Prometheus instance label) so multi-replica deployments can be filtered.

## Panels

- **Top-line row** — requests/s, P95 latency, 5xx rate, active rooms (four stat panels at a glance)
- **HTTP row** — requests by route, P50/P95/P99 latency, error rate by route
- **Internals row** — CRDT updates/s by source (`local`|`peer`), snapshots/s, DB pool size+idle, DB pool busy %

## SLO thresholds

The `P95 latency` and `5xx error rate` stat panels carry coloured thresholds matching the values in `docs/SLO.md`. Adjust per workspace.

## Bundling into Helm (future)

The chart does not currently ship this dashboard as a ConfigMap with a `grafana_dashboard: "1"` label (the kube-prometheus-stack discovery mechanism). Adding it is a small follow-up — see Plan 10 outcome.
