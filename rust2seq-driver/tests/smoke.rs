//! Smoke test for the rustc plugin pipeline. Runs the installed `cargo
//! rust2seq` binary against the `rust2seq-example` crate and asserts the
//! generated diagram has the expected shape.
//!
//! This test requires the driver to be installed locally:
//!
//! ```sh
//! cd clients && cargo install --path rust2seq/rust2seq-driver --debug --locked
//! ```
//!
//! It also requires `rust2seq-example`'s sources to be present (they're a
//! workspace sibling). CI scripts should install the driver before running
//! this test.

use std::path::PathBuf;
use std::process::Command;

fn example_dir() -> PathBuf {
    // `tests/smoke.rs` is at `clients/rust2seq/rust2seq-driver/tests/`.
    // The example crate sibling is at `clients/rust2seq/rust2seq-example/`.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .expect("rust2seq-driver should sit under rust2seq/")
        .join("rust2seq-example")
}

fn driver_installed() -> bool {
    Command::new("cargo")
        .args(["rust2seq", "--help"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn driver_emits_expected_oauth_diagram_shape() {
    if !driver_installed() {
        eprintln!(
            "skipping smoke test — `cargo rust2seq` not installed.\n\
             run: cd clients && cargo install --path rust2seq/rust2seq-driver --debug --locked"
        );
        return;
    }

    let example = example_dir();
    assert!(
        example.exists(),
        "rust2seq-example not found at {}",
        example.display()
    );

    // Wipe any leftover .puml files so a regression that fails to (re)emit
    // a diagram can't silently pass against stale on-disk content.
    let diagrams = example.join("diagrams");
    if diagrams.exists() {
        std::fs::remove_dir_all(&diagrams).expect("clear stale diagrams/");
    }

    let status = Command::new("cargo")
        .arg("rust2seq")
        .current_dir(&example)
        .status()
        .expect("failed to invoke cargo rust2seq");
    assert!(status.success(), "cargo rust2seq exited non-zero");

    let puml_path = example
        .join("diagrams")
        .join("oauth_authorization_code.puml");
    assert!(
        puml_path.exists(),
        "expected diagram not written at {}",
        puml_path.display()
    );

    let body = std::fs::read_to_string(&puml_path).expect("read diagram");

    // Spot-check the structural pieces we care about.
    assert!(body.starts_with("@startuml oauth_authorization_code"));
    assert!(body.contains("title OAuth"));
    assert!(body.contains("autonumber"));
    assert!(body.contains("participant \"User\" as User"));
    assert!(body.contains("participant \"User's browser"));
    assert!(body.contains("participant \"Authorization Server"));

    // Entry is User::click_login_button; the body calls Browser::handle_login,
    // so the first arrow is User -> Browser (no `from = "..."` override needed).
    assert!(body.contains("User -> Browser : handle login"));

    // The if/else inside AuthServer::authorize must render as an alt block.
    assert!(body.contains("alt challenge.0.is_empty()"));
    assert!(body.lines().any(|l| l.trim_end() == "else"));
    assert!(body.contains("400 Bad Request"));
    assert!(body.contains("record pending code"));

    // Custom #[seq::label] overrides.
    assert!(body.contains("derive PKCE pair"));
    assert!(body.contains("POST /token"));
    assert!(body.contains("GET /api/resource"));

    // Auto-emitted dashed response arrows.
    assert!(body.contains("AuthServer --> Browser : AuthCode"));
    assert!(body.contains("AuthServer --> Browser : Tokens"));
    assert!(body.contains("ResourceServer --> Browser : ResourceData"));
    // And Browser::handle_login → User return because handle_login returns RenderedUi.
    assert!(body.contains("Browser --> User : RenderedUi"));

    assert!(body.trim_end().ends_with("@enduml"));

    // ---- Scope-aware entry resolution: two sibling modules each declare
    //      their own `Frontend` + `Backend` + a `seq::diagram! { entry =
    //      Frontend::start }`. Neither must leak into the other.
    let alpha = std::fs::read_to_string(example.join("diagrams/alpha_protocol.puml"))
        .expect("read alpha diagram");
    assert!(
        alpha.contains("handle alpha"),
        "alpha diagram missing its own call:\n{alpha}"
    );
    assert!(
        !alpha.contains("handle beta"),
        "alpha diagram leaked beta_protocol contents:\n{alpha}"
    );

    let beta = std::fs::read_to_string(example.join("diagrams/beta_protocol.puml"))
        .expect("read beta diagram");
    assert!(
        beta.contains("handle beta"),
        "beta diagram missing its own call:\n{beta}"
    );
    assert!(
        !beta.contains("handle alpha"),
        "beta diagram leaked alpha_protocol contents:\n{beta}"
    );

    // ---- Qualified `crate::…` path: declared inside alpha_protocol but
    //      resolves into beta_protocol. Should render the beta call graph.
    let cross = std::fs::read_to_string(example.join("diagrams/cross_module_qualified.puml"))
        .expect("read cross-module diagram");
    assert!(
        cross.contains("handle beta"),
        "cross-module diagram should follow beta:\n{cross}"
    );
    assert!(
        !cross.contains("handle alpha"),
        "cross-module diagram must not include alpha:\n{cross}"
    );
}
