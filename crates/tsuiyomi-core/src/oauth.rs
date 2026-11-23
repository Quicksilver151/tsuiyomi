//! Browser OAuth for AniList and MyAnimeList.
//!
//! The flow uses a loopback HTTP callback and blocks the caller until the user finishes
//! in the browser. The tracker worker owns that blocking call so the UI stays responsive.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{config::TrackerAuth, error::CoreError};

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerService {
    AniList,
    MyAnimeList,
}

impl TrackerService {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "anilist" => Some(Self::AniList),
            "mal" => Some(Self::MyAnimeList),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::AniList => "anilist",
            Self::MyAnimeList => "mal",
        }
    }
}

pub fn login(
    service: TrackerService,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<TrackerAuth, CoreError> {
    let client_id = client_id.trim();
    if client_id.is_empty() {
        return Err(oauth_error("a client ID is required"));
    }
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|error| oauth_error(error.to_string()))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| oauth_error(error.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|error| oauth_error(error.to_string()))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let state = random_token()?;
    let verifier = random_token()?;
    let challenge = pkce_challenge(&verifier);
    let authorize = authorize_url(service, client_id, &redirect_uri, &state, &challenge);
    open_browser(&authorize)?;
    let code = wait_for_code(&listener, &state)?;
    exchange_code(
        service,
        client_id,
        client_secret,
        &redirect_uri,
        &code,
        &verifier,
    )
}

pub fn refresh(
    service: TrackerService,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<TrackerAuth, CoreError> {
    let client_id = client_id.trim();
    if client_id.is_empty() {
        return Err(oauth_error("a client ID is required"));
    }
    let refresh_token = refresh_token.trim();
    if refresh_token.is_empty() {
        return Err(oauth_error("no refresh token is available"));
    }
    match service {
        TrackerService::AniList => Err(oauth_error(
            "AniList does not issue refresh tokens in this flow; log in again",
        )),
        TrackerService::MyAnimeList => {
            let mut form = vec![
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", client_id),
            ];
            if let Some(secret) = client_secret.filter(|secret| !secret.trim().is_empty()) {
                form.push(("client_secret", secret));
            }
            let value = post_form("https://myanimelist.net/v1/oauth2/token", &form)?;
            parse_tokens(&value)
        }
    }
}

fn authorize_url(
    service: TrackerService,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> String {
    let mut url = match service {
        TrackerService::AniList => "https://anilist.co/api/v2/oauth/authorize",
        TrackerService::MyAnimeList => "https://myanimelist.net/v1/oauth2/authorize",
    }
    .to_owned();
    url.push('?');
    let mut params = vec![
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("state", state),
    ];
    if service == TrackerService::MyAnimeList {
        params.push(("code_challenge", challenge));
        params.push(("code_challenge_method", "S256"));
    }
    url.push_str(
        &params
            .into_iter()
            .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
            .collect::<Vec<_>>()
            .join("&"),
    );
    url
}

fn exchange_code(
    service: TrackerService,
    client_id: &str,
    client_secret: Option<&str>,
    redirect_uri: &str,
    code: &str,
    verifier: &str,
) -> Result<TrackerAuth, CoreError> {
    match service {
        TrackerService::AniList => {
            let body = json!({
                "grant_type": "authorization_code",
                "client_id": client_id,
                "client_secret": client_secret.unwrap_or_default(),
                "redirect_uri": redirect_uri,
                "code": code,
            });
            let value = post_json("https://anilist.co/api/v2/oauth/token", &body)?;
            parse_tokens(&value)
        }
        TrackerService::MyAnimeList => {
            let mut form = vec![
                ("grant_type", "authorization_code"),
                ("client_id", client_id),
                ("code", code),
                ("code_verifier", verifier),
                ("redirect_uri", redirect_uri),
            ];
            if let Some(secret) = client_secret.filter(|secret| !secret.trim().is_empty()) {
                form.push(("client_secret", secret));
            }
            let value = post_form("https://myanimelist.net/v1/oauth2/token", &form)?;
            parse_tokens(&value)
        }
    }
}

fn parse_tokens(value: &Value) -> Result<TrackerAuth, CoreError> {
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        let detail = value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or(error);
        return Err(oauth_error(detail.to_owned()));
    }
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| oauth_error("the tracker did not return an access token"))?
        .to_owned();
    let refresh_token = value
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_owned);
    let expires_in = value.get("expires_in").and_then(Value::as_u64);
    Ok(TrackerAuth {
        access_token,
        refresh_token,
        expires_at: expires_in.map(|seconds| unix_seconds().saturating_add(seconds)),
        account_id: None,
    })
}

