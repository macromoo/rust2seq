//! Proc-macro shims for `rust2seq`.

use proc_macro::TokenStream;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Marks a struct, enum, or union as a sequence-diagram participant — one
/// vertical lane on the generated diagram.
///
/// Required on every Rust type that should appear on the diagram. The walker
/// discovers participants by traversing annotated calls — any type whose
/// `impl` block hosts a `#[seq::msg]` fn that gets reached becomes a
/// participant.
///
/// Optional `display = "..."` arg sets a custom display label (supports
/// multi-line via `\n`). Defaults to the type's name.
///
/// ```ignore
/// #[seq::participant]
/// pub struct Browser;
///
/// #[seq::participant(display = "Authorization Server\n(OAuth provider)")]
/// pub struct AuthServer;
/// ```
#[proc_macro_attribute]
pub fn participant(attr: TokenStream, item: TokenStream) -> TokenStream {
    let payload = attr.to_string();
    emit_marker("participant", &payload, item)
}

/// Marks a fn or method as a step that appears on the diagram whenever it's
/// reached from a diagram's entry.
///
/// Takes no args — the source ("from") participant is *always* inferred from
/// the owning `impl` block's type, and the destination ("to") is the type
/// hosting this fn. No knob to override; if you want a different originator,
/// move the call into a different participant's `impl`.
///
/// ```ignore
/// impl Browser {
///     #[seq::msg]
///     fn handle_login(&mut self) { /* ... */ }
/// }
/// ```
#[proc_macro_attribute]
pub fn msg(attr: TokenStream, item: TokenStream) -> TokenStream {
    let payload = attr.to_string();
    emit_marker("msg", &payload, item)
}

/// Overrides the auto-derived diagram label for a `#[seq::msg]` fn.
///
/// ```ignore
/// #[seq::msg]
/// #[seq::label("POST /token { code, code_verifier, ... }")]
/// fn exchange_code_for_token(&self, /* ... */) -> Tokens { /* ... */ }
/// ```
#[proc_macro_attribute]
pub fn label(attr: TokenStream, item: TokenStream) -> TokenStream {
    let payload = attr.to_string();
    emit_marker("label", &payload, item)
}

/// Declares one sequence diagram. Expands to a hidden const carrying the
/// original args as a string, which the driver discovers + parses
/// post-expansion.
///
/// Required args: `name`, `entry`. Optional: `title`, `output`.
///
/// Output path defaults to `diagrams/<name>.puml` (relative to the crate
/// root). Override with `output = "some/other/path.puml"` if needed.
///
/// No `participants` arg — the driver discovers them by walking the call
/// graph from `entry`. Participant order is first-appearance during the walk.
///
/// ```ignore
/// rust2seq::diagram! {
///     name = "private_session",
///     title = "Private session",
///     entry = Dashboard::start,
/// }
/// ```
#[proc_macro]
pub fn diagram(input: TokenStream) -> TokenStream {
    let id = next_id();
    let body = input.to_string();
    // The const sits at module scope. Name encodes a counter so multiple
    // diagram! invocations in the same module don't collide.
    let code = format!(
        "#[doc(hidden)]\n\
         #[allow(non_upper_case_globals, dead_code)]\n\
         const __RUST2SEQ_DIAGRAM_{id}: &str = {body_lit};\n",
        id = id,
        body_lit = escape_str_literal(&body),
    );
    code.parse()
        .expect("rust2seq: failed to synthesize diagram! marker const")
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn emit_marker(kind: &str, payload: &str, item: TokenStream) -> TokenStream {
    // Format: `__rust2seq_marker__:<kind>:<payload>`
    // Multiple markers on one item are fine — each one is its own `#[doc]`.
    let marker = format!("__rust2seq_marker__:{kind}:{payload}");
    let attr: TokenStream = format!("#[doc = {}]", escape_str_literal(&marker))
        .parse()
        .expect("rust2seq: failed to synthesize marker doc attr");
    let mut out = attr;
    out.extend(item);
    out
}

/// Render `s` as a valid Rust string literal — `"..."` with embedded `"` and
/// `\` properly escaped, and any other awkward chars passed through verbatim
/// (proc-macro2 will Unicode-validate the rest).
fn escape_str_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

static NEXT_DIAGRAM_ID: AtomicUsize = AtomicUsize::new(0);

fn next_id() -> usize {
    NEXT_DIAGRAM_ID.fetch_add(1, Ordering::Relaxed)
}
