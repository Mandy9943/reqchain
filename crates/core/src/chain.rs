use crate::auth::{self, AuthError};
use crate::cache::{now_unix, TokenCache};
use crate::exec::{AuthStep, ExecError, RunResult, Runner};
use crate::model::{Api, Auth, AuthExtract, AuthInject, AuthTtl, TtlUnit};
use crate::request::{self, urlencode, BuildError, EffectiveBody, EffectiveRequest};
use crate::secrets::Secrets;
use crate::vars::Scope;

pub const MAX_DEPTH: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error(transparent)]
    Exec(#[from] ExecError),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("{message}")]
    Chain { message: String },
}

/// Whether building an effective request may resolve a chained auth for real
/// (performing the auth request) or must render it as a placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChainMode {
    /// Fetch the token, sending the auth request if it is not cached.
    Resolve,
    /// Render a placeholder; perform no I/O.
    Placeholder,
}

pub struct Executor {
    runner: Runner,
    cache: TokenCache,
    secrets: Secrets,
    derived: Vec<String>,
    auth_headers: Vec<String>,
    static_values: Vec<String>,
}

impl Executor {
    pub fn new(runner: Runner, cache: TokenCache, secrets: Secrets) -> Executor {
        Executor {
            runner,
            cache,
            secrets,
            derived: Vec::new(),
            auth_headers: Vec::new(),
            static_values: Vec::new(),
        }
    }

    pub fn cache_mut(&mut self) -> &mut TokenCache {
        &mut self.cache
    }

    /// Replaces the secret store this executor resolves `{{secret:...}}`
    /// against. Without this, an executor created once at startup keeps
    /// interpolating a secret's value from the moment it was constructed —
    /// after a rotation or deletion, it would keep sending the stale value
    /// on the wire while only the NEW value is known to be sensitive
    /// (present in a caller's mask), so the stale one would appear
    /// unmasked in the effective request, the curl export and history.
    /// Callers that hold a long-lived `Executor` (the desktop app's
    /// `AppState`) must call this whenever the secret store is reloaded.
    pub fn set_secrets(&mut self, secrets: Secrets) {
        self.secrets = secrets;
    }

    /// Every token this executor has derived from an auth response. They are not
    /// in the secret store, so callers must add them to the mask list before
    /// displaying a request, a response or a shell export.
    pub fn derived_values(&self) -> Vec<String> {
        self.derived.clone()
    }

    /// Every header name a static auth has written a credential to. A caller that
    /// displays a request must redact these headers' values BY NAME: unlike a secret
    /// or a chain token, a static auth value (`X-Api-Version: 1`, say) is not
    /// something that may appear elsewhere, and masking it globally by substring
    /// would corrupt unrelated parts of the display.
    pub fn auth_headers(&self) -> Vec<String> {
        self.auth_headers.clone()
    }

    /// Every literal value a *static* auth mechanism (basic, bearer, header,
    /// computed) has set on a request so far — a Basic base64 blob, a fixed
    /// bearer token, a computed header's value. These are not secrets or
    /// chain-derived tokens, so they must never join the global REQUEST mask
    /// (that would corrupt unrelated request output — see `auth_headers`);
    /// but a server can echo one of them back in a response body or header,
    /// so a caller building a RESPONSE-side mask (for the response body, its
    /// headers, or history) should extend it with these.
    pub fn static_values(&self) -> Vec<String> {
        self.static_values.clone()
    }

    fn remember(&mut self, value: &str) {
        if !value.is_empty() && !self.derived.iter().any(|v| v == value) {
            self.derived.push(value.to_string());
        }
    }

    fn remember_auth_header(&mut self, name: &str) {
        if !name.is_empty() && !self.auth_headers.iter().any(|h| h == name) {
            self.auth_headers.push(name.to_string());
        }
    }

    fn remember_static_value(&mut self, value: &str) {
        if !value.is_empty() && !self.static_values.iter().any(|v| v == value) {
            self.static_values.push(value.to_string());
        }
    }

    pub async fn run(
        &mut self,
        api: &Api,
        endpoint_id: &str,
        env: Option<&str>,
    ) -> Result<RunResult, RunError> {
        let mut visited = Vec::new();
        self.run_inner(api, endpoint_id, env, &mut visited).await
    }

