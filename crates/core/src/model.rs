use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

pub type Vars = BTreeMap<String, String>;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("invalid JSON at line {line}, column {column}: {message}")]
    Json {
        line: usize,
        column: usize,
        message: String,
    },
    #[error("unsupported schemaVersion {found}, this build understands {expected}")]
    SchemaVersion { found: u32, expected: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Api {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(default)]
    pub variables: Vars,
    #[serde(default)]
    pub environments: Vec<Environment>,
    #[serde(default = "Auth::none")]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<HistoryConfig>,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryConfig {
    #[serde(default = "default_true")]
    pub store_bodies: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub name: String,
    #[serde(default)]
    pub variables: Vars,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
    pub method: Method,
    pub path: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub query: BTreeMap<String, String>,
    #[serde(default)]
    pub variables: Vars,
    #[serde(default = "Auth::inherit")]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Body {
    Json {
        content: serde_json::Value,
    },
    Form {
        fields: BTreeMap<String, String>,
    },
    Multipart {
        fields: BTreeMap<String, String>,
        #[serde(default)]
        files: BTreeMap<String, String>,
    },
    Text {
        content: String,
    },
    Xml {
        content: String,
    },
    Binary {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Auth {
    Inherit,
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    Header {
        headers: BTreeMap<String, String>,
    },
    Computed {
        name: String,
        expression: String,
    },
    Chained {
        source: ChainSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        extract: Option<AuthExtract>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ttl: Option<AuthTtl>,
        inject: AuthInject,
        #[serde(default = "default_retry_on", rename = "retryOn")]
        retry_on: Vec<u16>,
    },
}

impl Auth {
    pub fn none() -> Self {
        Auth::None
    }
    pub fn inherit() -> Self {
        Auth::Inherit
    }
}

fn default_retry_on() -> Vec<u16> {
    vec![401, 403]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainSource {
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "lowercase", deny_unknown_fields)]
pub enum AuthExtract {
    Body {
        #[serde(default, rename = "jsonPath", skip_serializing_if = "Option::is_none")]
        json_path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        xpath: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        regex: Option<String>,
    },
    Header {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        regex: Option<String>,
    },
    Status,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "lowercase", deny_unknown_fields)]
pub enum AuthTtl {
    Body {
        #[serde(rename = "jsonPath")]
        json_path: String,
        #[serde(default)]
        unit: TtlUnit,
    },
    Fixed {
        seconds: u64,
    },
    Absolute {
        #[serde(rename = "jsonPath")]
        json_path: String,
    },
}

/// The only units a relative, body-derived ttl may be expressed in. Anything else
/// is rejected at load time rather than silently read as seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TtlUnit {
    #[default]
    Seconds,
    Milliseconds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "into", rename_all = "lowercase", deny_unknown_fields)]
pub enum AuthInject {
    Header {
        name: String,
        template: String,
    },
    Query {
        name: String,
        #[serde(default = "value_template")]
        template: String,
    },
    Body {
        pointer: String,
        #[serde(default = "value_template")]
        template: String,
    },
}

fn value_template() -> String {
    "{{value}}".to_string()
}

impl Api {
    pub fn from_json(text: &str) -> Result<Api, LoadError> {
        let api: Api = serde_json::from_str(text).map_err(|e| LoadError::Json {
            line: e.line(),
            column: e.column(),
            message: e.to_string(),
        })?;
        if api.schema_version != SCHEMA_VERSION {
            return Err(LoadError::SchemaVersion {
                found: api.schema_version,
                expected: SCHEMA_VERSION,
            });
        }
        Ok(api)
    }

    pub fn to_json_string(&self) -> String {
        let mut out = serde_json::to_string_pretty(self).expect("model is serializable");
        out.push('\n');
        out
    }

    pub fn endpoint(&self, id: &str) -> Option<&Endpoint> {
        self.endpoints.iter().find(|e| e.id == id)
    }
}
