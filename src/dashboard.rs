//! Local web dashboard (`research dashboard`).
//!
//! A GET-only HTTP server bound to 127.0.0.1 that serves a single embedded
//! HTML page plus read-only JSON endpoints over the workspace database.
//! Request handling is a pure `route` function so routing is testable
//! without sockets; the accept loop stays thin.

use crate::adapters::sqlite_store::SqliteStore;
use crate::config::Config;
use crate::domain::paper::{Paper, PaperStatus, ReadingStatus};
use crate::ports::index_store::IndexStore;
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const INDEX_HTML: &str = include_str!("dashboard/assets/index.html");
/// Minimal token-entry page served instead of the app while unauthenticated.
const LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>research-agent — sign in</title><style>
body{margin:0;min-height:100vh;display:grid;place-items:center;background:#14120E;color:#EAE4D8;
font:14px/1.5 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
form{background:#1B1813;border:1px solid #2C2720;border-radius:8px;padding:28px;width:min(340px,90vw)}
h1{font:600 19px "Iowan Old Style",Palatino,Georgia,serif;margin:0 0 4px}
p{color:#A69C8A;font-size:12.5px;margin:0 0 16px}
input{width:100%;box-sizing:border-box;padding:8px 10px;background:#14120E;color:#EAE4D8;
border:1px solid #2C2720;border-radius:6px;font:13px ui-monospace,Menlo,monospace}
button{margin-top:12px;width:100%;padding:8px;background:#221E17;color:#EAE4D8;
border:1px solid #6E6656;border-radius:6px;font:inherit;cursor:pointer}
button:hover{border-color:#F0BE3A}.err{color:#C4553D;font-size:12px;margin:8px 0 0;min-height:15px}
</style></head><body>
<form onsubmit="return submitToken(event)">
<h1>research-agent</h1><p>This dashboard is exposed to the network. Enter its access token.</p>
<input id="token" type="password" autofocus autocomplete="current-password">
<div class="err" id="err"></div><button>Sign in</button></form>
<script>
async function submitToken(e){
  e.preventDefault();
  const r=await fetch("/api/login",{method:"POST",headers:{"Content-Type":"application/json"},
    body:JSON.stringify({token:document.getElementById("token").value})});
  if(r.ok){location.href="/";return false}
  document.getElementById("err").textContent="That token does not match.";
  return false;
}
</script></body></html>"#;
/// Cap per history stream so a large library can't produce a multi-MB payload.
const HISTORY_LIMIT: usize = 500;
/// The only `Host` values this server accepts (with any port).
const LOOPBACK_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];
/// Valid `reading_status` values for the PATCH endpoint.
const READING_STATUSES: [&str; 5] = ["unread", "queued", "in_progress", "completed", "abandoned"];

/// Shared per-request context: everything the route function may touch.
struct RouteCtx {
    db_path: PathBuf,
    /// The `--db` flag this server instance was started with, if any. A
    /// restart must re-apply it — the config file cannot express it.
    db_override: Option<PathBuf>,
    /// Config file backing `/api/config` (injected so tests never touch $HOME).
    config_path: PathBuf,
    /// `Host` header from the request, if any.
    host: Option<String>,
    /// `Content-Type` header from the request, if any.
    content_type: Option<String>,
    /// `Authorization` header from the request, if any.
    authz: Option<String>,
    /// `Cookie` header from the request, if any.
    cookie: Option<String>,
    /// Required shared secret when the dashboard is bound non-loopback.
    token: Option<String>,
}

struct Response {
    status: u16,
    content_type: &'static str,
    body: String,
    /// Raw binary body (PDF bytes). When present it is written instead of
    /// `body`, and Content-Length is taken from it.
    binary: Option<Vec<u8>>,
    /// Optional Set-Cookie header value.
    set_cookie: Option<String>,
    /// Set by /api/restart: exit the process once the response is flushed.
    shutdown: bool,
}

impl Response {
    fn json(value: Value) -> Self {
        Self {
            status: 200,
            content_type: "application/json",
            body: value.to_string(),
            set_cookie: None,
            shutdown: false,
            binary: None,
        }
    }

    fn error(status: u16, message: &str) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: json!({ "error": message }).to_string(),
            set_cookie: None,
            shutdown: false,
            binary: None,
        }
    }

    fn html(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body,
            set_cookie: None,
            shutdown: false,
            binary: None,
        }
    }

    fn binary(content_type: &'static str, bytes: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type,
            body: String::new(),
            set_cookie: None,
            shutdown: false,
            binary: Some(bytes),
        }
    }
}

/// Serve the dashboard until interrupted. Binds to 127.0.0.1 only — the
/// endpoints expose the local library and are not meant to be shared.
/// `db_override` (the global `--db` flag) wins; otherwise the workspace
/// location comes from the config file, so changing it in the dashboard
/// settings and restarting actually moves the library.
pub async fn serve(
    db_override: Option<PathBuf>,
    config_path: PathBuf,
    port_override: Option<u16>,
) -> anyhow::Result<()> {
    let config = Config::load(&config_path)?;
    let db_path = db_override
        .clone()
        .unwrap_or_else(|| config.database_path.clone());
    let mut dash = config.dashboard;
    if let Some(port) = port_override {
        dash.port = Some(port);
    }
    let addr = format!("{}:{}", dash.host_or_default(), dash.port_or_default());
    let listener = TcpListener::bind(&addr).await?;
    let scope = if dash.is_loopback() {
        "localhost"
    } else {
        "NETWORK (token required)"
    };
    println!("Dashboard running at http://{addr}/ [{scope}] (Ctrl-C to stop)");
    loop {
        let (stream, _) = listener.accept().await?;
        let db_path = db_path.clone();
        let db_override = db_override.clone();
        let config_path = config_path.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, db_path, db_override, config_path).await {
                tracing::debug!(error = %e, "dashboard connection error");
            }
        });
    }
}