    async fn run_inner(
        &mut self,
        api: &Api,
        endpoint_id: &str,
        env: Option<&str>,
        visited: &mut Vec<String>,
    ) -> Result<RunResult, RunError> {
        if visited.iter().any(|v| v == endpoint_id) {
            visited.push(endpoint_id.to_string());
            return Err(RunError::Chain {
                message: format!("auth cycle detected: {}", visited.join(" -> ")),
            });
        }
        if visited.len() >= MAX_DEPTH {
            return Err(RunError::Chain {
                message: format!(
                    "auth chain deeper than {MAX_DEPTH} levels: {}",
                    visited.join(" -> ")
                ),
            });
        }
        visited.push(endpoint_id.to_string());

        let endpoint = api.endpoint(endpoint_id).ok_or_else(|| RunError::Chain {
            message: format!("endpoint `{endpoint_id}` not found in API `{}`", api.id),
        })?;
        let scope = Scope::new(api, endpoint, env, &self.secrets);
        let mut req = request::build(api, endpoint, &scope)?;
        for (header, value) in auth::apply_static(api, endpoint, &scope, &mut req)? {
            self.remember_auth_header(&header);
            self.remember_static_value(&value);
        }

        let resolved = auth::resolve(api, endpoint).clone();
        let Auth::Chained {
            source,
            extract,
            ttl,
            inject,
            retry_on,
        } = resolved.clone()
        else {
            let mut res = self.runner.send(&req).await?;
            res.effective = req;
            return Ok(res);
        };

        let key = TokenCache::key(
            &api.id,
            &resolved,
            &self.scope_fingerprint(api, &source.endpoint, env),
        );

        let (value, mut trace) = self
            .token(api, &source.endpoint, env, &extract, &ttl, &key, visited)
            .await?;

        let mut attempt = req.clone();
        inject_value(&mut attempt, &inject, &value)?;
        let mut res = self.runner.send(&attempt).await?;
        res.effective = attempt;

        if retry_on.contains(&res.status) {
            self.cache.invalidate(&key);
            // A fresh chain walk: the previous one is already recorded in `trace`.
            let mut retry_visited = vec![endpoint_id.to_string()];
            let (fresh, steps) = self
                .token(
                    api,
                    &source.endpoint,
                    env,
                    &extract,
                    &ttl,
                    &key,
                    &mut retry_visited,
                )
                .await?;
            trace.extend(steps);
            let mut retry = req.clone();
            inject_value(&mut retry, &inject, &fresh)?;
            res = self.runner.send(&retry).await?;
            res.effective = retry;
        }

        res.auth_trace = trace;
        Ok(res)
    }

    /// The placeholder a [`Executor::preview`] renders in place of a chained
    /// token it deliberately did not fetch. It names the source endpoint, so the
    /// preview still says where the credential would come from — it is a
    /// description of the chain, not a pretence that a token exists.
    pub fn chained_token_placeholder(source_endpoint: &str) -> String {
        format!(
            "{}{source_endpoint}{}",
            request::PLACEHOLDER_OPEN,
            request::PLACEHOLDER_CLOSE
        )
    }

    /// Builds the effective request for `endpoint_id` exactly as [`Executor::run`]
    /// would — interpolating variables, applying static auth and resolving the auth
    /// chain — but never sends the endpoint's OWN request.
    ///
    /// Resolving a chain may still perform the AUTH request: a chained token does
    /// not exist until its source endpoint has been called. That makes this the
    /// right call for an export the user explicitly asked for (`copy as curl`,
    /// which must carry a real token) and the WRONG call for anything that runs on
    /// its own — use [`Executor::preview`] there.
    pub async fn prepare(
        &mut self,
        api: &Api,
        endpoint_id: &str,
        env: Option<&str>,
    ) -> Result<EffectiveRequest, RunError> {
        self.build_effective(api, endpoint_id, env, ChainMode::Resolve)
            .await
    }

