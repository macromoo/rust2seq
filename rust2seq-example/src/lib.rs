//! # OAuth 2.0 Authorization Code + PKCE — modelled with rust2seq
//!
//! Real Rust code that *implements* the OAuth flow as a sketch. The bodies
//! are intentionally minimal but they actually call each other across
//! `#[seq::participant]` types. The diagram at
//! `diagrams/oauth_authorization_code.puml` is regenerated from the call
//! graph rooted at `User::click_login_button` by walking the typed AST.
//!
//! To regenerate: `cargo rust2seq` from this dir.

#![allow(dead_code)]

use rust2seq as seq;

// ---------------------------------------------------------------------------
// Domain types — small newtype wrappers so the response-arrow labels in the
// generated diagram are meaningful (`Tokens`, `AuthCode`, …) rather than the
// noise that `(String, String)` would produce.
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Verifier(pub String);

#[derive(Clone)]
pub struct Challenge(pub String);

#[derive(Clone)]
pub struct AuthCode(pub String);

#[derive(Clone)]
pub struct AccessToken(pub String);

#[derive(Clone)]
pub struct Tokens {
    pub access: AccessToken,
    pub refresh: String,
    pub expires_in_secs: u64,
}

#[derive(Clone)]
pub struct ResourceData(pub String);

#[derive(Clone)]
pub struct RenderedUi;

// ---------------------------------------------------------------------------
// Diagram declaration — no participants list. The walker discovers them from
// the call graph rooted at `entry` and orders them left-to-right by first
// appearance. Output defaults to `diagrams/<name>.puml` relative to crate
// root, so we don't bother setting it explicitly.
// ---------------------------------------------------------------------------

seq::diagram! {
    name = "oauth_authorization_code",
    title = "OAuth 2.0 — Authorization Code grant with PKCE",
    entry = User::click_login_button,
}

// ---------------------------------------------------------------------------
// Participants — each gets a lifeline on the diagram. `display = "..."` sets
// the column label; without it, the alias defaults to the type name.
// ---------------------------------------------------------------------------

#[seq::participant]
pub struct User;

#[seq::participant(display = "User's browser\n(SPA client)", color = "#ffe1a8")]
pub struct Browser {
    pub session_id: String,
}

#[seq::participant(display = "Authorization Server\n(OAuth provider)", color = "#c1d7f0")]
pub struct AuthServer;

#[seq::participant(display = "Resource Server\n(API)", color = "#d6f5d6")]
pub struct ResourceServer;

// ---------------------------------------------------------------------------
// User side — the flow's entry point. The body calls into Browser, which
// gives us the natural first arrow `User -> Browser`.
// ---------------------------------------------------------------------------

impl User {
    #[seq::msg]
    #[seq::label("click \"Log in\"")]
    pub fn click_login_button() {
        let mut browser = Browser {
            session_id: String::new(),
        };
        browser.handle_login();
    }
}

// ---------------------------------------------------------------------------
// Browser side
// ---------------------------------------------------------------------------

impl Browser {
    #[seq::msg]
    fn handle_login(&mut self) -> RenderedUi {
        let (verifier, challenge) = self.derive_pkce();
        let code = AuthServer::authorize(challenge);
        let tokens = AuthServer::exchange(code, verifier);
        let data = ResourceServer::fetch(&tokens.access);
        self.render(data)
    }

    #[seq::msg]
    #[seq::label("derive PKCE pair (verifier + SHA-256 challenge)")]
    fn derive_pkce(&self) -> (Verifier, Challenge) {
        // In real code this would be:
        //   verifier   = base64url(rand_bytes(32))
        //   challenge  = base64url(sha256(verifier.as_bytes()))
        (
            Verifier("RANDOM_VERIFIER".into()),
            Challenge("SHA256_OF_VERIFIER".into()),
        )
    }

    #[seq::msg]
    fn render(&mut self, _data: ResourceData) -> RenderedUi {
        RenderedUi
    }
}

// ---------------------------------------------------------------------------
// Authorization Server side
// ---------------------------------------------------------------------------

impl AuthServer {
    /// User-facing consent screen + redirect with auth code. Demonstrates
    /// that `if` shows up as an `alt` block on the diagram.
    #[seq::msg]
    #[seq::label("GET /authorize?code_challenge=… → consent → 302 ?code=…")]
    fn authorize(challenge: Challenge) -> AuthCode {
        if challenge.0.is_empty() {
            AuthServer::reject_invalid_challenge();
            AuthCode(String::new())
        } else {
            AuthServer::record_pending_code(&challenge);
            AuthCode("ONE_TIME_CODE".into())
        }
    }

    #[seq::msg(color = "#cc3333")]
    #[seq::label("400 Bad Request — invalid PKCE challenge")]
    fn reject_invalid_challenge() {}

    #[seq::msg]
    fn record_pending_code(_challenge: &Challenge) {}

    /// PKCE-verified token exchange.
    #[seq::msg]
    #[seq::label("POST /token { code, code_verifier }")]
    fn exchange(_code: AuthCode, _verifier: Verifier) -> Tokens {
        // Real impl checks SHA256(verifier) == stored challenge before issuing.
        Tokens {
            access: AccessToken("ACCESS_TOKEN".into()),
            refresh: "REFRESH_TOKEN".into(),
            expires_in_secs: 3600,
        }
    }
}

// ---------------------------------------------------------------------------
// Resource Server side
// ---------------------------------------------------------------------------

impl ResourceServer {
    #[seq::msg]
    #[seq::label("GET /api/resource (Authorization: Bearer …)")]
    fn fetch(token: &AccessToken) -> ResourceData {
        if token.0.is_empty() {
            ResourceServer::log_missing_token();
        }
        ResourceData("the answer is 42".into())
    }

    #[seq::msg]
    #[seq::label("log: missing bearer token")]
    fn log_missing_token() {}
}

// ---------------------------------------------------------------------------
// Sibling modules that both declare `Frontend` + `Backend` participants —
// exercises scope-aware entry resolution. Without it, both diagrams would
// resolve to whichever `Frontend::start` rustc enumerated first.
// ---------------------------------------------------------------------------

mod alpha_protocol {
    use rust2seq as seq;

    seq::diagram! {
        name = "alpha_protocol",
        title = "Alpha protocol",
        entry = Frontend::start,
    }

    // A second diagram in the same module that qualifies across module
    // boundaries with `crate::…`. Must resolve to the beta module's
    // `Frontend`, not the local alpha one.
    seq::diagram! {
        name = "cross_module_qualified",
        title = "Cross-module qualified entry",
        entry = crate::beta_protocol::Frontend::start,
    }

    #[seq::participant]
    pub struct Frontend;

    #[seq::participant]
    pub struct Backend;

    impl Frontend {
        #[seq::msg]
        pub fn start() {
            Backend::handle_alpha();
        }
    }

    impl Backend {
        #[seq::msg]
        fn handle_alpha() {}
    }
}

mod beta_protocol {
    use rust2seq as seq;

    seq::diagram! {
        name = "beta_protocol",
        title = "Beta protocol",
        entry = Frontend::start,
    }

    #[seq::participant]
    pub struct Frontend;

    #[seq::participant]
    pub struct Backend;

    impl Frontend {
        #[seq::msg]
        pub fn start() {
            Backend::handle_beta();
        }
    }

    impl Backend {
        #[seq::msg]
        fn handle_beta() {}
    }
}