/// Read one HTTP request (head + body), route it, write the response, close.
/// One request per connection (`Connection: close`) keeps parsing trivial.
async fn handle_conn(
    mut stream: TcpStream,
    db_path: PathBuf,
    db_override: Option<PathBuf>,
    config_path: PathBuf,
) -> anyhow::Result<()> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    // The request head ends at the first CRLFCRLF. Cap the read so a
    // pathological client cannot buffer forever.
    let mut terminated = false;
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            terminated = true;
            break;
        }
        if buf.len() > 16 * 1024 {
            break;
        }
    }

    let head_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 4)
        .unwrap_or(buf.len());
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut parts = head.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let raw_path = parts.next().unwrap_or("").to_string();
    // Stripped form only for the /api/models special-case below; route()
    // splits its own query string off raw_path.
    let path = raw_path.split('?').next().unwrap_or("").to_string();

    // Requests with a body: read Content-Length bytes past the head (capped).
    let content_length: usize = head
        .lines()
        .find_map(|l| {
            l.strip_prefix("Content-Length:")
                .or_else(|| l.strip_prefix("content-length:"))
                .map(str::trim)
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0);
    let mut body: Option<String> = None;
    if content_length > 0 {
        if content_length > 64 * 1024 {
            let _ =
                write_response(&mut stream, &Response::error(413, "request body too large")).await;
            return Ok(());
        }
        while buf.len() < head_end + content_length {
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        body = String::from_utf8(buf[head_end..].to_vec()).ok();
    }

    // Reject requests whose Host is not loopback: a rebound DNS name would
    // otherwise let a public page read this server same-origin (DNS rebinding).
    let host = head.lines().find_map(|line| {
        line.strip_prefix("Host:")
            .or_else(|| line.strip_prefix("host:"))
            .map(str::trim)
    });
    let content_type = head.lines().find_map(|line| {
        line.strip_prefix("Content-Type:")
            .or_else(|| line.strip_prefix("content-type:"))
            .map(str::trim)
    });
    let header = |name: &str, lower: &str| {
        head.lines().find_map(|line| {
            line.strip_prefix(name)
                .or_else(|| line.strip_prefix(lower))
                .map(str::trim)
        })
    };
    let authz = header("Authorization:", "authorization:");
    let cookie = header("Cookie:", "cookie:");
    // Auth reads the same file `serve` started from; falling back to the
    // default here would split routing and auth across two configs.
    let token = Config::load(&config_path)
        .ok()
        .and_then(|c| c.dashboard.token);
    let ctx = RouteCtx {
        db_path,
        db_override,
        config_path,
        host: host.map(String::from),
        content_type: content_type.map(String::from),
        authz: authz.map(String::from),
        cookie: cookie.map(String::from),
        token,
    };
    let response = if terminated {
        // `/api/models` is the one async route; it must pass the same Host
        // guard as everything routed through `route`, or a rebound DNS name
        // could make the server spend the configured API key on its behalf.
        if method == "GET" && path == "/api/models" && host_allowed(&ctx) && authed_for_models(&ctx)
        {
            fetch_models(&ctx).await
        } else {
            route(&method, &raw_path, body.as_deref(), &ctx)
        }
    } else {
        // Head hit the cap without terminating: reject instead of parsing a
        // truncated request.
        Response::error(431, "request head too large")
    };
    write_response(&mut stream, &response).await
}

/// The models proxy needs the same auth as everything else.
fn authed_for_models(ctx: &RouteCtx) -> bool {
    match &ctx.token {
        None => true,
        Some(required) => request_token(ctx).is_some_and(|t| ct_eq(&t, required)),
    }
}

/// Query the configured provider's OpenAI-compatible `GET /models` endpoint
/// server-side (so the browser never needs CORS or the key) and return the
/// sorted model ids.
async fn fetch_models(ctx: &RouteCtx) -> Response {
    let config = match Config::load(&ctx.config_path) {
        Ok(c) => c,
        Err(e) => return Response::error(500, &format!("config error: {e}")),
    };
    let Some(llm) = config.llm else {
        return Response::error(400, "no LLM is configured");
    };
    let Some(key) = llm.resolve_api_key() else {
        return Response::error(400, "api key not set in environment");
    };
    let base = llm
        .base_url
        .clone()
        .unwrap_or_else(|| format!("https://api.{}.com/v1", llm.provider));
    let url = format!("{}/models", base.trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Response::error(500, &format!("http client error: {e}")),
    };
    let result = client.get(&url).bearer_auth(key).send().await;
    let response = match result {
        Ok(r) => r,
        Err(e) => return Response::error(502, &format!("provider unreachable: {e}")),
    };
    if !response.status().is_success() {
        return Response::error(502, &format!("provider returned {}", response.status()));
    }
    let body = match response.text().await {
        Ok(t) => t,
        Err(e) => return Response::error(502, &format!("provider body unreadable: {e}")),
    };
    Response::json(json!({ "models": parse_models(&body) }))
}

/// Extract model ids from an OpenAI-compatible `{"data":[{"id":...}]}` payload.
fn parse_models(body: &str) -> Vec<String> {
    let mut models: Vec<String> = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("data").and_then(Value::as_array).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from))
        .collect();
    models.sort();
    models
}

async fn write_response(stream: &mut TcpStream, response: &Response) -> anyhow::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Content",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        _ => "OK",
    };
    let cookie = response
        .set_cookie
        .as_ref()
        .map(|c| format!("Set-Cookie: {c}\r\n"))
        .unwrap_or_default();
    let body_len = response
        .binary
        .as_ref()
        .map_or(response.body.len(), |b| b.len());
    let http = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: SAMEORIGIN\r\nContent-Security-Policy: default-src 'self'; script-src 'self' https://cdn.jsdelivr.net 'unsafe-inline'; style-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; font-src https://cdn.jsdelivr.net; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'\r\n{cookie}Connection: close\r\n\r\n",
        response.status, reason, response.content_type, body_len
    );
    stream.write_all(http.as_bytes()).await?;
    match &response.binary {
        Some(bytes) => stream.write_all(bytes).await?,
        None => stream.write_all(response.body.as_bytes()).await?,
    }
    stream.flush().await?;
    if response.shutdown {
        // Response delivered — hand the port over to the restarted process.
        std::process::exit(0);
    }
    Ok(())
}

/// Pure request handler: (method, path, body, ctx) -> Response. No I/O besides
/// the store, so tests exercise routing against a tempfile DB.
/// Only loopback `Host` values are served: exact match, plus an optional
/// `:port` suffix, so `127.0.0.1.evil.com` fails. A rebound DNS name must
/// not be able to reach this server same-origin from a public page.
fn host_allowed(ctx: &RouteCtx) -> bool {
    ctx.host.as_deref().is_some_and(|h| {
        LOOPBACK_HOSTS.iter().any(|allowed| {
            h == *allowed
                || h.strip_prefix(*allowed)
                    .is_some_and(|rest| rest.starts_with(':'))
        })
    })
}

