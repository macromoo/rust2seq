# rust2seq-example

A worked example of `rust2seq` modeling the **OAuth 2.0 Authorization Code grant with PKCE**.

The diagram you see in `diagrams/oauth_authorization_code.puml` was **derived from the Rust code** in `src/lib.rs`.

`User::click_login_button`'s body actually calls `Browser::handle_login`, which actually calls `AuthServer::authorize`, which actually calls `AuthServer::exchange`, etc. The rust2seq driver walks the typed AST after the Rust compiler finishes analysis and reads the structure directly.

## Layout

- `src/lib.rs` — annotated Rust with real method bodies that call across `#[seq::principal]` types
- `diagrams/oauth_authorization_code.puml` — generated + committed; acts as a regression fixture

## Regenerate

Install the driver once:

```sh
# from clients/
cargo install --path rust2seq/rust2seq-driver --debug --locked
```

Then any time you want to regenerate (whether or not source changed):

```sh
# from this directory
cargo rust2seq
```

The driver wipes its target-dir fingerprints on each invocation, so every run actually re-typechecks + re-walks. No more "no source change, cargo skipped rustc" gotchas.

## Render

Open `diagrams/oauth_authorization_code.puml` in any PlantUML viewer (the VS Code [PlantUML extension](https://marketplace.visualstudio.com/items?itemName=jebbs.plantuml), [plantuml.com](https://plantuml.com), etc).