    /// Like [`Executor::prepare`], but guaranteed to perform NO network I/O at
    /// all: variables are interpolated and static auth applied as usual, while a
    /// chained auth's injected value is rendered as
    /// [`Executor::chained_token_placeholder`] instead of being fetched.
    ///
    /// This exists because the desktop app previews the selected endpoint
    /// automatically, on selection and on environment change. Resolving the chain
    /// there would POST to a production token endpoint merely because the user
    /// clicked around the sidebar — a live request the user never asked for, that
    /// appears nowhere in the UI, which is the exact opposite of the design spec's
    /// "never a black box" (§7).
    pub async fn preview(
        &mut self,
        api: &Api,
        endpoint_id: &str,
        env: Option<&str>,
    ) -> Result<EffectiveRequest, RunError> {
        self.build_effective(api, endpoint_id, env, ChainMode::Placeholder)
            .await
    }

    async fn build_effective(
        &mut self,
        api: &Api,
        endpoint_id: &str,
        env: Option<&str>,
        mode: ChainMode,
    ) -> Result<EffectiveRequest, RunError> {
        let endpoint = api.endpoint(endpoint_id).ok_or_else(|| RunError::Chain {
            message: format!("endpoint `{endpoint_id}` not found in API `{}`", api.id),
        })?;
        let scope = Scope::new(api, endpoint, env, &self.secrets);
        let mut req = request::build(api, endpoint, &scope)?;
        for (header, value) in auth::apply_static(api, endpoint, &scope, &mut req)? {
            self.remember_auth_header(&header);
            self.remember_static_value(&value);
        }

        let resolved = auth::resolve(api, endpoint).clone();
        let Auth::Chained {
            source,
            extract,
            ttl,
            inject,
            ..
        } = resolved.clone()
        else {
            return Ok(req);
        };

        let value = match mode {
            // Never touches the network, and never consults the cache either: a
            // preview that showed a real token when one happened to be cached and
            // a placeholder otherwise would be an inconsistent, and occasionally
            // credential-bearing, display of the same endpoint.
            ChainMode::Placeholder => Executor::chained_token_placeholder(&source.endpoint),
            ChainMode::Resolve => {
                let key = TokenCache::key(
                    &api.id,
                    &resolved,
                    &self.scope_fingerprint(api, &source.endpoint, env),
                );
                let mut visited = vec![endpoint_id.to_string()];
                let (value, _trace) = self
                    .token(
                        api,
                        &source.endpoint,
                        env,
                        &extract,
                        &ttl,
                        &key,
                        &mut visited,
                    )
                    .await?;
                value
            }
        };
        inject_value(&mut req, &inject, &value)?;
        Ok(req)
    }

    /// The part of a cache key that is not the auth *definition*: the active
    /// environment plus a digest of the credential values that definition actually
    /// resolves to along the chain it walks. Hashing the resolved credentials — and
    /// only their hash, which is all that ever reaches the cache file — makes
    /// rotating a secret produce a different key, so the stale token is not reused
    /// (design spec §7).
    fn scope_fingerprint(&self, api: &Api, source_id: &str, env: Option<&str>) -> String {
        use sha2::Digest;
        let mut material = String::new();
        let mut current = source_id.to_string();
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..MAX_DEPTH {
            if seen.iter().any(|v| v == &current) {
                break;
            }
            seen.push(current.clone());
            let Some(ep) = api.endpoint(&current) else {
                break;
            };
            let scope = Scope::new(api, ep, env, &self.secrets);
            let mut probe = EffectiveRequest {
                method: ep.method,
                url: String::new(),
                headers: Vec::new(),
                body: None,
            };
            // A credential that cannot be resolved (an unknown secret, say) is not a
            // fingerprinting concern: the run itself is about to fail on it.
            if let Ok(values) = auth::apply_static(api, ep, &scope, &mut probe) {
                for (_, v) in values {
                    material.push_str(&v);
                    material.push('\u{0}');
                }
            }
            match auth::resolve(api, ep) {
                Auth::Chained { source, .. } => current = source.endpoint.clone(),
                _ => break,
            }
        }
        let digest = sha2::Sha256::digest(material.as_bytes());
        format!("{}\u{0}{digest:x}", env.unwrap_or(""))
    }