fn route(method: &str, raw_path: &str, body: Option<&str>, ctx: &RouteCtx) -> Response {
    let (path, query) = split_query(raw_path);
    // The loopback-only Host check defends against DNS rebinding while the
    // server is localhost-only. Once a token is configured the token is the
    // security boundary (rebinding cannot steal it), and legitimate remote
    // browsers arrive with a non-loopback Host, so the check yields.
    if !host_allowed(ctx) && ctx.token.is_none() {
        return Response::error(403, "loopback host required");
    }
    // Token auth: required whenever a token is configured (i.e. the server is
    // reachable beyond loopback). The login page and POST /api/login are the
    // only unauthenticated surfaces.
    let authed = match &ctx.token {
        None => true,
        Some(required) => request_token(ctx).is_some_and(|t| ct_eq(&t, required)),
    };
    if !authed {
        let result = match (method, path) {
            ("POST", "/api/login") => login(body, ctx),
            ("GET", "/" | "/index.html") => Ok(Response::html(LOGIN_HTML.to_string())),
            _ => Ok(Response::error(401, "authentication required")),
        };
        return match result {
            Ok(response) => response,
            Err(e) => {
                tracing::error!(path = %path, error = %e, "login endpoint failed");
                Response::error(500, "internal error")
            }
        };
    }
    let result = match (method, path) {
        ("GET", "/" | "/index.html") => Ok(Response::html(INDEX_HTML.to_string())),
        ("GET", "/api/models") => Ok(Response::error(
            400,
            "models are fetched by the async server path; this route is a placeholder",
        )),
        ("POST", "/api/login") => Ok(Response::error(400, "already authenticated")),
        ("POST", "/api/restart") => restart(ctx.db_override.as_deref()),
        ("GET", "/api/overview") => overview(&ctx.db_path),
        ("GET", "/api/papers") => papers(&ctx.db_path, query),
        ("GET", p) if p.starts_with("/api/papers/") && p.ends_with("/body") => {
            paper_body(&p["/api/papers/".len()..p.len() - "/body".len()], ctx)
        }
        ("GET", p) if p.starts_with("/api/papers/") && p.ends_with("/pdf") => {
            paper_pdf(&p["/api/papers/".len()..p.len() - "/pdf".len()], ctx)
        }
        ("PATCH", p) if p.starts_with("/api/papers/") => {
            patch_paper(&p["/api/papers/".len()..], body, ctx)
        }
        ("GET", "/api/pipeline") => pipeline(&ctx.db_path),
        ("GET", "/api/history") => history(&ctx.db_path),
        ("GET", "/api/results") => results(&ctx.db_path),
        ("GET", "/api/config") => config(&ctx.config_path, &ctx.db_path),
        ("PUT", "/api/config") => put_config(body, ctx),
        (
            _,
            "/" | "/index.html" | "/api/overview" | "/api/papers" | "/api/pipeline"
            | "/api/history" | "/api/results" | "/api/config",
        ) => Ok(Response::error(405, "method not allowed")),
        _ => Ok(Response::error(404, "not found")),
    };
    match result {
        Ok(response) => response,
        Err(e) => {
            tracing::error!(path = %path, error = %e, "dashboard endpoint failed");
            Response::error(500, &format!("store error: {e}"))
        }
    }
}

/// Shared guard for write endpoints: a cross-site form post cannot send a
/// JSON content type without a CORS preflight, which this server never
/// answers, so the Host check plus this rule close the CSRF surface.
fn require_json(ctx: &RouteCtx) -> Result<(), Response> {
    let is_json = ctx
        .content_type
        .as_deref()
        .is_some_and(|ct| ct.split(';').next().unwrap_or("").trim() == "application/json");
    if is_json {
        Ok(())
    } else {
        Err(Response::error(
            415,
            "content-type must be application/json",
        ))
    }
}

fn parse_body(body: Option<&str>) -> Result<Value, Response> {
    body.and_then(|b| serde_json::from_str::<Value>(b).ok())
        .filter(|v| v.is_object())
        .ok_or_else(|| Response::error(400, "body must be a JSON object"))
}

