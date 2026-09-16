# reqchain API file format — version 1

## 1. What this format is

A reqchain API file is a single JSON document describing one API: its base URL, its
variables, its environments, its auth, and its endpoints. `reqchain` reads these files
from `$REQCHAIN_DIR/workspace/apis/<id>.json`, where `$REQCHAIN_DIR` defaults to
`~/.config/reqchain`. Every file in that directory is loaded at startup; the `id` field
is how the CLI addresses the API (`reqchain run <id> <endpoint-id>`). The format's
distinguishing feature is *chained auth*: an endpoint may declare that its credential is
obtained by calling another endpoint in the same file first.

## 2. Rules

1. One API per file. A file contains exactly one top-level object, never an array.
2. Name the file after the `id` field: `id: "odilo"` lives in `odilo.json`. This is a
   convention, not a checked rule — every `*.json` file in the directory is loaded and
   indexed by its `id` field, and `validate` does not look at the file name.
3. `schemaVersion` must be the integer `1`. Any other value is rejected at load time.
4. Unknown fields are rejected everywhere. The format is closed: a typo in a field name
   is an error, not a silently ignored key.
5. Indent with 2 spaces. End the file with a single trailing newline.
6. Never write a credential into the file. Reference it as `{{secret:NAME}}` and put the
   value in `$REQCHAIN_DIR/secrets.json`, which is a flat JSON object of string to string.
7. Run `reqchain validate <file>` after writing the file. Do not consider the file
   finished until it reports `ok`.

## 3. Complete example 1 — chained auth

`tests/fixtures/ceibal-gateway-test.json`. An OAuth2-style gateway: a `token` endpoint
authenticated with HTTP Basic against two secrets, and a business endpoint whose bearer
token is fetched from it.

```json
{
  "schemaVersion": 1,
  "id": "ceibal-gateway-test",
  "name": "Ceibal Gateway (test)",
  "baseUrl": "https://api-manager.test-ceibal.edu.uy",
  "variables": {},
  "environments": [
    {
      "name": "test",
      "variables": {
        "documento": "12345678"
      }
    }
  ],
  "auth": {
    "type": "none"
  },
  "endpoints": [
    {
      "id": "token",
      "name": "Token",
      "method": "POST",
      "path": "/token",
      "headers": {},
      "query": {},
      "variables": {},
      "auth": {
        "type": "basic",
        "username": "{{secret:GW_USER}}",
        "password": "{{secret:GW_PASS}}"
      },
      "body": {
        "type": "form",
        "fields": {
          "grant_type": "client_credentials"
        }
      }
    },
    {
      "id": "consultaReparacion",
      "name": "Consulta Reparacion",
      "method": "POST",
      "path": "/consultareparacion/1.0",
      "headers": {},
      "query": {},
      "variables": {},
      "auth": {
        "type": "chained",
        "source": {
          "endpoint": "token"
        },
        "extract": {
          "from": "body",
          "jsonPath": "$.access_token"
        },
        "ttl": {
          "from": "body",
          "jsonPath": "$.expires_in",
          "unit": "seconds"
        },
        "inject": {
          "into": "header",
          "name": "Authorization",
          "template": "Bearer {{value}}"
        },
        "retryOn": [
          401,
          403
        ]
      },
      "body": {
        "type": "json",
        "content": {
          "SDTConsultaReparacion_In": {
            "PersonaDocumento": "{{documento}}"
          }
        }
      }
    }
  ]
}
```

Block by block:

| Block | What it does |
| --- | --- |
| `schemaVersion`, `id`, `name`, `baseUrl` | The four required identity fields. `baseUrl` is prefixed to every endpoint `path`. |
| `variables: {}` | No API-level variables; the object may be omitted entirely. |
| `environments` | One environment, `test`, defining `documento`. Selected with `reqchain run ... --env test`. |
| `auth: {"type": "none"}` | API-level default. Endpoints that say nothing inherit it. |
| `endpoints[0]` (`token`) | `POST /token` with a URL-encoded form body, authenticated with HTTP Basic from two secrets. This endpoint exists only to produce a token. |
| `endpoints[1].auth` | Chained auth: call `token`, take `$.access_token` from its JSON body, cache it for `$.expires_in` seconds, inject it as `Authorization: Bearer <token>`, and on a 401 or 403 refetch once and retry. |
| `endpoints[1].body` | A JSON body; `{{documento}}` resolves from the active environment. |

## 4. Complete example 2 — computed header

`tests/fixtures/odilo.json`. No token endpoint: the credential is a hash computed at
request time from the current date and a variable, sent alongside a static secret header.