    #[allow(clippy::too_many_arguments)]
    async fn token(
        &mut self,
        api: &Api,
        source_id: &str,
        env: Option<&str>,
        extract: &Option<AuthExtract>,
        ttl: &Option<AuthTtl>,
        key: &str,
        visited: &mut Vec<String>,
    ) -> Result<(String, Vec<AuthStep>), RunError> {
        if let Some(cached) = self.cache.get(key, now_unix()) {
            self.remember(&cached);
            return Ok((
                cached,
                vec![AuthStep {
                    endpoint_id: source_id.to_string(),
                    request: None,
                    status: 0,
                    body: String::new(),
                    from_cache: true,
                }],
            ));
        }

        if api.endpoint(source_id).is_none() {
            return Err(RunError::Chain {
                message: format!(
                    "chained auth points at endpoint `{source_id}`, which does not exist in API `{}`",
                    api.id
                ),
            });
        }

        let mut res = Box::pin(self.run_inner(api, source_id, env, visited)).await?;
        // The nested run may itself have been authenticated by a chain: keep its
        // steps so the whole auth path stays visible, deepest first.
        let nested = std::mem::take(&mut res.auth_trace);
        let body_text = res.body_text();

        // A failing auth endpoint must be reported as such, not as a missing token.
        let wants_status = matches!(extract, Some(AuthExtract::Status));
        if !wants_status && !(200..300).contains(&res.status) {
            return Err(RunError::Chain {
                message: format!(
                    "chained auth: auth endpoint `{source_id}` returned {} — {}",
                    res.status,
                    excerpt(&body_text)
                ),
            });
        }

        let value = extract_value(&res, &body_text, extract)?;
        // No expiry means no cache: a token we cannot age out would go stale
        // forever and only a 401 would recover it.
        if let Some(expires_at) = compute_expiry(&body_text, ttl)? {
            self.cache.put(key, value.clone(), Some(expires_at));
        }
        self.remember(&value);
        let mut trace = nested;
        trace.push(AuthStep {
            endpoint_id: source_id.to_string(),
            request: Some(res.effective.clone()),
            status: res.status,
            body: body_text,
            from_cache: false,
        });
        Ok((value, trace))
    }
}

fn default_extract() -> AuthExtract {
    AuthExtract::Body {
        json_path: Some("$.access_token".into()),
        xpath: None,
        regex: None,
    }
}

fn excerpt(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "empty response body".to_string();
    }
    let mut out: String = trimmed.chars().take(200).collect();
    if trimmed.chars().count() > 200 {
        out.push_str("...")
    }
    out
}

fn json_keys(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.as_object()
                .map(|o| o.keys().cloned().collect::<Vec<_>>().join(", "))
        })
        .unwrap_or_else(|| "<not a JSON object>".into())
}

fn query_one(path: &str, value: &serde_json::Value) -> Result<Option<serde_json::Value>, RunError> {
    let jp = serde_json_path::JsonPath::parse(path).map_err(|e| RunError::Chain {
        message: format!("chained auth: invalid jsonPath `{path}`: {e}"),
    })?;
    Ok(jp.query(value).first().cloned())
}

fn extract_value(
    res: &RunResult,
    body: &str,
    extract: &Option<AuthExtract>,
) -> Result<String, RunError> {
    let was_default = extract.is_none();
    match extract.clone().unwrap_or_else(default_extract) {
        AuthExtract::Status => Ok(res.status.to_string()),
        AuthExtract::Header { name, regex } => {
            let raw = res
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&name))
                .map(|(_, v)| v.clone())
                .ok_or_else(|| RunError::Chain {
                    message: format!(
                        "chained auth: response header `{name}` not found (headers: {})",
                        res.headers
                            .iter()
                            .map(|(k, _)| k.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                })?;
            apply_regex(&raw, regex.as_deref())
        }
        AuthExtract::Body {
            json_path,
            xpath,
            regex,
        } => {
            if xpath.is_some() {
                return Err(RunError::Chain {
                    message: "chained auth: xpath extraction is not implemented yet — use jsonPath or regex".into(),
                });
            }
            if let Some(rx) = regex {
                return apply_regex(body, Some(&rx));
            }
            let path = json_path.unwrap_or_else(|| "$.access_token".into());
            let value: serde_json::Value =
                serde_json::from_str(body).map_err(|e| RunError::Chain {
                    message: format!(
                        "chained auth: auth response is not JSON ({e}), cannot apply `{path}`"
                    ),
                })?;
            match query_one(&path, &value)? {
                Some(serde_json::Value::String(s)) => Ok(s),
                Some(other) => Ok(other.to_string().trim_matches('"').to_string()),
                None if was_default => Err(RunError::Chain {
                    message: format!(
                        "chained auth: default extract {path} not found in response body (keys: {}) — set auth.extract explicitly",
                        json_keys(body)
                    ),
                }),
                None => Err(RunError::Chain {
                    message: format!(
                        "chained auth: {path} not found in response body (keys: {})",
                        json_keys(body)
                    ),
                }),
            }
        }
    }
}