/// Add, change, or remove the `[llm]` section of the config file. The
/// database path is intentionally not writable — this server instance was
/// started against a fixed `--db` path.
fn put_config(body: Option<&str>, ctx: &RouteCtx) -> RouteResult {
    let value = match require_json(ctx).and_then(|_| parse_body(body)) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    let mut config = Config::load(&ctx.config_path)?;

    // Section semantics: key absent = leave untouched, explicit null = remove.
    match value.get("llm") {
        None => {}
        Some(Value::Null) => config.llm = None,
        Some(llm) => {
            let non_empty = |name: &str| -> Result<String, Response> {
                llm.get(name)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .ok_or_else(|| {
                        Response::error(422, &format!("llm.{name} must be a non-empty string"))
                    })
            };
            let (provider, model, api_key_env) = match (
                non_empty("provider"),
                non_empty("model"),
                non_empty("api_key_env"),
            ) {
                (Ok(p), Ok(m), Ok(k)) => (p, m, k),
                (Err(resp), _, _) | (_, Err(resp), _) | (_, _, Err(resp)) => return Ok(resp),
            };
            let base_url = llm
                .get("base_url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from);
            config.llm = Some(crate::config::LlmConfig {
                provider,
                model,
                api_key_env,
                base_url,
            });
        }
    }

    // Workspace: the database path the dashboard (and CLI) should use.
    if let Some(ws) = value
        .get("workspace")
        .and_then(|ws| ws.get("database_path"))
    {
        let path_value = ws;
        {
            let path = path_value.as_str().map(str::trim).filter(|s| !s.is_empty());
            let Some(path) = path else {
                return Ok(Response::error(
                    422,
                    "workspace.database_path must be a non-empty string",
                ));
            };
            if !path.starts_with('~')
                && std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
            {
                return Ok(Response::error(
                    422,
                    "workspace.database_path must point to a file, not a directory",
                ));
            }
            config.database_path = expand_home(path).into();
        }
    }

    // Dashboard: host / port / token. Enabling a non-loopback host without a
    // token would expose the library to the whole network, so one is
    // generated on the spot and returned to the (already authenticated or
    // loopback) caller exactly once.
    if let Some(dash) = value.get("dashboard") {
        let host = dash
            .get("host")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(host) = host {
            if host != "127.0.0.1" && host != "localhost" && host != "0.0.0.0" {
                return Ok(Response::error(
                    422,
                    "dashboard.host must be 127.0.0.1, localhost, or 0.0.0.0",
                ));
            }
            config.dashboard.host = Some(host.to_string());
        }
        if let Some(port) = dash.get("port") {
            match port.as_u64() {
                None => return Ok(Response::error(422, "dashboard.port must be an integer")),
                Some(p) if (1..=65535).contains(&p) => config.dashboard.port = Some(p as u16),
                Some(_) => return Ok(Response::error(422, "dashboard.port out of range")),
            }
        }
        if let Some(token) = dash.get("token") {
            config.dashboard.token = token.as_str().and_then(|t| {
                let t = t.trim();
                (!t.is_empty()).then(|| t.to_string())
            });
        }
        if !config.dashboard.is_loopback() && config.dashboard.token.is_none() {
            config.dashboard.token = Some(uuid::Uuid::new_v4().to_string());
        }
        if config.dashboard.is_loopback() {
            // Loopback never needs the token; dropping it avoids a stale
            // secret lingering in the config file.
            config.dashboard.token = None;
        }
    }

    config.save(&ctx.config_path)?;
    Ok(Response::json(config_payload(&config, &ctx.db_path)))
}

/// Expand a leading `~` using the home directory (config paths accept it).
fn expand_home(path: &str) -> String {
    if let Some((rest, Some(home))) = path.strip_prefix("~/").map(|r| (r, dirs::home_dir())) {
        return home.join(rest).display().to_string();
    }
    path.to_string()
}

fn config_payload(config: &Config, db_path: &Path) -> Value {
    // Only the env-var NAME is exposed, never a resolved key value. The
    // database path reported is the one this server is actually serving,
    // which may differ from the config file when `--db` overrides it.
    json!({
        "database_path": db_path.display().to_string(),
        "configured_database_path": config.database_path.display().to_string(),
        "llm": config.llm.as_ref().map(|llm| json!({
            "provider": llm.provider,
            "model": llm.model,
            "api_key_env": llm.api_key_env,
            "base_url": llm.base_url,
            "api_key_set": llm.resolve_api_key().is_some(),
        })),
        "dashboard": {
            "host": config.dashboard.host_or_default(),
            "port": config.dashboard.port_or_default(),
            "token_set": config.dashboard.token.is_some(),
            "token": config.dashboard.token,
        },
    })
}

/// Extract the presented token from the Authorization header or the
/// `dashboard_token` cookie. Comparison is plain equality; the secret is
/// high-entropy (uuid v4) and the surface is a LAN dashboard, so a timing
/// side channel is not a practical path to it.
fn request_token(ctx: &RouteCtx) -> Option<String> {
    if let Some(token) = ctx
        .authz
        .as_deref()
        .and_then(|authz| authz.strip_prefix("Bearer "))
    {
        return Some(token.trim().to_string());
    }
    ctx.cookie.as_deref().and_then(|cookie| {
        cookie.split(';').find_map(|pair| {
            let pair = pair.trim();
            pair.strip_prefix("dashboard_token=").map(str::to_string)
        })
    })
}

/// Constant-time equality so token checks don't leak the token byte-by-byte
/// through response timing.
fn ct_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Exchange the shared token for a session cookie so browser navigation can
/// carry it without embedding the secret in URLs.
fn login(body: Option<&str>, ctx: &RouteCtx) -> RouteResult {
    let supplied = body
        .and_then(|b| serde_json::from_str::<Value>(b).ok())
        .and_then(|v| v.get("token").and_then(Value::as_str).map(String::from));
    let Some(supplied) = supplied else {
        return Ok(Response::error(403, "invalid token"));
    };
    let Some(required) = &ctx.token else {
        return Ok(Response::error(400, "no token is configured"));
    };
    if supplied != *required {
        return Ok(Response::error(403, "invalid token"));
    }
    let mut response = Response::json(json!({ "ok": true }));
    response.set_cookie = Some(format!(
        "dashboard_token={supplied}; Path=/; HttpOnly; SameSite=Strict; Max-Age=31536000"
    ));
    Ok(response)
}

/// Restart into the freshly saved config: spawn a new process from the same
/// executable, then exit once the 200 response has been flushed. Port is not
/// carried over — the saved config decides it (a CLI `--port` flag is
/// session-local). The `--db` override is re-applied because the config file
/// cannot express it.
fn restart(db_override: Option<&Path>) -> RouteResult {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("dashboard");
    if let Some(db) = db_override {
        cmd.arg("--db").arg(db);
    }
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.stdin(std::process::Stdio::null()).spawn()?;
    let mut response = Response::json(json!({ "ok": true, "restarting": true }));
    response.shutdown = true;
    Ok(response)
}

/// Update a paper's reading progress.
fn patch_paper(id: &str, body: Option<&str>, ctx: &RouteCtx) -> RouteResult {
    if id.is_empty() || id.contains('/') {
        return Ok(Response::error(404, "not found"));
    }
    let value = match require_json(ctx).and_then(|_| parse_body(body)) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };

    let store = SqliteStore::open(&ctx.db_path)?;
    if store.get_paper(id)?.is_none() {
        return Ok(Response::error(404, "paper not found"));
    }

    // Validate both fields before applying either (no partial writes).
    if let Some(status) = value.get("reading_status") {
        let Some(status) = status.as_str() else {
            return Ok(Response::error(422, "reading_status must be a string"));
        };
        if !READING_STATUSES.contains(&status) {
            return Ok(Response::error(
                422,
                &format!(
                    "reading_status must be one of: {}",
                    READING_STATUSES.join(", ")
                ),
            ));
        }
    }
    // `Some(None)` means an explicit `null`: clear the stored rating.
    let rating: Option<Option<crate::domain::paper::Rating>> = match value.get("rating") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => match v
            .as_u64()
            .map(|n| crate::domain::paper::Rating::new(n as u8))
        {
            Some(Ok(r)) => Some(Some(r)),
            Some(Err(_)) | None => {
                return Ok(Response::error(
                    422,
                    "rating must be an integer between 1 and 5, or null to clear",
                ));
            }
        },
    };

    if let Some(status) = value.get("reading_status").and_then(Value::as_str) {
        store.update_reading_status(id, ReadingStatus::from_str_lossy(status))?;
    }
    match rating {
        None => {}
        Some(None) => store.clear_rating(id)?,
        Some(Some(r)) => store.update_rating(id, r)?,
    }
    Ok(Response::json(json!(store.get_paper(id)?)))
}

type RouteResult = anyhow::Result<Response>;

fn overview(db_path: &Path) -> RouteResult {
    let store = SqliteStore::open(db_path)?;
    let topics = store.list_topics()?;
    let papers = store.list_papers(None)?;
    let gaps = store.list_gaps(None)?;
    let reports = store.list_reports(None)?;

    let read = papers
        .iter()
        .filter(|p| p.reading_status == ReadingStatus::Completed)
        .count();
    let queued = papers
        .iter()
        .filter(|p| p.reading_status == ReadingStatus::Queued)
        .count();
    let rated = papers.iter().filter(|p| p.rating.is_some()).count();

    Ok(Response::json(json!({
        "db_path": db_path.display().to_string(),
        "topics": topics.len(),
        "papers": papers.len(),
        "gaps": gaps.len(),
        "reports": reports.len(),
        "read": read,
        "queued": queued,
        "rated": rated,
    })))
}

fn papers(db_path: &Path, query: Option<&str>) -> RouteResult {
    let store = SqliteStore::open(db_path)?;
    let papers = match query_param(query, "topic").filter(|t| !t.is_empty()) {
        Some(topic_id) => store.list_papers_by_topic(&topic_id, None)?,
        None => store.list_papers(None)?,
    };
    Ok(Response::json(json!({ "papers": papers })))
}

/// Value of the first `key=` pair in a raw query string. Percent-decoding is
/// not needed for the one supported parameter (`topic` — UUIDs).
fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}

/// Split `"/p?a=1"` into `("/p", Some("a=1"))`; an empty query counts as none.
fn split_query(raw: &str) -> (&str, Option<&str>) {
    match raw.split_once('?') {
        Some((path, q)) if !q.is_empty() => (path, Some(q)),
        Some((path, _)) => (path, None),
        None => (raw, None),
    }
}