```json
{
  "schemaVersion": 1,
  "id": "odilo",
  "name": "Odilo (test)",
  "baseUrl": "https://api.test-ceibal.edu.uy",
  "variables": {},
  "environments": [
    {
      "name": "test",
      "variables": {
        "documento": "12345678"
      }
    }
  ],
  "auth": {
    "type": "none"
  },
  "endpoints": [
    {
      "id": "odilo-user",
      "name": "Odilo User",
      "method": "GET",
      "path": "/odilo/user",
      "headers": {
        "token": "{{secret:API_TOKEN}}",
        "user": "{{documento}}"
      },
      "query": {},
      "variables": {},
      "auth": {
        "type": "computed",
        "name": "hash",
        "expression": "md5(now(\"YYYYMMDD\") + {{documento}})"
      }
    }
  ]
}
```

The `computed` auth sets one header, `hash`, to the MD5 of today's date in `YYYYMMDD`
form concatenated with `documento`. The `token` and `user` headers are ordinary headers:
`token` comes from the secret store, `user` from the environment. The endpoint has no
`body`, so the field is omitted.

## 5. Field reference

### API (top-level object)

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `schemaVersion` | integer | yes | Must be `1`. |
| `id` | string | yes | Stable identifier for the API; must equal the file's base name. |
| `name` | string | yes | Human-readable display name. |
| `baseUrl` | string | yes | URL prefix prepended to every endpoint `path`. May contain `{{variable}}`. |
| `variables` | object of string to string | no (default `{}`) | API-level variables. Lowest precedence. |
| `environments` | array of Environment | no (default `[]`) | Named variable-override sets. |
| `auth` | Auth | no (default `{"type": "none"}`) | Used by any endpoint whose own auth is `inherit`. |
| `endpoints` | array of Endpoint | yes | The endpoints. Ids must be unique; a duplicate is an error. |

### Environment

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | string | yes | Environment name, selected with `--env <name>`. |
| `variables` | object of string to string | no (default `{}`) | Overrides for this environment. Middle precedence. |

### Endpoint

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `id` | string | yes | Stable identifier, unique within the API. |
| `name` | string | yes | Human-readable display name. |
| `method` | enum | yes | One of `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`. Uppercase. |
| `path` | string | yes | Appended to `baseUrl`. May contain `{{variable}}`. |
| `headers` | object of string to string | no (default `{}`) | Request headers. Values may contain `{{variable}}`. |
| `query` | object of string to string | no (default `{}`) | Query string parameters. Values may contain `{{variable}}`. |
| `variables` | object of string to string | no (default `{}`) | Endpoint-level variables. Highest precedence. |
| `auth` | Auth | no (default `{"type": "inherit"}`) | This endpoint's auth. |
| `body` | Body | no | The request body. Omit the field entirely when there is none. |

### Body variants (discriminated by `type`)

| `type` | Field | Type | Required | Meaning |
| --- | --- | --- | --- | --- |
| `json` | `content` | any JSON value | yes | Sent as the JSON request body. |
| `form` | `fields` | object of string to string | yes | URL-encoded form fields. |
| `multipart` | `fields` | object of string to string | yes | Text parts of a `multipart/form-data` body. |
| `multipart` | `files` | object of string to string | no (default `{}`) | File parts; each value is a filesystem path. |
| `text` | `content` | string | yes | Sent as a plain-text body. |
| `xml` | `content` | string | yes | Sent as an XML body. |
| `binary` | `path` | string | yes | Filesystem path whose bytes are the body. |

### Auth variants (discriminated by `type`)

| `type` | Field | Type | Required | Meaning |
| --- | --- | --- | --- | --- |
| `inherit` | — | — | — | Use the API-level auth. An API whose own auth is `inherit` behaves as `none`. |
| `none` | — | — | — | Send no auth. |
| `basic` | `username` | string | yes | Username for HTTP Basic. |
| `basic` | `password` | string | yes | Password for HTTP Basic. Use `{{secret:NAME}}`. |
| `bearer` | `token` | string | yes | Sent as `Authorization: Bearer <token>`. |
| `header` | `headers` | object of string to string | yes | Static auth headers, set on the request. |
| `computed` | `name` | string | yes | Name of the header to set with the computed value. |
| `computed` | `expression` | string | yes | Expression evaluated at request time (see §7). |
| `chained` | `source` | ChainSource | yes | The endpoint to call to obtain the token. |
| `chained` | `extract` | Extract | no (default `{"from": "body", "jsonPath": "$.access_token"}`) | How to pull the token out of the source response. |
| `chained` | `ttl` | Ttl | no (default `{"from": "body", "jsonPath": "$.expires_in", "unit": "seconds"}`) | How to determine the token's lifetime. |
| `chained` | `inject` | Inject | yes | Where the token goes in this endpoint's request. |
| `chained` | `retryOn` | array of integer 100–599 | no (default `[401, 403]`) | Statuses that trigger one refresh and retry. |

