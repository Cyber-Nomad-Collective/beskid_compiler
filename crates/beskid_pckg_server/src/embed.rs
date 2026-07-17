//! Public pckg embedding compatibility endpoints.

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use beskid_pckg_artifacts::{ArtifactRecord, select_download};
use serde::Deserialize;

use crate::AppState;

const CACHE_CONTROL: &str = "public, max-age=120";
const HTML_NOT_FOUND: &str = "<!DOCTYPE html><html><body><p>Package not found.</p></body></html>";
const CARD_STYLE: &str = "<style>:root{color-scheme:light dark}body{margin:0;font-family:system-ui,-apple-system,Segoe UI,Roboto,Ubuntu,Cantarell,Helvetica Neue,Arial,sans-serif;background:#f5f5f5;color:#111}@media(prefers-color-scheme:dark){body{background:#1a1a1a;color:#f3f3f3}a{color:#6cb6ff}}.card{box-sizing:border-box;max-width:420px;margin:0 auto;padding:12px 14px;border-radius:8px;border:1px solid rgba(127,127,127,.35);background:rgba(255,255,255,.92);box-shadow:0 1px 2px rgba(0,0,0,.06)}@media(prefers-color-scheme:dark){.card{background:rgba(40,40,40,.95)}}.brand{font-size:11px;letter-spacing:.04em;text-transform:uppercase;opacity:.75;margin-bottom:6px}h1{font-size:16px;margin:0 0 6px;line-height:1.25}p.meta{margin:0 0 8px;font-size:13px;opacity:.9}p.desc{margin:0 0 10px;font-size:13px;line-height:1.35;opacity:.92}.row{display:flex;align-items:center;justify-content:space-between;gap:10px;flex-wrap:wrap}img.badge{height:20px;display:block}a.pkg{text-decoration:none;color:inherit}a.pkg:hover{text-decoration:underline}</style>";

#[derive(Debug, Deserialize)]
pub(crate) struct EmbedQuery {
    package: Option<String>,
}

/// The badge intentionally returns a neutral SVG for absent and private
/// packages. This preserves README rendering while making private visibility
/// indistinguishable from absence.
pub(crate) async fn badge(
    State(state): State<AppState>,
    Query(query): Query<EmbedQuery>,
) -> Response {
    let Some(package) = public_package(&state, query.package.as_deref()).await else {
        return svg_response(not_found_badge());
    };
    let latest = latest_version(&state, &package.id).await.unwrap_or(None);
    svg_response(build_badge(&package.name, latest.as_deref()))
}

/// The iframe card only exposes public metadata. It deliberately gives the
/// same not-found document for an absent, malformed, or private package.
pub(crate) async fn card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EmbedQuery>,
) -> Response {
    let Some(package) = public_package(&state, query.package.as_deref()).await else {
        return html_not_found();
    };
    let latest = latest_version(&state, &package.id).await.unwrap_or(None);
    let origin = request_origin(&headers);
    let body = build_card(&origin, &package.name, latest.as_deref());
    let mut response = Response::new(body.into());
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static(CACHE_CONTROL),
    );
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        header::HeaderValue::from_static("frame-ancestors *"),
    );
    response
}

async fn public_package(
    state: &AppState,
    name: Option<&str>,
) -> Option<beskid_pckg_store::Package> {
    let name = name?.trim();
    if name.is_empty() {
        return None;
    }
    state
        .packages
        .find_package(name)
        .await
        .ok()?
        .filter(|package| package.is_public)
}

async fn latest_version(state: &AppState, package_id: &str) -> Result<Option<String>, ()> {
    let versions = state
        .packages
        .list_versions(package_id)
        .await
        .map_err(|_| ())?;
    let records = versions
        .iter()
        .map(|version| ArtifactRecord::new(&version.version, version.is_yanked))
        .collect::<Vec<_>>();
    Ok(select_download(&records, "latest").map(|record| record.version.clone()))
}

fn svg_response(body: String) -> Response {
    let mut response = Response::new(body.into());
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("image/svg+xml; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static(CACHE_CONTROL),
    );
    response
}

