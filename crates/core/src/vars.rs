use crate::model::{Api, Endpoint, Vars};
use crate::secrets::Secrets;

#[derive(Debug, thiserror::Error)]
pub enum VarError {
    #[error(
        "unknown variable `{name}` — define it on the endpoint, the active environment, or the API"
    )]
    Unknown { name: String },
    #[error("unknown secret `{name}` — add it to the secret store")]
    UnknownSecret { name: String },
}

pub struct Scope<'a> {
    endpoint_vars: &'a Vars,
    env_vars: Option<&'a Vars>,
    api_vars: &'a Vars,
    secrets: &'a Secrets,
}

impl<'a> Scope<'a> {
    pub fn new(
        api: &'a Api,
        endpoint: &'a Endpoint,
        env: Option<&str>,
        secrets: &'a Secrets,
    ) -> Scope<'a> {
        let env_vars = env
            .and_then(|name| api.environments.iter().find(|e| e.name == name))
            .map(|e| &e.variables);
        Scope {
            endpoint_vars: &endpoint.variables,
            env_vars,
            api_vars: &api.variables,
            secrets,
        }
    }

    /// Precedence, most specific first: endpoint -> active environment -> API.
    pub fn lookup(&self, name: &str) -> Option<&str> {
        self.endpoint_vars
            .get(name)
            .or_else(|| self.env_vars.and_then(|v| v.get(name)))
            .or_else(|| self.api_vars.get(name))
            .map(|s| s.as_str())
    }

    pub fn interpolate(&self, input: &str) -> Result<String, VarError> {
        self.expand(input, false)
    }

    pub fn interpolate_masked(&self, input: &str) -> String {
        self.expand(input, true)
            .unwrap_or_else(|_| input.to_string())
    }

    pub fn secret_values(&self) -> Vec<String> {
        self.secrets.values().cloned().collect()
    }

    fn expand(&self, input: &str, mask: bool) -> Result<String, VarError> {
        let mut out = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else {
                out.push_str(&rest[start..]);
                return Ok(out);
            };
            let name = after[..end].trim();
            if let Some(secret) = name.strip_prefix("secret:") {
                let secret = secret.trim();
                let value = self
                    .secrets
                    .get(secret)
                    .ok_or_else(|| VarError::UnknownSecret {
                        name: secret.to_string(),
                    })?;
                out.push_str(if mask { "***" } else { value });
            } else {
                let value = self.lookup(name).ok_or_else(|| VarError::Unknown {
                    name: name.to_string(),
                })?;
                out.push_str(value);
            }
            rest = &after[end + 2..];
        }
        out.push_str(rest);
        Ok(out)
    }
}
