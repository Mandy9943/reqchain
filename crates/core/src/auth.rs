use crate::expr::{self, ExprError};
use crate::model::{Api, Auth, Endpoint};
use crate::request::EffectiveRequest;
use crate::vars::{Scope, VarError};
use base64::Engine;

const NONE: Auth = Auth::None;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error(transparent)]
    Var(#[from] VarError),
    #[error(transparent)]
    Expr(#[from] ExprError),
}

/// Resolves `inherit` to the API-level auth. An API whose own auth is `inherit`
/// is treated as `none` — there is nothing above it to inherit from.
pub fn resolve<'a>(api: &'a Api, endpoint: &'a Endpoint) -> &'a Auth {
    match &endpoint.auth {
        Auth::Inherit => match &api.auth {
            Auth::Inherit => &NONE,
            other => other,
        },
        other => other,
    }
}

fn set_header(req: &mut EffectiveRequest, name: &str, value: String) {
    req.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
    req.headers.push((name.to_string(), value));
}

pub fn apply_static(
    api: &Api,
    endpoint: &Endpoint,
    scope: &Scope,
    req: &mut EffectiveRequest,
) -> Result<(), AuthError> {
    match resolve(api, endpoint) {
        Auth::Inherit | Auth::None => {}
        Auth::Basic { username, password } => {
            let user = scope.interpolate(username)?;
            let pass = scope.interpolate(password)?;
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
            set_header(req, "Authorization", format!("Basic {encoded}"));
        }
        Auth::Bearer { token } => {
            set_header(
                req,
                "Authorization",
                format!("Bearer {}", scope.interpolate(token)?),
            );
        }
        Auth::Header { headers } => {
            for (k, v) in headers {
                let value = scope.interpolate(v)?;
                set_header(req, k, value);
            }
        }
        Auth::Computed { name, expression } => {
            let value = expr::eval(expression, scope)?;
            set_header(req, name, value);
        }
        // Chained auth needs I/O; `chain::Executor` owns it.
        Auth::Chained { .. } => {}
    }
    Ok(())
}
