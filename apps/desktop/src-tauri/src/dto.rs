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
