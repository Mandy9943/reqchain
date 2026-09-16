use reqchain_core::exec::{AuthStep, RunResult};
use reqchain_core::request::{redact, EffectiveBody, EffectiveRequest};
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

impl From<&reqchain_core::model::Endpoint> for EndpointDto {
    fn from(ep: &reqchain_core::model::Endpoint) -> EndpointDto {
        EndpointDto {
            id: ep.id.clone(),
            name: ep.name.clone(),
            method: ep.method.as_str().to_string(),
            path: ep.path.clone(),
            auth_kind: auth_kind(&ep.auth).to_string(),
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

impl From<&EffectiveRequest> for EffectiveDto {
    fn from(req: &EffectiveRequest) -> EffectiveDto {
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
    /// the auth endpoint — not masked yet — so it is masked here with the
    /// same `mask`/`auth_headers` every other request crossing into the DTO
    /// layer goes through (the masking contract in the task brief). `step.body`
    /// is the auth endpoint's raw response body, which can hold the token
    /// verbatim, so it is passed through the same string-level `redact` the
    /// top-level response body uses.
    fn from_step(step: &AuthStep, mask: &[String], auth_headers: &[String]) -> AuthStepDto {
        let masked_request = step
            .request
            .as_ref()
            .map(|r| r.masked_with(mask, auth_headers));
        AuthStepDto {
            endpoint_id: step.endpoint_id.clone(),
            request: masked_request.as_ref().map(EffectiveDto::from),
            status: if step.from_cache {
                None
            } else {
                Some(step.status)
            },
            body: redact(&step.body, mask),
            from_cache: step.from_cache,
        }
    }
}

impl RunDto {
    /// Builds the DTO for a finished run. `mask` must be the executor's
    /// derived tokens plus every secret value
    /// (`executor.derived_values()` extended with `secrets.values()`);
    /// `auth_headers` must be `executor.auth_headers()`. Every string that
    /// could carry a credential — the effective request, each auth-trace
    /// request and body, and the response body — is masked or redacted
    /// here; this is the only place a `RunDto` may be built from a raw
    /// `RunResult`.
    pub fn from_result(result: &RunResult, mask: &[String], auth_headers: &[String]) -> RunDto {
        let masked_effective = result.effective.masked_with(mask, auth_headers);
        let (body, body_is_binary) = match std::str::from_utf8(&result.body) {
            Ok(text) => (redact(text, mask), false),
            Err(_) => (String::new(), true),
        };
        RunDto {
            status: result.status,
            elapsed_ms: result.elapsed_ms.min(u128::from(u64::MAX)) as u64,
            size_bytes: result.size_bytes,
            headers: result
                .headers
                .iter()
                .map(|(k, v)| [k.clone(), redact(v, mask)])
                .collect(),
            body,
            body_is_binary,
            effective: EffectiveDto::from(&masked_effective),
            auth_trace: result
                .auth_trace
                .iter()
                .map(|s| AuthStepDto::from_step(s, mask, auth_headers))
                .collect(),
        }
    }
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
            endpoints: api.endpoints.iter().map(Into::into).collect(),
            text,
        }
    }
}