fn wait_for_code(listener: &TcpListener, expected_state: &str) -> Result<String, CoreError> {
    let deadline = Instant::now() + CALLBACK_TIMEOUT;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(code) = handle_callback(stream, expected_state)? {
                    return Ok(code);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(oauth_error(error.to_string())),
        }
    }
    Err(oauth_error("timed out waiting for the browser login"))
}

fn handle_callback(
    mut stream: TcpStream,
    expected_state: &str,
) -> Result<Option<String>, CoreError> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| oauth_error(error.to_string()))?;
    let mut buffer = [0u8; 8192];
    let read = stream
        .read(&mut buffer)
        .map_err(|error| oauth_error(error.to_string()))?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default();
    let Some(query) = target.split_once('?').map(|(_, query)| query) else {
        respond(&mut stream, "Waiting for login…");
        return Ok(None);
    };
    let params = parse_query(query);
    if params
        .get("state")
        .is_none_or(|state| state != expected_state)
    {
        respond(&mut stream, "Login failed: state mismatch.");
        return Err(oauth_error("OAuth state mismatch"));
    }
    if let Some(error) = params.get("error") {
        respond(&mut stream, "Login failed.");
        return Err(oauth_error(error.clone()));
    }
    let Some(code) = params.get("code") else {
        respond(&mut stream, "Login failed: no code.");
        return Err(oauth_error(
            "the tracker did not return an authorization code",
        ));
    };
    respond(&mut stream, "Login complete. You can close this tab.");
    Ok(Some(code.clone()))
}

fn respond(stream: &mut TcpStream, message: &str) {
    let body = format!("<!doctype html><title>tsuiyomi</title><p>{message}</p>");
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn post_json(url: &str, body: &Value) -> Result<Value, CoreError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();
    let response = agent
        .post(url)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .send_json(body.clone())
        .map_err(|error| oauth_error(error.to_string()))?;
    response
        .into_json()
        .map_err(|error| oauth_error(error.to_string()))
}

fn post_form(url: &str, form: &[(&str, &str)]) -> Result<Value, CoreError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();
    let response = agent
        .post(url)
        .set("Accept", "application/json")
        .send_form(form)
        .map_err(|error| oauth_error(error.to_string()))?;
    response
        .into_json()
        .map_err(|error| oauth_error(error.to_string()))
}

fn open_browser(url: &str) -> Result<(), CoreError> {
    let (command, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[url])
    } else if cfg!(target_os = "windows") {
        ("cmd", &["/C", "start", "", url])
    } else {
        ("xdg-open", &[url])
    };
    Command::new(command)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| oauth_error(format!("could not open the browser: {error}")))
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((decode(key), decode(value)))
        })
        .collect()
}

fn pkce_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn random_token() -> Result<String, CoreError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| oauth_error(error.to_string()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        if bytes[index] == b'+' {
            decoded.push(b' ');
        } else {
            decoded.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn oauth_error(message: impl Into<String>) -> CoreError {
    CoreError::Network {
        url: "tracker OAuth".to_owned(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_rfc_vector() {
        // RFC 7636 appendix B.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn parses_callback_query() {
        let params = parse_query("code=abc%2Fdef&state=xyz&empty=");
        assert_eq!(params.get("code").map(String::as_str), Some("abc/def"));
        assert_eq!(params.get("state").map(String::as_str), Some("xyz"));
    }

    #[test]
    fn authorize_urls_include_expected_parameters() {
        let url = authorize_url(
            TrackerService::MyAnimeList,
            "client",
            "http://127.0.0.1:1234/callback",
            "state",
            "challenge",
        );
        assert!(url.contains("code_challenge=challenge"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A1234%2Fcallback"));
    }
}
