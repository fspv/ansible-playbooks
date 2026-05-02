use std::time::Duration;

use ansible_playbook_tests::{
    assert_url_returns_prometheus_metrics, run_command_must_succeed_within,
};

const SS_TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn node_exporter_serves_metrics() {
    assert_url_returns_prometheus_metrics("http://127.0.0.1:9100/metrics");
}

#[test]
fn smartctl_exporter_serves_metrics() {
    assert_url_returns_prometheus_metrics("http://127.0.0.1:9633/metrics");
}

#[test]
fn et_listening_on_2022() {
    let out = run_command_must_succeed_within("ss", &["-Htnl", "sport", "=", ":2022"], SS_TIMEOUT)
        .unwrap_or_else(|e| panic!("{e}"));
    let body = String::from_utf8_lossy(&out.stdout);
    assert!(
        !body.trim().is_empty(),
        "no listener on :2022 (et expected); ss output: {body:?}"
    );
}