/// Stored full text of one paper. Ingest page anchors (`<!-- page N -->`)
/// are passed through verbatim — the reader renders them as page separators.
fn paper_body(id: &str, ctx: &RouteCtx) -> RouteResult {
    let store = SqliteStore::open(&ctx.db_path)?;
    match store.get_paper_body(id)? {
        Some(body) => Ok(Response::json(json!({ "id": id, "body": body }))),
        None => Ok(Response::error(
            404,
            "no body stored for this paper (ingested without a PDF, or the PDF has moved)",
        )),
    }
}

/// Serve the paper's stored PDF for the reader's built-in viewer. The path
/// comes from the database, never the request, so there is nothing to
/// traverse.
fn paper_pdf(id: &str, ctx: &RouteCtx) -> RouteResult {
    let store = SqliteStore::open(&ctx.db_path)?;
    let Some(paper) = store.get_paper(id)? else {
        return Ok(Response::error(404, "no such paper"));
    };
    let Some(path) = paper.pdf_path else {
        return Ok(Response::error(404, "no PDF stored for this paper"));
    };
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Response::binary("application/pdf", bytes)),
        Err(_) => Ok(Response::error(404, "PDF file missing on disk")),
    }
}

fn pipeline(db_path: &Path) -> RouteResult {
    let store = SqliteStore::open(db_path)?;
    let papers = store.list_papers(None)?;

    let mut paper_status: BTreeMap<&str, usize> = BTreeMap::new();
    let mut reading_status: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &papers {
        *paper_status.entry(p.status.as_str()).or_default() += 1;
        *reading_status.entry(p.reading_status.as_str()).or_default() += 1;
    }

    Ok(Response::json(json!({
        "paper_status": paper_status,
        "paper_status_stages": status_stages(),
        "reading_status": reading_status,
        "reading_status_stages": reading_stages(),
    })))
}

fn status_stages() -> Vec<&'static str> {
    [
        PaperStatus::Discovered,
        PaperStatus::AbstractRead,
        PaperStatus::Skimmed,
        PaperStatus::Read,
        PaperStatus::DeepRead,
    ]
    .iter()
    .map(|s| s.as_str())
    .collect()
}

fn reading_stages() -> Vec<&'static str> {
    [
        ReadingStatus::Unread,
        ReadingStatus::Queued,
        ReadingStatus::InProgress,
        ReadingStatus::Completed,
        ReadingStatus::Abandoned,
    ]
    .iter()
    .map(|s| s.as_str())
    .collect()
}

fn history(db_path: &Path) -> RouteResult {
    let store = SqliteStore::open(db_path)?;
    let papers = store.list_papers(None)?;
    let gaps = store.list_gaps(None)?;
    let reports = store.list_reports(None)?;

    let mut papers_added: Vec<Value> = papers
        .iter()
        .map(|p| {
            json!({
                "date": p.created_at,
                "title": p.title,
                "id": p.id,
                "source": source_of(p),
            })
        })
        .collect();
    papers_added.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));
    papers_added.truncate(HISTORY_LIMIT);

    let mut gaps_found: Vec<Value> = gaps
        .iter()
        .map(|g| {
            json!({
                "date": g.discovered_at,
                "description": g.description,
                "topic_id": g.topic_id,
            })
        })
        .collect();
    gaps_found.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));
    gaps_found.truncate(HISTORY_LIMIT);

    let mut reports_generated: Vec<Value> = reports
        .iter()
        .map(|r| {
            json!({
                "date": r.generated_at,
                "title": r.title,
                "id": r.id,
            })
        })
        .collect();
    reports_generated.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));
    reports_generated.truncate(HISTORY_LIMIT);

    Ok(Response::json(json!({
        "papers_added": papers_added,
        "gaps_found": gaps_found,
        "reports_generated": reports_generated,
    })))
}

/// Best-effort provenance label from which identifier a paper carries.
fn source_of(p: &Paper) -> &'static str {
    if p.arxiv_id.is_some() {
        "arxiv"
    } else if p.s2_id.is_some() {
        "s2"
    } else if p.pdf_path.is_some() {
        "pdf"
    } else {
        "manual"
    }
}

fn results(db_path: &Path) -> RouteResult {
    let store = SqliteStore::open(db_path)?;
    let gaps = store.list_gaps(None)?;
    let reports = store.list_reports(None)?;
    let topics = store.list_topics()?;

    let mut topic_states = Vec::with_capacity(topics.len());
    for t in &topics {
        let state = store.get_research_state(&t.id)?;
        topic_states.push(json!({
            "id": t.id,
            "name": t.name,
            "depth": t.depth,
            "priority": t.priority,
            "state": state,
        }));
    }

    Ok(Response::json(json!({
        "gaps": gaps,
        "reports": reports,
        "topics": topic_states,
    })))
}

