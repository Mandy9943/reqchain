# reqchain

An HTTP client for APIs whose credentials are themselves derived from a request: a token
fetched from another endpoint, or a header hashed from the current date. Endpoints are
described in JSON files; `reqchain` resolves the auth chain, caches the token, and sends
the request.

The file format is documented in [SPEC.md](SPEC.md). That document is self-contained and
is what you should point an AI agent at when asking it to write an API file.

## Prerequisites

- **Rust, stable toolchain**, via [rustup](https://rustup.rs):
  ```sh
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
  The repo pins `channel = "stable"` in `rust-toolchain.toml`; rustup picks it up
  automatically. Edition 2021.
- **A C toolchain and linker.** On Debian/Ubuntu:
  ```sh
  sudo apt install build-essential
  ```
  Nothing else is needed: the HTTP stack uses rustls, not system OpenSSL, so there is no
  `libssl-dev` or `pkg-config` dependency.

## Build

```sh
cargo build --release
```

The binary lands at `target/release/reqchain`. Put it on your `PATH`, or run it in place:

```sh
./target/release/reqchain --help
```

To run the test suite:

```sh
cargo test --workspace
```

## Directory layout

Everything `reqchain` reads and writes lives under `$REQCHAIN_DIR`, which defaults to
`~/.config/reqchain`. Set the variable to point at a different workspace.

```
$REQCHAIN_DIR/
├── workspace/
│   └── apis/
│       ├── ceibal-gateway-test.json    one API per file, named after its `id`
│       └── odilo.json
├── secrets.json                        flat {"NAME": "value"} map, mode 0600
├── cache/
│   └── tokens.json                     chained-auth token cache, managed by reqchain
└── history/
```

Create it:

```sh
mkdir -p ~/.config/reqchain/workspace/apis ~/.config/reqchain/cache ~/.config/reqchain/history
```

## The secret store

API files must never contain a credential. They reference one as `{{secret:NAME}}`, and
the value lives in `$REQCHAIN_DIR/secrets.json` — a flat JSON object mapping name to
string value. Create it with owner-only permissions:

```sh
umask 077
cat > ~/.config/reqchain/secrets.json <<'EOF'
{
  "GW_USER": "replace-me",
  "GW_PASS": "replace-me",
  "API_TOKEN": "replace-me"
}
EOF
chmod 600 ~/.config/reqchain/secrets.json
```

Check it:

```sh
ls -l ~/.config/reqchain/secrets.json   # expect -rw-------
```

A missing store is not an error — a workspace with no secrets is valid — but a file that
references a secret the store does not define fails at run time.

Secret values, and any token derived by chained auth, are masked as `***` in printed
requests, in `--print-command` output and in error messages.

## Running the worked examples

The repository ships the two examples from SPEC.md in `tests/fixtures`. Copy them into a
workspace and try them:

```sh
cp tests/fixtures/ceibal-gateway-test.json tests/fixtures/odilo.json \
   ~/.config/reqchain/workspace/apis/

reqchain validate
reqchain list
```

`validate` with no arguments checks every file in the workspace; pass paths to check
specific files:

```sh
reqchain validate tests/fixtures/odilo.json
```

Run an endpoint, selecting an environment:

```sh
reqchain run ceibal-gateway-test consultaReparacion --env test
```

`reqchain` calls the `token` endpoint first, caches the token for its `expires_in`, injects
it as `Authorization: Bearer …`, and sends the business request. The status line and the
auth trace go to stderr, the response body to stdout.

To see the request without sending it:

```sh
reqchain run odilo odilo-user --env test --print-command
```

Both examples point at Ceibal test hosts and need real credentials in the secret store to
return anything; `validate` and `list` work without network access.

`--print-command` never sends the endpoint's own request — a `DELETE` endpoint printed this
way is not deleted. It does still call the **auth** endpoint when the endpoint uses chained
auth, because the token does not exist until that call has been made; so for a chained
endpoint it needs to reach the auth host, and for every other endpoint it needs no network
access at all.

## Commands

| Command | Purpose |
| --- | --- |
| `reqchain list` | List every API in the workspace and its endpoints. |
| `reqchain run <api> <endpoint> [--env NAME] [--print-command]` | Run an endpoint, resolving its auth chain. With `--print-command`, print the request as `curl` instead of sending it (a chained auth still fetches its token). |
| `reqchain validate [files...]` | Validate API files; defaults to the whole workspace. |

Exit codes:

| Code | `run` | `validate` / `list` |
| --- | --- | --- |
| 0 | Request completed (any HTTP status). | No errors. |
| 1 | Configuration, auth-chain or response-handling failure. | A file had an error or could not be read. |
| 2 | Transport failure (DNS, connection, TLS, timeout). | — |
| 3 | API or endpoint id not found. | — |

## Writing an API file

Read [SPEC.md](SPEC.md). In short: one API per file, file name matching `id`,
`schemaVersion: 1`, 2-space indent, trailing newline, credentials only as
`{{secret:NAME}}`, and `reqchain validate <file>` must report `ok` before you use it.
