use anyhow::{Context as _, Result, ensure};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::PathBuf};

const EXPECTED_AUTHORITY_REPOSITORY: &str = "ORESoftware/ores-common-desktop-infra";
const EXPECTED_AUTHORITY_PR: u64 = 9;
const EXPECTED_AUTHORITY_REVISION: &str = "3fda402f724fc7e846d962645747cfd9d0cc21fc";

fn contract_path() -> PathBuf {
    return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ores-generation-contract.json");
}

fn load_contract() -> Result<Value> {
    let path = contract_path();
    let bytes = fs::read(&path)
        .with_context(|| format!("cannot read generation contract at {}", path.display()))?;
    return serde_json::from_slice(&bytes).context("generation contract is not valid JSON");
}

fn strings(value: &Value, field: &str) -> Result<Vec<String>> {
    let array = value
        .get(field)
        .and_then(Value::as_array)
        .with_context(|| format!("{field} must be an array"))?;
    let mut output = Vec::with_capacity(array.len());
    for item in array {
        let text = item
            .as_str()
            .with_context(|| format!("{field} entries must be strings"))?;
        output.push(text.to_owned());
    }
    return Ok(output);
}

#[test]
fn generation_contract_is_pinned_and_fail_closed() -> Result<()> {
    let contract = load_contract()?;

    ensure!(
        contract.get("schema").and_then(Value::as_str)
            == Some("ores.desktop-generation-consumer/v1"),
        "unexpected generation contract schema"
    );

    let authority = contract
        .get("authority")
        .context("authority object is required")?;
    ensure!(
        authority.get("repository").and_then(Value::as_str) == Some(EXPECTED_AUTHORITY_REPOSITORY),
        "generation authority repository drifted"
    );
    ensure!(
        authority.get("pull_request").and_then(Value::as_u64) == Some(EXPECTED_AUTHORITY_PR),
        "generation authority pull request drifted"
    );
    ensure!(
        authority.get("revision").and_then(Value::as_str) == Some(EXPECTED_AUTHORITY_REVISION),
        "generation authority revision drifted"
    );

    let expected_lifecycle = vec![
        "prepare",
        "validate",
        "compile_build_generation",
        "stage",
        "health_check",
        "atomic_activate",
        "bounded_drain",
        "commit",
    ];
    ensure!(
        strings(&contract, "lifecycle")?
            == expected_lifecycle
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        "generation lifecycle order drifted"
    );

    let rollback = contract.get("rollback").context("rollback is required")?;
    ensure!(
        rollback
            .get("required_before_commit")
            .and_then(Value::as_bool)
            == Some(true),
        "rollback must remain required before commit"
    );
    ensure!(
        rollback
            .get("retain_previous_generation")
            .and_then(Value::as_bool)
            == Some(true),
        "previous generation must remain available for rollback"
    );

    let requests = contract
        .get("request_semantics")
        .context("request_semantics is required")?;
    ensure!(
        requests.get("new_requests").and_then(Value::as_str) == Some("active_generation"),
        "new requests must use active generation"
    );
    ensure!(
        requests.get("existing_requests").and_then(Value::as_str) == Some("pinned_generation"),
        "in-flight requests must remain generation pinned"
    );
    ensure!(
        requests
            .get("generation_identity_required")
            .and_then(Value::as_bool)
            == Some(true),
        "generation identity must be mandatory"
    );

    let routing = contract.get("routing").context("routing is required")?;
    ensure!(
        routing
            .get("edge_proxy_route_authority")
            .and_then(Value::as_bool)
            == Some(false),
        "edge proxy must not become route authority"
    );
    let stable_edges = strings(routing, "stable_edges")?
        .into_iter()
        .collect::<BTreeSet<_>>();
    for required in ["nginx", "haproxy", "caddy"] {
        ensure!(
            stable_edges.contains(required),
            "stable edge set is missing {required}"
        );
    }

    let middleware = contract
        .get("middleware")
        .context("middleware is required")?;
    ensure!(
        middleware
            .get("beam_code_reload_requires_drain_or_otp_proof")
            .and_then(Value::as_bool)
            == Some(true),
        "BEAM reload must require drain or OTP proof"
    );

    let verification = contract
        .get("verification")
        .context("verification is required")?;
    ensure!(
        verification
            .get("shared_conformance_required")
            .and_then(Value::as_bool)
            == Some(true),
        "shared conformance must remain mandatory"
    );
    ensure!(
        verification
            .get("product_e2e_required")
            .and_then(Value::as_bool)
            == Some(true),
        "product E2E must remain mandatory"
    );
    ensure!(
        verification.get("promotion_state").and_then(Value::as_str) == Some("candidate"),
        "contract must remain candidate until shared + product verification pass"
    );

    return Ok(());
}
