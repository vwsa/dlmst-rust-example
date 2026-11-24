use std::process::Command;

#[test]
fn fixture_cli_and_invalid_configuration() {
    let output = Command::new(env!("CARGO_BIN_EXE_dlmst"))
        .args([
            "solve",
            "examples/small.json",
            "--verify",
            "--boruvka-steps",
            "0",
            "--base-case-edges",
            "0",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["petgraph_reference"]["matches"], true);
    assert_eq!(value["result"]["edge_ids"], serde_json::json!([1, 2, 3, 5]));
    assert_eq!(value["result"]["total_weight"], 5);
    assert_eq!(value["result"]["components"], 2);

    let bad = Command::new(env!("CARGO_BIN_EXE_dlmst"))
        .args(["demo", "--blocks", "0"])
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("blocks must be"));
}