fn html_not_found() -> Response {
    let mut response = Response::new(HTML_NOT_FOUND.into());
    *response.status_mut() = StatusCode::NOT_FOUND;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

fn request_origin(headers: &HeaderMap) -> String {
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| matches!(*value, "http" | "https"))
        .unwrap_or("http");
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|host| !host.is_empty() && host.bytes().all(is_safe_host_byte))
        .unwrap_or("localhost");
    format!("{scheme}://{host}")
}

fn is_safe_host_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b'-' | b'[' | b']')
}

fn badge_url(origin: &str, name: &str) -> String {
    format!(
        "{origin}/api/embed/badge.svg?package={}",
        percent_encode(name)
    )
}

fn package_url(origin: &str, name: &str) -> String {
    format!("{origin}/packages/{}", percent_encode(name))
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write as _;
            write!(encoded, "%{byte:02X}").expect("writing into String cannot fail");
        }
    }
    encoded
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn build_card(origin: &str, name: &str, latest: Option<&str>) -> String {
    let name_html = escape_html(name);
    let package_url = escape_html(&package_url(origin, name));
    let badge_url = escape_html(&badge_url(origin, name));
    let version_line = latest
        .map(|version| format!("Latest: <strong>{}</strong>", escape_html(version)))
        .unwrap_or_else(|| "Latest: <strong>none published</strong>".to_owned());
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"/><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/><meta name=\"robots\" content=\"noindex\"/><base target=\"_blank\" rel=\"noopener noreferrer\"/>{CARD_STYLE}</head><body><article class=\"card\"><div class=\"brand\">Beskid registry</div><h1><a class=\"pkg\" href=\"{package_url}\">{name_html}</a></h1><p class=\"meta\">{version_line} · 0 downloads</p><p class=\"desc\">Beskid package on the registry.</p><div class=\"row\"><a href=\"{badge_url}\"><img class=\"badge\" src=\"{badge_url}\" alt=\"pckg registry badge\"/></a><a href=\"{package_url}\">View package →</a></div></article></body></html>"
    )
}

fn not_found_badge() -> String {
    build_badge_parts("not found", "#9a9a9a")
}

fn build_badge(name: &str, latest: Option<&str>) -> String {
    let display_name = if name.chars().count() > 36 {
        format!("{}…", name.chars().take(33).collect::<String>())
    } else {
        name.to_owned()
    };
    let latest = latest.unwrap_or("no release").trim();
    build_badge_parts(&format!("{display_name} · {latest}"), "#007ec6")
}

fn build_badge_parts(right_text: &str, right_fill: &str) -> String {
    const LEFT: &str = "pckg";
    const PADDING: f64 = 10.0;
    const CHAR_WIDTH: f64 = 6.2;
    let left_width = (LEFT.chars().count() as f64 * CHAR_WIDTH + PADDING * 2.0).ceil() as usize;
    let right_width =
        (right_text.chars().count() as f64 * CHAR_WIDTH + PADDING * 2.0).ceil() as usize;
    let total_width = left_width + right_width;
    let text = escape_html(right_text);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total_width}\" height=\"20\" role=\"img\" aria-label=\"pckg {text}\"><title>pckg {text}</title><linearGradient id=\"a\" x2=\"0\" y2=\"100%\"><stop offset=\"0\" stop-color=\"#bbb\" stop-opacity=\".1\"/><stop offset=\"1\" stop-opacity=\".1\"/></linearGradient><rect rx=\"3\" width=\"{total_width}\" height=\"20\" fill=\"#555\"/><rect rx=\"3\" x=\"{left_width}\" width=\"{right_width}\" height=\"20\" fill=\"{right_fill}\"/><rect rx=\"3\" width=\"{total_width}\" height=\"20\" fill=\"url(#a)\"/><g fill=\"#fff\" text-anchor=\"middle\" font-family=\"DejaVu Sans,Verdana,Geneva,sans-serif\" font-size=\"11\"><text x=\"{}\" y=\"14\" fill=\"#010101\" fill-opacity=\".3\">pckg</text><text x=\"{}\" y=\"13\">pckg</text><text x=\"{}\" y=\"14\" fill=\"#010101\" fill-opacity=\".3\">{text}</text><text x=\"{}\" y=\"13\">{text}</text></g></svg>",
        left_width as f64 / 2.0,
        left_width as f64 / 2.0,
        left_width as f64 + right_width as f64 / 2.0,
        left_width as f64 + right_width as f64 / 2.0,
    )
}