### ChainSource

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `endpoint` | string | yes | The `id` of an endpoint in this same file. |

### Extract (discriminated by `from`)

| `from` | Field | Type | Required | Meaning |
| --- | --- | --- | --- | --- |
| `body` | `jsonPath` | string | no (default `$.access_token`) | JSONPath selecting the token in the JSON response body. |
| `body` | `regex` | string | no | Regex over the raw body; capture group 1 is the token, else the whole match. Takes precedence over `jsonPath`. |
| `body` | `xpath` | string | no | **Not implemented.** Accepted by the schema but rejected by the validator and the runtime. Use `jsonPath` or `regex`. |
| `header` | `name` | string | yes | Response header holding the token. Matched case-insensitively. |
| `header` | `regex` | string | no | Regex over the header value; capture group 1 is the token, else the whole match. |
| `status` | — | — | — | The response's HTTP status, as a string, is the token. |

### Ttl (discriminated by `from`)

| `from` | Field | Type | Required | Meaning |
| --- | --- | --- | --- | --- |
| `body` | `jsonPath` | string | yes | JSONPath selecting a relative lifetime in the response body. |
| `body` | `unit` | string | no (default `seconds`) | `seconds`, or `milliseconds` to divide the value by 1000. Any other value is read as seconds. |
| `fixed` | `seconds` | integer ≥ 0 | yes | The token is valid for this many seconds from the moment it was fetched. |
| `absolute` | `jsonPath` | string | yes | JSONPath selecting an RFC 3339 timestamp at which the token expires. A value that is not RFC 3339 is a run-time error. |

### Inject (discriminated by `into`)

| `into` | Field | Type | Required | Meaning |
| --- | --- | --- | --- | --- |
| `header` | `name` | string | yes | Header to set. Any existing header of that name is replaced. |
| `header` | `template` | string | yes | Value template; `{{value}}` is replaced by the token. |
| `query` | `name` | string | yes | Query parameter to append. |
| `query` | `template` | string | no (default `{{value}}`) | Value template. |
| `body` | `pointer` | string | yes | JSON pointer (e.g. `/auth/token`) into the request body. The pointer must already match an existing location, or the run fails. |
| `body` | `template` | string | no (default `{{value}}`) | Value template. |

## 6. Variables

`{{name}}` in any of `baseUrl`, `path`, header values, query values, body content, form or
multipart field values, a binary body path, or an auth field is replaced before the
request is sent.

Precedence, most specific first:

1. the endpoint's `variables`
2. the active environment's `variables` (only the one named by `--env`)
3. the API's `variables`

The first scope that defines the name wins. Rules:

- A reference to a name no scope defines is an **error**, raised before anything is sent.
  No partial request goes out.
- `{{secret:NAME}}` resolves **only** from the secret store at
  `$REQCHAIN_DIR/secrets.json`. It never falls back to a variable, and a variable named
  `secret:NAME` cannot shadow it. A missing secret is an error.
- `{{value}}` is the token placeholder inside a chained-auth `inject` template **only**,
  and is substituted there without going through variable lookup. Anywhere else it is an
  ordinary variable reference: the validator accepts it, but the run fails unless a
  variable named `value` is actually defined.
- Whitespace inside the braces is trimmed: `{{ documento }}` and `{{documento}}` are the
  same reference.
- An unterminated `{{` is left in the text verbatim rather than being treated as a
  reference.

Validator caveat: `reqchain validate` accepts a variable that is defined in **any**
environment, not only the one you will run with. A file can therefore validate cleanly and
still fail at run time under a different `--env`. Define a variable in every environment,
or at API level, if every environment needs it.

## 7. Expression language

Used only in `auth.type: "computed"`, in the `expression` field.

```
expr    := term ( "+" term )*
term    := string | varref | funcall
string  := '"' <any characters except '"'> '"'
varref  := "{{" name "}}"          ; an ordinary variable or {{secret:NAME}}
funcall := name "(" [ expr ( "," expr )* ] ")"
```

- Every value is a string. `+` is string concatenation, never arithmetic.
- Whitespace between tokens is insignificant.
- String literals have no escape sequences: a `"` always ends the literal.
- Nesting deeper than 32 levels is an error.
- Trailing input after a complete expression is a syntax error.

