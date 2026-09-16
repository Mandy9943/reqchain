use reqchain_core::exec::{AuthStep, RunResult};
use reqchain_core::request::{redact_display, EffectiveBody, EffectiveRequest};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDto {
    pub apis: Vec<ApiDto>,
    pub errors: Vec<FileErrorDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiDto {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub environments: Vec<String>,
    pub endpoints: Vec<EndpointDto>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointDto {
    pub id: String,
    pub name: String,
    pub method: String,
    pub path: String,
    pub auth_kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileErrorDto {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticDto {
    pub severity: String,
    pub path: String,
    pub message: String,
}

impl From<&reqchain_core::validate::Diagnostic> for DiagnosticDto {
    fn from(d: &reqchain_core::validate::Diagnostic) -> DiagnosticDto {
        DiagnosticDto {
            severity: match d.severity {
                reqchain_core::validate::Severity::Error => "error",
                reqchain_core::validate::Severity::Warning => "warning",
            }
            .into(),
            path: d.path.clone(),
            message: d.message.clone(),
        }
    }
}

impl From<&reqchain_core::store::FileError> for FileErrorDto {
    fn from(e: &reqchain_core::store::FileError) -> FileErrorDto {
        FileErrorDto {
            path: e.path.display().to_string(),
            message: e.message.clone(),
        }
    }
}

fn auth_kind(auth: &reqchain_core::model::Auth) -> &'static str {
    use reqchain_core::model::Auth;
    match auth {
        Auth::Inherit => "inherit",
        Auth::None => "none",
        Auth::Basic { .. } => "basic",
        Auth::Bearer { .. } => "bearer",
        Auth::Header { .. } => "header",
        Auth::Computed { .. } => "computed",
        Auth::Chained { .. } => "chained",
    }
}

impl EndpointDto {
    /// `auth_kind` reports the auth that will actually be USED, not the one the
    /// endpoint declares: `auth::resolve` folds `inherit` into the API-level
    /// auth. An endpoint that inherits a chained API auth would otherwise report
    /// `inherit` and get no `chain` marker in the sidebar — and that marker is
    /// the one signal this whole product exists to surface.
    pub fn from_endpoint(
        api: &reqchain_core::model::Api,
        ep: &reqchain_core::model::Endpoint,
    ) -> EndpointDto {
        EndpointDto {
            id: ep.id.clone(),
            name: ep.name.clone(),
            method: ep.method.as_str().to_string(),
            path: ep.path.clone(),
            auth_kind: auth_kind(reqchain_core::auth::resolve(api, ep)).to_string(),
        }
    }
}

/// The result of running an endpoint, fully masked: nothing in here may carry
/// an unredacted secret or chain-derived token. Build it only through
/// [`RunDto::from_result`], which is the one place that applies the mask.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDto {
    pub status: u16,
    pub elapsed_ms: u64,
    pub size_bytes: usize,
    pub headers: Vec<[String; 2]>,
    pub body: String,
    pub body_is_binary: bool,
    /// The body was longer than `MAX_BODY_CHARS` and `body` holds only its
    /// first `MAX_BODY_CHARS` characters. `size_bytes` still reports the real
    /// size, so the UI can say how much was withheld.
    pub body_truncated: bool,
    pub effective: EffectiveDto,
    pub auth_trace: Vec<AuthStepDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveDto {
    pub method: String,
    pub url: String,
    pub headers: Vec<[String; 2]>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStepDto {
    pub endpoint_id: String,
    pub request: Option<EffectiveDto>,
    /// `None` for a cache hit. The executor reports a cache hit as
    /// `status: 0` internally — a sentinel, not a real HTTP status — so it is
    /// never forwarded as a bare number here. The frontend should render
    /// "from cache" (driven by `from_cache`) rather than a status when this
    /// is `None`.
    pub status: Option<u16>,
    pub body: String,
    pub from_cache: bool,
}

/// Renders an [`EffectiveBody`] for display/curl purposes. Called only after
/// the body has already been through [`EffectiveRequest::masked_with`], so
/// any `Text`/`Form` content here is already redacted.
fn effective_body_text(body: &EffectiveBody) -> Option<String> {
    Some(match body {
        EffectiveBody::Text { content, .. } => content.clone(),
        EffectiveBody::Form { fields } => fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&"),
        EffectiveBody::Multipart { fields, files } => format!(
            "<multipart: {} field(s), {} file(s)>",
            fields.len(),
            files.len()
        ),
        EffectiveBody::Binary { path } => format!("<binary file: {path}>"),
    })
}

impl EffectiveDto {
    /// The only constructor. Deliberately not a public `From<&EffectiveRequest>`
    /// impl (any external caller could build a DTO straight from an
    /// unmasked request with nothing but a doc comment stopping them) and
    /// deliberately not exported past this crate: every call site that
    /// reaches this function lives in `dto.rs` or `commands.rs`, and every
    /// one of them passes a request that has already been through
    /// `masked_with`.
    pub(crate) fn from_masked(req: &EffectiveRequest) -> EffectiveDto {
        EffectiveDto {
            method: req.method.as_str().to_string(),
            url: req.url.clone(),
            headers: req
                .headers
                .iter()
                .map(|(k, v)| [k.clone(), v.clone()])
                .collect(),
            body: req.body.as_ref().and_then(effective_body_text),
        }
    }
}

impl AuthStepDto {
    /// `step.request`, when present, is the raw request the executor sent to
    /// the auth endpoint — not masked yet — so it is masked here with
    /// `request_mask`/`auth_headers`, the same pair every REQUEST crossing
    /// into the DTO layer goes through (the masking contract in the task
    /// brief). `step.body` is the auth endpoint's raw RESPONSE body, which
    /// can hold the token verbatim, so it goes through `response_mask`
    /// instead — the broader list that also covers static auth credential
    /// values a server might echo back (see `commands::response_mask`).
    fn from_step(
        step: &AuthStep,
        request_mask: &[String],
        response_mask: &[String],
        auth_headers: &[String],
    ) -> AuthStepDto {
        let masked_request = step
            .request
            .as_ref()
            .map(|r| r.masked_with(request_mask, auth_headers));
        AuthStepDto {
            endpoint_id: step.endpoint_id.clone(),
            request: masked_request.as_ref().map(EffectiveDto::from_masked),
            status: if step.from_cache {
                None
            } else {
                Some(step.status)
            },
            body: redact_display(&step.body, response_mask),
            from_cache: step.from_cache,
        }
    }
}

impl RunDto {
    /// Builds the DTO for a finished run.
    ///
    /// `request_mask` masks anything shaped like a REQUEST we built — the
    /// effective request and each auth-trace request — and must be the
    /// executor's derived tokens plus every secret value
    /// (`executor.derived_values()` extended with `secrets.values()`), the
    /// same list `auth::apply_static`'s by-name masking deliberately keeps
    /// static auth credential values out of.
    ///
    /// `response_mask` masks anything a SERVER sent back — the response
    /// body, its headers, and each auth-trace body — and must additionally
    /// include `executor.static_values()`: a server can echo a static
    /// credential (a Basic blob, a computed header) verbatim, and that is
    /// not covered by by-name masking, which only touches the request we
    /// sent. `auth_headers` must be `executor.auth_headers()`.
    ///
    /// This is the only place a `RunDto` may be built from a raw
    /// `RunResult` — every string that could carry a credential is masked
    /// or redacted here.
    pub fn from_result(
        result: &RunResult,
        request_mask: &[String],
        response_mask: &[String],
        auth_headers: &[String],
    ) -> RunDto {
        let masked_effective = result.effective.masked_with(request_mask, auth_headers);
        let (body, body_is_binary, body_truncated) = render_body(result, response_mask);
        RunDto {
            status: result.status,
            elapsed_ms: result.elapsed_ms.min(u128::from(u64::MAX)) as u64,
            size_bytes: result.size_bytes,
            headers: result
                .headers
                .iter()
                .map(|(k, v)| [k.clone(), redact_display(v, response_mask)])
                .collect(),
            body,
            body_is_binary,
            body_truncated,
            effective: EffectiveDto::from_masked(&masked_effective),
            auth_trace: result
                .auth_trace
                .iter()
                .map(|s| AuthStepDto::from_step(s, request_mask, response_mask, auth_headers))
                .collect(),
        }
    }
}

/// Above this, a response body is handed to the webview truncated. A single
/// `<pre>` holding a 50 MB payload locks up the window, and nobody reads that
/// far; `size_bytes` still carries the true size.
pub const MAX_BODY_CHARS: usize = 1_048_576;

/// Content-type families that are binary regardless of whether the bytes
/// happen to be valid UTF-8 — a small SVG or a JSON-shaped font manifest would
/// otherwise be dumped into the body pane as text.
fn is_binary_content_type(headers: &[(String, String)]) -> bool {
    let Some((_, value)) = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
    else {
        return false;
    };
    let value = value.to_ascii_lowercase();
    let essence = value.split(';').next().unwrap_or("").trim();
    essence.starts_with("image/")
        || essence.starts_with("audio/")
        || essence.starts_with("video/")
        || essence.starts_with("font/")
        || matches!(
            essence,
            "application/octet-stream"
                | "application/pdf"
                | "application/zip"
                | "application/gzip"
                | "application/x-tar"
                | "application/wasm"
        )
}

/// Decides how a response body reaches the webview: `(text, is_binary, truncated)`.
/// Binary bodies are reported by type and size rather than rendered (spec §8),
/// and a text body is redacted before it is truncated, never after — cutting
/// first could leave half a credential in view.
fn render_body(result: &RunResult, response_mask: &[String]) -> (String, bool, bool) {
    if is_binary_content_type(&result.headers) {
        return (String::new(), true, false);
    }
    let Ok(text) = std::str::from_utf8(&result.body) else {
        return (String::new(), true, false);
    };
    let redacted = redact_display(text, response_mask);
    if redacted.chars().count() <= MAX_BODY_CHARS {
        return (redacted, false, false);
    }
    let cut: String = redacted.chars().take(MAX_BODY_CHARS).collect();
    (cut, false, true)
}

impl ApiDto {
    /// Builds the DTO for an already-loaded `Api`, pairing it with the raw text
    /// the file held on disk so the editor opens the real bytes, not a
    /// re-serialization.
    pub fn from_api(api: &reqchain_core::model::Api, text: String) -> ApiDto {
        ApiDto {
            id: api.id.clone(),
            name: api.name.clone(),
            base_url: api.base_url.clone(),
            environments: api.environments.iter().map(|e| e.name.clone()).collect(),
            endpoints: api
                .endpoints
                .iter()
                .map(|ep| EndpointDto::from_endpoint(api, ep))
                .collect(),
            text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqchain_core::model::Method;
    use reqchain_core::request::EffectiveRequest;

    fn result_with(headers: Vec<(String, String)>, body: Vec<u8>) -> RunResult {
        let req = EffectiveRequest {
            method: Method::Get,
            url: "https://api.example.com/x".into(),
            headers: vec![],
            body: None,
        };
        RunResult {
            status: 200,
            elapsed_ms: 1,
            size_bytes: body.len(),
            headers,
            body,
            effective: req,
            auth_trace: vec![],
        }
    }

    #[test]
    fn a_binary_content_type_is_not_rendered_even_when_the_bytes_are_valid_utf8() {
        // An SVG is valid UTF-8, so UTF-8 validity alone would dump the whole
        // document into the body pane as text.
        let result = result_with(
            vec![("content-type".into(), "image/svg+xml".into())],
            b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec(),
        );
        let (body, is_binary, truncated) = render_body(&result, &[]);
        assert!(is_binary);
        assert!(body.is_empty());
        assert!(!truncated);
    }

    #[test]
    fn a_json_content_type_is_still_rendered_as_text() {
        let result = result_with(
            vec![(
                "Content-Type".into(),
                "application/json; charset=utf-8".into(),
            )],
            br#"{"ok":true}"#.to_vec(),
        );
        let (body, is_binary, truncated) = render_body(&result, &[]);
        assert!(!is_binary);
        assert_eq!(body, r#"{"ok":true}"#);
        assert!(!truncated);
    }

    #[test]
    fn a_body_past_the_cap_is_truncated_and_flagged() {
        let huge = "x".repeat(MAX_BODY_CHARS + 500);
        let result = result_with(vec![], huge.into_bytes());
        let (body, is_binary, truncated) = render_body(&result, &[]);
        assert!(!is_binary);
        assert!(truncated);
        assert_eq!(body.chars().count(), MAX_BODY_CHARS);
    }

    /// Truncation must never be able to expose half of a credential: the mask
    /// is applied to the whole body first, and only the redacted text is cut.
    #[test]
    fn redaction_happens_before_truncation() {
        let secret = "super-secret-value";
        let mut body = "y".repeat(MAX_BODY_CHARS - 5);
        body.push_str(secret);
        body.push_str(&"z".repeat(100));
        let result = result_with(vec![], body.into_bytes());
        let (shown, _, truncated) = render_body(&result, &[secret.to_string()]);
        assert!(truncated);
        assert!(!shown.contains(secret));
        assert!(!shown.contains("super-secret"));
        // The two assertions above both pass under the WRONG order too:
        // truncating first leaves the 5-char fragment "super" at the cut,
        // which contains neither the full secret nor "super-secret". This is
        // the assertion that actually distinguishes the orders.
        assert!(!shown.contains("super"));
    }
}
