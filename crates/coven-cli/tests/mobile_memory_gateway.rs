use std::process::{Command, Output};

fn coven(home: &std::path::Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_coven"))
        .env("COVEN_HOME", home)
        .args(arguments)
        .output()
        .expect("run coven")
}

#[test]
fn mobile_gateway_is_disabled_by_default_in_a_fresh_home() {
    let home = tempfile::tempdir().unwrap();
    let output = coven(home.path(), &["memory", "mobile", "status", "--json"]);
    assert!(output.status.success(), "{output:?}");
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["configured"], false);
    assert_eq!(status["enabled"], false);
    assert!(!home.path().join("mobile/gateway.json").exists());
}

#[test]
fn mobile_enable_rejects_public_and_wildcard_bindings_without_state() {
    for bind in ["0.0.0.0:7443", "203.0.113.10:7443"] {
        let home = tempfile::tempdir().unwrap();
        let endpoint = format!("https://{bind}");
        let output = coven(
            home.path(),
            &[
                "memory",
                "mobile",
                "enable",
                "--bind",
                bind,
                "--endpoint",
                &endpoint,
            ],
        );
        assert!(!output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("private-network"),
            "{output:?}"
        );
        assert!(!home.path().join("mobile/gateway.json").exists());
    }
}