The five functions are the only ones permitted. Each takes exactly one argument; any other
arity is an error. Any other function name is an error — reported by the validator before
the request is built, and again at run time.

| Function | Result |
| --- | --- |
| `md5(s)` | MD5 of `s`, lowercase hex. |
| `sha1(s)` | SHA-1 of `s`, lowercase hex. |
| `sha256(s)` | SHA-256 of `s`, lowercase hex. |
| `base64(s)` | Standard base64 (with padding) of `s`. |
| `now(fmt)` | The current local time formatted with `fmt`. |

`now` format tokens; every other character is emitted literally.

| Token | Meaning |
| --- | --- |
| `YYYY` | 4-digit year |
| `MM` | 2-digit month |
| `DD` | 2-digit day of month |
| `HH` | 2-digit hour, 24-hour clock |
| `mm` | 2-digit minute |
| `ss` | 2-digit second |

Example: `md5(now("YYYYMMDD") + {{documento}})` — see §4.

## 8. Chained auth

When an endpoint's resolved auth is `chained`, running it does this:

1. Build the endpoint's own request and apply any static auth to it.
2. Obtain a token:
   - If a cached token for this API, this auth configuration and this environment is
     present and unexpired, use it.
   - Otherwise run the endpoint named by `source.endpoint`, in the same environment. That
     endpoint may itself have chained auth; the chain is walked recursively.
3. Unless `extract` is `{"from": "status"}`, a non-2xx response from the source endpoint is
   an **error**: the run stops and reports the status and an excerpt of the body. It is
   never treated as "no token".
4. Apply `extract` to the source response to get the token string.
5. Apply `ttl` to compute an expiry and cache the token. **If the expiry cannot be
   determined — the body is not JSON, the `jsonPath` matches nothing, or the matched value
   is not a number — the token is not cached at all** and is refetched on the next run.
   A cached token is treated as expired **30 seconds before** its computed expiry, so that
   a token does not go stale in flight. A `ttl` of 30 seconds or less therefore never
   produces a cache hit; every run refetches.
6. Inject the token into the original request per `inject` and send it.
7. If the response status is in `retryOn` (default `[401, 403]`), invalidate the cache
   entry, fetch a fresh token and send the request **exactly once more**. The second
   response is final, whatever its status. There is no second retry.

Defaults and their risk: omitting `extract` means `$.access_token` and omitting `ttl`
means `$.expires_in` in seconds. Those shapes fit an OAuth2 token response and little
else. The validator emits a **warning** — not an error — for each omitted field; a file
that relies on the defaults still validates. Set both explicitly unless the source
endpoint really is OAuth2-shaped.

Limits:

- **Depth limit 5.** A chain that walks more than 5 endpoints is an error. Both the
  validator and the runtime enforce it.
- **Cycles are detected.** `a` needing `b` which needs `a` is reported as
  `auth cycle detected: a -> b -> a`, by the validator and again at run time. Neither
  hangs nor recurses forever.
- The source endpoint must exist in the same file. Cross-file chains are not supported.
- Tokens derived from a chain are masked as `***` in printed requests, traces and errors,
  exactly like secrets.

## 9. Validating

```
reqchain validate <file>...
```

With no arguments it validates every `*.json` file in `$REQCHAIN_DIR/workspace/apis`.
Each diagnostic is printed as `<file>: <error|warning>: <message> at <json-path>`, and a
file with no errors additionally prints `<file>: ok`.

| Exit code | Meaning |
| --- | --- |
| 0 | No file had an error. Warnings alone do not fail. |
| 1 | At least one file had an error, or could not be read. |

Sample warning (the file still passes):

```
apis/gw.json: warning: endpoint `business` relies on the default extract $.access_token — set auth.extract explicitly if this API returns a different field at $.endpoints[1].auth
apis/gw.json: ok
```

Sample error (the file fails):

```
apis/gw.json: error: chained auth of `business` points at endpoint `nonexistent`, which does not exist in this API at $.endpoints[0].auth.source.endpoint
```

What is an **error**: malformed JSON, an unknown field, a `schemaVersion` other than 1, a
duplicate endpoint id, a reference to an undefined variable, an unknown function in a
computed expression, a chained `source.endpoint` that does not exist, an invalid
`jsonPath` in `auth.extract` (a `ttl` jsonPath is only checked at run time), an `xpath`
extract, an auth cycle, and a chain deeper than 5.

What is a **warning**: relying on the default chained-auth `extract`, and relying on the
default chained-auth `ttl`.

Whether a `{{secret:NAME}}` actually exists in the store is a run-time concern, not a file
concern: the validator only checks that the name is non-empty.