fn apply_regex(input: &str, regex: Option<&str>) -> Result<String, RunError> {
    let Some(rx) = regex else {
        return Ok(input.to_string());
    };
    let re = regex::Regex::new(rx).map_err(|e| RunError::Chain {
        message: format!("chained auth: invalid regex `{rx}`: {e}"),
    })?;
    let caps = re.captures(input).ok_or_else(|| RunError::Chain {
        message: format!("chained auth: regex `{rx}` did not match the auth response"),
    })?;
    Ok(caps
        .get(1)
        .or_else(|| caps.get(0))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default())
}

fn compute_expiry(body: &str, ttl: &Option<AuthTtl>) -> Result<Option<u64>, RunError> {
    let ttl = ttl.clone().unwrap_or(AuthTtl::Body {
        json_path: "$.expires_in".into(),
        unit: TtlUnit::Seconds,
    });
    match ttl {
        AuthTtl::Fixed { seconds } => Ok(Some(now_unix() + seconds)),
        AuthTtl::Body { json_path, unit } => {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
                return Ok(None);
            };
            let Some(found) = query_one(&json_path, &value)? else {
                return Ok(None);
            };
            let Some(n) = found
                .as_u64()
                .or_else(|| found.as_str().and_then(|s| s.parse().ok()))
            else {
                return Ok(None);
            };
            let seconds = match unit {
                TtlUnit::Milliseconds => n / 1000,
                TtlUnit::Seconds => n,
            };
            Ok(Some(now_unix() + seconds))
        }
        AuthTtl::Absolute { json_path } => {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
                return Ok(None);
            };
            let Some(found) =
                query_one(&json_path, &value)?.and_then(|v| v.as_str().map(String::from))
            else {
                return Ok(None);
            };
            match chrono::DateTime::parse_from_rfc3339(&found) {
                Ok(dt) => Ok(Some(dt.timestamp().max(0) as u64)),
                Err(e) => Err(RunError::Chain {
                    message: format!("chained auth: `{found}` is not an RFC 3339 date ({e})"),
                }),
            }
        }
    }
}

fn inject_value(
    req: &mut EffectiveRequest,
    inject: &AuthInject,
    value: &str,
) -> Result<(), RunError> {
    match inject {
        AuthInject::Header { name, template } => {
            let rendered = template.replace("{{value}}", value);
            req.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
            req.headers.push((name.clone(), rendered));
            Ok(())
        }
        AuthInject::Query { name, template } => {
            let rendered = template.replace("{{value}}", value);
            let sep = if req.url.contains('?') { '&' } else { '?' };
            req.url.push(sep);
            req.url
                .push_str(&format!("{}={}", urlencode(name), urlencode(&rendered)));
            Ok(())
        }
        AuthInject::Body { pointer, template } => {
            let rendered = template.replace("{{value}}", value);
            let Some(EffectiveBody::Text { content, .. }) = &mut req.body else {
                return Err(RunError::Chain {
                    message: format!(
                        "chained auth: cannot inject the token at `{pointer}` — the request has no JSON body"
                    ),
                });
            };
            let mut json: serde_json::Value =
                serde_json::from_str(content).map_err(|e| RunError::Chain {
                    message: format!(
                        "chained auth: cannot inject the token at `{pointer}` — the request body is not JSON ({e})"
                    ),
                })?;
            let Some(slot) = json.pointer_mut(pointer) else {
                return Err(RunError::Chain {
                    message: format!(
                        "chained auth: JSON pointer `{pointer}` matches nothing in the request body (keys: {}) — the request would go out unauthenticated",
                        json_keys(content)
                    ),
                });
            };
            *slot = serde_json::Value::String(rendered);
            *content = json.to_string();
            Ok(())
        }
    }
}