fn config(config_path: &Path, db_path: &Path) -> RouteResult {
    let config = Config::load(config_path)?;
    Ok(Response::json(config_payload(&config, db_path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::knowledge_gap::{GapType, KnowledgeGap};
    use crate::domain::paper::Paper;
    use crate::domain::research_report::ResearchReport;
    use crate::domain::research_topic::ResearchTopic;
    use tempfile::TempDir;

    fn ctx_with<F: FnOnce(&SqliteStore)>(seed: F) -> (RouteCtx, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = SqliteStore::open(&db_path).unwrap();
        seed(&store);
        (
            RouteCtx {
                db_path,
                db_override: None,
                config_path: dir.path().join("config.toml"),
                host: Some("127.0.0.1:7777".into()),
                content_type: None,
                authz: None,
                cookie: None,
                token: None,
            },
            dir,
        )
    }

    /// Like `ctx_with` but with a loopback Host bypassed — for host-agnostic checks.
    fn ctx_host(db_path: &Path, host: Option<&str>) -> RouteCtx {
        RouteCtx {
            db_path: db_path.to_path_buf(),
            db_override: None,
            config_path: PathBuf::from("/nonexistent/config.toml"),
            host: host.map(String::from),
            content_type: None,
            authz: None,
            cookie: None,
            token: None,
        }
    }

    fn put_config(ctx: &RouteCtx, body: &str, content_type: Option<&str>) -> Response {
        put_as(ctx, "PUT", "/api/config", body, content_type)
    }

    #[allow(clippy::too_many_arguments)]
    fn put_as(
        ctx: &RouteCtx,
        method: &str,
        path: &str,
        body: &str,
        content_type: Option<&str>,
    ) -> Response {
        let ctx = RouteCtx {
            db_path: ctx.db_path.clone(),
            db_override: ctx.db_override.clone(),
            config_path: ctx.config_path.clone(),
            host: ctx.host.clone(),
            content_type: content_type.map(String::from),
            authz: ctx.authz.clone(),
            cookie: ctx.cookie.clone(),
            token: ctx.token.clone(),
        };
        route(method, path, Some(body), &ctx)
    }

    fn patch(ctx: &RouteCtx, id: &str, body: &str, content_type: Option<&str>) -> Response {
        put_as(
            ctx,
            "PATCH",
            &format!("/api/papers/{id}"),
            body,
            content_type,
        )
    }

    #[test]
    fn root_serves_html() {
        let (ctx, _dir) = ctx_with(|_| {});
        let r = route("GET", "/", None, &ctx);
        assert_eq!(r.status, 200);
        assert!(r.content_type.starts_with("text/html"));
        assert!(r.body.contains("research-agent"));
    }

    #[test]
    fn non_loopback_host_is_rejected() {
        let (ctx, _dir) = ctx_with(|_| {});
        for host in [
            "rebind.evil.com:7777",
            "192.168.1.5:7777",
            // Prefix look-alikes must fail: this is the rebinding bypass class.
            "127.0.0.1.evil.com:7777",
            "localhost.evil.com:7777",
        ] {
            let evil = ctx_host(&ctx.db_path, Some(host));
            assert_eq!(route("GET", "/", None, &evil).status, 403, "host: {host}");
        }
        let no_host = ctx_host(&ctx.db_path, None);
        assert_eq!(route("GET", "/", None, &no_host).status, 403);
        for host in ["127.0.0.1:9999", "localhost", "[::1]:7777"] {
            let ok = ctx_host(&ctx.db_path, Some(host));
            assert_eq!(route("GET", "/nope", None, &ok).status, 404, "host: {host}");
        }
    }

    #[test]
    fn models_endpoint_is_host_guarded() {
        let (ctx, _dir) = ctx_with(|_| {});
        // A rebound Host must not reach the async models proxy — it makes the
        // server spend the configured provider key on the caller's behalf.
        let evil = ctx_host(&ctx.db_path, Some("rebind.evil.com:7777"));
        assert_eq!(route("GET", "/api/models", None, &evil).status, 403);
        let no_host = ctx_host(&ctx.db_path, None);
        assert_eq!(route("GET", "/api/models", None, &no_host).status, 403);
    }

    #[test]
    fn unknown_path_is_404_and_post_is_405() {
        let (ctx, _dir) = ctx_with(|_| {});
        assert_eq!(route("GET", "/nope", None, &ctx).status, 404);
        assert_eq!(route("POST", "/", None, &ctx).status, 405);
    }

    #[test]
    fn overview_counts_seed_data() {
        let (ctx, _dir) = ctx_with(|store| {
            store.insert_paper(&Paper::new("P1".into())).unwrap();
            store
                .insert_topic(&ResearchTopic::new("T1".into()))
                .unwrap();
        });
        let r = route("GET", "/api/overview", None, &ctx);
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["papers"], 1);
        assert_eq!(v["topics"], 1);
        assert_eq!(v["gaps"], 0);
    }

    #[test]
    fn paper_body_endpoint_serves_stored_text_or_404() {
        let mut id = String::new();
        let (ctx, _dir) = ctx_with(|store| {
            let p = Paper::new("Attention Is All You Need".into());
            id = p.id.clone();
            store.insert_paper(&p).unwrap();
            store
                .set_paper_body(&id, "intro text <!-- page 2 --> section two")
                .unwrap();
        });
        let r = route("GET", &format!("/api/papers/{id}/body"), None, &ctx);
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert!(v["body"].as_str().unwrap().contains("<!-- page 2 -->"));
        assert_eq!(
            route("GET", "/api/papers/absent/body", None, &ctx).status,
            404
        );
    }

    #[test]
    fn paper_pdf_endpoint_serves_stored_file_or_404() {
        let mut id = String::new();
        let (ctx, dir) = ctx_with(|store| {
            let p = Paper::new("A PDF Paper".into());
            id = p.id.clone();
            store.insert_paper(&p).unwrap();
        });
        let pdf = dir.path().join("paper.pdf");
        std::fs::write(&pdf, b"%PDF-1.4 fake-bytes").unwrap();
        SqliteStore::open(&ctx.db_path)
            .unwrap()
            .set_paper_pdf_path(&id, &pdf.display().to_string())
            .unwrap();

        let r = route("GET", &format!("/api/papers/{id}/pdf"), None, &ctx);
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "application/pdf");
        assert_eq!(r.binary.as_deref(), Some(b"%PDF-1.4 fake-bytes".as_slice()));

        assert_eq!(
            route("GET", "/api/papers/absent/pdf", None, &ctx).status,
            404
        );

        // File gone since ingest (moved/deleted) — 404, not a panic.
        std::fs::remove_file(&pdf).unwrap();
        assert_eq!(
            route("GET", &format!("/api/papers/{id}/pdf"), None, &ctx).status,
            404
        );
    }

    #[test]
    fn papers_endpoint_lists_seed() {
        let (ctx, _dir) = ctx_with(|store| {
            store
                .insert_paper(&Paper::new("Attention Is All You Need".into()))
                .unwrap();
        });
        let v: Value = serde_json::from_str(&route("GET", "/api/papers", None, &ctx).body).unwrap();
        assert_eq!(v["papers"].as_array().unwrap().len(), 1);
        assert_eq!(v["papers"][0]["title"], "Attention Is All You Need");
    }

    #[test]
    fn papers_endpoint_filters_by_topic_query() {
        let (ctx, _dir) = ctx_with(|store| {
            let topic = ResearchTopic::new("Transformers".into());
            store.insert_topic(&topic).unwrap();
            let mut paper = Paper::new("Topic paper".into());
            paper.abstract_text = "in topic".into();
            store.insert_paper(&paper).unwrap();
            store
                .insert_paper(&Paper::new("Untopiced paper".into()))
                .unwrap();
            store
                .link_paper_to_topic(&paper.id, &topic.id, 0.5)
                .unwrap();
        });
        // Unfiltered keeps listing everything.
        let v: Value = serde_json::from_str(&route("GET", "/api/papers", None, &ctx).body).unwrap();
        assert_eq!(v["papers"].as_array().unwrap().len(), 2);
        // ?topic= scopes the listing to that topic's papers.
        let topic_id = {
            let store = SqliteStore::open(&ctx.db_path).unwrap();
            store.list_topics().unwrap()[0].id.clone()
        };
        let v: Value = serde_json::from_str(
            &route("GET", &format!("/api/papers?topic={topic_id}"), None, &ctx).body,
        )
        .unwrap();
        let papers = v["papers"].as_array().unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0]["title"], "Topic paper");
    }

    #[test]
    fn pipeline_reports_stage_distribution() {
        let (ctx, _dir) = ctx_with(|store| {
            let mut p = Paper::new("A".into());
            p.reading_status = ReadingStatus::Completed;
            store.insert_paper(&p).unwrap();
            store.insert_paper(&Paper::new("B".into())).unwrap();
        });
        let v: Value =
            serde_json::from_str(&route("GET", "/api/pipeline", None, &ctx).body).unwrap();
        assert_eq!(v["paper_status"]["discovered"], 2);
        assert_eq!(v["reading_status"]["completed"], 1);
        assert_eq!(v["reading_status"]["unread"], 1);
    }

    #[test]
    fn history_streams_seed_entries() {
        let (ctx, _dir) = ctx_with(|store| {
            let topic = ResearchTopic::new("T".into());
            store.insert_topic(&topic).unwrap();
            store.insert_paper(&Paper::new("P".into())).unwrap();
            let gap = KnowledgeGap::new(
                "missing optimization literature".into(),
                topic.id.clone(),
                GapType::MissingLiterature,
            );
            store.insert_gap(&gap).unwrap();
            let report = ResearchReport::new("Survey".into(), vec![topic.id.clone()]);
            store.insert_report(&report).unwrap();
        });
        let v: Value =
            serde_json::from_str(&route("GET", "/api/history", None, &ctx).body).unwrap();
        assert_eq!(v["papers_added"].as_array().unwrap().len(), 1);
        assert_eq!(v["gaps_found"].as_array().unwrap().len(), 1);
        assert_eq!(v["reports_generated"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn history_sorts_newest_first() {
        let (ctx, _dir) = ctx_with(|store| {
            let mut old = Paper::new("Old".into());
            old.created_at = "2020-01-01T00:00:00+00:00".into();
            store.insert_paper(&old).unwrap();
            let mut new = Paper::new("New".into());
            new.created_at = "2026-01-01T00:00:00+00:00".into();
            store.insert_paper(&new).unwrap();
        });
        let v: Value =
            serde_json::from_str(&route("GET", "/api/history", None, &ctx).body).unwrap();
        let titles: Vec<&str> = v["papers_added"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, ["New", "Old"]);
    }

    #[test]
    fn store_error_yields_500() {
        let (ctx, _dir) = ctx_with(|_| {});
        let broken = RouteCtx {
            db_path: PathBuf::from("/nonexistent/dir/test.db"),
            ..ctx
        };
        assert_eq!(route("GET", "/api/overview", None, &broken).status, 500);
    }

    #[test]
    fn patch_updates_reading_status_and_rating() {
        let (ctx, _dir) = ctx_with(|store| {
            store.insert_paper(&Paper::new("P".into())).unwrap();
        });
        // Find the real uuid id through the list endpoint.
        let v: Value = serde_json::from_str(&route("GET", "/api/papers", None, &ctx).body).unwrap();
        let id = v["papers"][0]["id"].as_str().unwrap().to_string();

        let r = patch(
            &ctx,
            &id,
            r#"{"reading_status":"queued","rating":4}"#,
            Some("application/json"),
        );
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["reading_status"], "queued");
        assert_eq!(v["rating"], 4);

        // Second patch on one field keeps the other.
        let r = patch(
            &ctx,
            &id,
            r#"{"reading_status":"completed"}"#,
            Some("application/json"),
        );
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["reading_status"], "completed");
        assert_eq!(v["rating"], 4);
    }

    #[test]
    fn patch_null_rating_clears() {
        let (ctx, _dir) = ctx_with(|store| {
            store.insert_paper(&Paper::new("P".into())).unwrap();
        });
        let v: Value = serde_json::from_str(&route("GET", "/api/papers", None, &ctx).body).unwrap();
        let id = v["papers"][0]["id"].as_str().unwrap().to_string();

        let r = patch(&ctx, &id, r#"{"rating":4}"#, Some("application/json"));
        assert_eq!(r.status, 200);

        // Explicit null clears the stored rating.
        let r = patch(&ctx, &id, r#"{"rating":null}"#, Some("application/json"));
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert!(v["rating"].is_null());
    }

    #[test]
    fn patch_rejects_bad_input() {
        let (ctx, _dir) = ctx_with(|store| {
            store.insert_paper(&Paper::new("P".into())).unwrap();
        });
        let v: Value = serde_json::from_str(&route("GET", "/api/papers", None, &ctx).body).unwrap();
        let id = v["papers"][0]["id"].as_str().unwrap().to_string();

        // Wrong content type is refused before touching the store.
        assert_eq!(patch(&ctx, &id, "{}", Some("text/plain")).status, 415);
        assert_eq!(patch(&ctx, &id, "{}", None).status, 415);
        // Malformed / non-object bodies.
        assert_eq!(
            patch(&ctx, &id, "not json", Some("application/json")).status,
            400
        );
        assert_eq!(
            patch(&ctx, &id, "[1]", Some("application/json")).status,
            400
        );
        // Unknown status strings must not silently become `unread`.
        assert_eq!(
            patch(
                &ctx,
                &id,
                r#"{"reading_status":"done"}"#,
                Some("application/json")
            )
            .status,
            422
        );
        // Out-of-range ratings.
        assert_eq!(
            patch(&ctx, &id, r#"{"rating":0}"#, Some("application/json")).status,
            422
        );
        assert_eq!(
            patch(&ctx, &id, r#"{"rating":9}"#, Some("application/json")).status,
            422
        );
        assert_eq!(
            patch(&ctx, &id, r#"{"rating":"4"}"#, Some("application/json")).status,
            422
        );
        // Unknown paper.
        assert_eq!(
            patch(&ctx, "nope", r#"{"rating":3}"#, Some("application/json")).status,
            404
        );
        // Path traversal shape.
        assert_eq!(
            patch(&ctx, "../x", r#"{"rating":3}"#, Some("application/json")).status,
            404
        );

        // Nothing above may have mutated the paper.
        let v: Value = serde_json::from_str(&route("GET", "/api/papers", None, &ctx).body).unwrap();
        assert_eq!(v["papers"][0]["reading_status"], "unread");
        assert!(v["papers"][0]["rating"].is_null());
    }

    #[test]
    fn token_guard_blocks_and_login_issued_cookie() {
        let (ctx, _dir) = ctx_with(|store| {
            store.insert_paper(&Paper::new("P".into())).unwrap();
        });
        let token = "secret-token";
        let with_token = |ctx: &RouteCtx| RouteCtx {
            db_path: ctx.db_path.clone(),
            db_override: ctx.db_override.clone(),
            config_path: ctx.config_path.clone(),
            host: ctx.host.clone(),
            content_type: None,
            authz: ctx.authz.clone(),
            cookie: ctx.cookie.clone(),
            token: Some(token.into()),
        };

        // No credentials: API 401, app HTML replaced by the login page.
        assert_eq!(
            route("GET", "/api/overview", None, &with_token(&ctx)).status,
            401
        );
        let r = route("GET", "/", None, &with_token(&ctx));
        assert_eq!(r.status, 200);
        assert!(r.body.contains("Sign in"));

        // Wrong token stays blocked; login sets an HttpOnly cookie.
        assert_eq!(
            put_as(
                &with_token(&ctx),
                "POST",
                "/api/login",
                r#"{"token":"nope"}"#,
                Some("application/json")
            )
            .status,
            403
        );
        let r = put_as(
            &with_token(&ctx),
            "POST",
            "/api/login",
            &format!(r#"{{"token":"{token}"}}"#),
            Some("application/json"),
        );
        assert_eq!(r.status, 200);
        let cookie = r.set_cookie.expect("cookie issued");
        assert!(cookie.starts_with("dashboard_token="));
        assert!(cookie.contains("HttpOnly"));

        // Bearer header and cookie both authenticate.
        let header_ctx = RouteCtx {
            authz: Some(format!("Bearer {token}")),
            ..with_token(&ctx)
        };
        assert_eq!(route("GET", "/api/overview", None, &header_ctx).status, 200);
        let cookie_ctx = RouteCtx {
            cookie: Some(format!("dashboard_token={token}")),
            ..with_token(&ctx)
        };
        assert_eq!(route("GET", "/api/overview", None, &cookie_ctx).status, 200);
    }

    #[test]
    fn put_config_dashboard_generates_token_for_network_bind() {
        let (ctx, _dir) = ctx_with(|_| {});
        let r = put_config(
            &ctx,
            r#"{"dashboard":{"host":"0.0.0.0","port":8080}}"#,
            Some("application/json"),
        );
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["dashboard"]["host"], "0.0.0.0");
        assert_eq!(v["dashboard"]["port"], 8080);
        assert_eq!(v["dashboard"]["token_set"], true);
        let generated = v["dashboard"]["token"].as_str().unwrap().to_string();
        assert!(generated.len() > 20);

        // Switching back to loopback drops the token.
        let r = put_config(
            &ctx,
            r#"{"dashboard":{"host":"127.0.0.1"}}"#,
            Some("application/json"),
        );
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert!(v["dashboard"]["token"].is_null());

        // Unknown hosts are refused.
        assert_eq!(
            put_config(
                &ctx,
                r#"{"dashboard":{"host":"example.com"}}"#,
                Some("application/json")
            )
            .status,
            422
        );
    }

    #[test]
    fn parse_models_reads_openai_shape() {
        assert_eq!(
            parse_models(r#"{"data":[{"id":"gpt-5.2"},{"id":"gpt-4.1"}]}"#),
            vec!["gpt-4.1".to_string(), "gpt-5.2".to_string()]
        );
        assert!(parse_models("not json").is_empty());
        assert!(parse_models(r#"{"data":[]}"#).is_empty());
    }

    #[test]
    fn put_config_workspace_updates_path() {
        let (ctx, _dir) = ctx_with(|_| {});
        let r = put_config(
            &ctx,
            r#"{"workspace":{"database_path":"/tmp/other.db"}}"#,
            Some("application/json"),
        );
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["configured_database_path"], "/tmp/other.db");
        let saved = std::fs::read_to_string(&ctx.config_path).unwrap();
        assert!(saved.contains("/tmp/other.db"));
    }

    #[test]
    fn put_config_adds_changes_and_removes_llm() {
        let (ctx, _dir) = ctx_with(|_| {});

        // Add.
        let r = put_config(
            &ctx,
            r#"{"llm":{"provider":"anthropic","model":"claude-sonnet-4-6","api_key_env":"MY_KEY"}}"#,
            Some("application/json"),
        );
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["llm"]["provider"], "anthropic");
        assert_eq!(v["llm"]["api_key_set"], false);

        // Persisted to the injected config path as TOML.
        let saved = std::fs::read_to_string(&ctx.config_path).unwrap();
        assert!(saved.contains("provider = \"anthropic\""));

        // Change.
        let r = put_config(
            &ctx,
            r#"{"llm":{"provider":"openai","model":"gpt-5","api_key_env":"OTHER"}}"#,
            Some("application/json"),
        );
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["llm"]["model"], "gpt-5");

        // Remove.
        let r = put_config(&ctx, r#"{"llm":null}"#, Some("application/json"));
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert!(v["llm"].is_null());
        let saved = std::fs::read_to_string(&ctx.config_path).unwrap();
        // A removed section renders as template comments; only a live header
        // (at line start, not "# [llm]") would mean the section survived.
        assert!(!saved.contains("\n[llm]"));
    }

    #[test]
    fn put_config_absent_sections_leave_config_untouched() {
        let (ctx, _dir) = ctx_with(|_| {});
        // Seed llm + dashboard via one PUT.
        put_config(
            &ctx,
            r#"{"llm":{"provider":"p","model":"m","api_key_env":"K"},"dashboard":{"host":"0.0.0.0"}}"#,
            Some("application/json"),
        );
        // A workspace-only save must not remove the llm section.
        let r = put_config(
            &ctx,
            r#"{"workspace":{"database_path":"/tmp/w.db"}}"#,
            Some("application/json"),
        );
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["llm"]["provider"], "p");
        assert_eq!(v["dashboard"]["host"], "0.0.0.0");
        assert_eq!(v["configured_database_path"], "/tmp/w.db");
    }

    #[test]
    fn put_config_rejects_bad_input() {
        let (ctx, _dir) = ctx_with(|_| {});
        assert_eq!(put_config(&ctx, "{}", Some("text/plain")).status, 415);
        assert_eq!(
            put_config(&ctx, "nope", Some("application/json")).status,
            400
        );
        // Missing or empty llm fields.
        assert_eq!(
            put_config(
                &ctx,
                r#"{"llm":{"provider":"x"}}"#,
                Some("application/json")
            )
            .status,
            422
        );
        assert_eq!(
            put_config(
                &ctx,
                r#"{"llm":{"provider":" ","model":"m","api_key_env":"K"}}"#,
                Some("application/json")
            )
            .status,
            422
        );
        // Config::load may create the default file, but no llm section is written.
        let saved = std::fs::read_to_string(&ctx.config_path).unwrap_or_default();
        // A removed section renders as template comments; only a live header
        // (at line start, not "# [llm]") would mean the section survived.
        assert!(!saved.contains("\n[llm]"));
    }

    #[test]
    fn results_include_gaps_reports_topics() {
        let (ctx, _dir) = ctx_with(|store| {
            let topic = ResearchTopic::new("T".into());
            store.insert_topic(&topic).unwrap();
            let report = ResearchReport::new("Report".into(), vec![topic.id.clone()]);
            store.insert_report(&report).unwrap();
        });
        let v: Value =
            serde_json::from_str(&route("GET", "/api/results", None, &ctx).body).unwrap();
        assert_eq!(v["topics"].as_array().unwrap().len(), 1);
        assert_eq!(v["reports"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn config_endpoint_omits_secret_values() {
        let (ctx, _dir) = ctx_with(|_| {});
        let v: Value = serde_json::from_str(&route("GET", "/api/config", None, &ctx).body).unwrap();
        let body = v.to_string();
        // Never a resolved key — at most the env var name.
        assert!(!body.contains("sk-"));
        assert!(v["database_path"].is_string());
    }
}
