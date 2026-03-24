use std::collections::HashMap;

use crate::error::{CopernicusError, Result};

const AUTH_URL: &str =
    "https://identity.dataspace.copernicus.eu/auth/realms/CDSE/protocol/openid-connect/token";

/// Obtain an OAuth2 access token from the CDSE identity provider.
///
/// # `&str` vs `String` -- the big Rust concept
///
/// - `&str` is a **borrowed** reference to text.  It doesn't own the data.
///   Think of it as a read-only view into someone else's string.
/// - `String` is an **owned** heap-allocated string.  You can modify it,
///   move it, return it from functions.
///
/// Here we take `&str` parameters because we only need to *read* the
/// username and password.  We return a `String` because the caller will
/// own the token.
///
/// # The `?` operator
///
/// Every line ending with `?` means: "if this returns an `Err`, return
/// that error immediately from the whole function."  It replaces verbose
/// `match` / `if let` chains and is the idiomatic way to propagate errors
/// in Rust.  It works because our `CopernicusError` has `#[from]` impls
/// for `reqwest::Error` and `serde_json::Error`.
pub fn get_access_token(username: &str, password: &str) -> Result<String> {
    let mut params = HashMap::new();
    params.insert("client_id", "cdse-public");
    params.insert("username", username);
    params.insert("password", password);
    params.insert("grant_type", "password");

    let client = reqwest::blocking::Client::new();
    let response = client.post(AUTH_URL).form(&params).send()?;

    if !response.status().is_success() {
        return Err(CopernicusError::AuthenticationFailed(format!(
            "HTTP {status}",
            status = response.status(),
        )));
    }

    let body: serde_json::Value = response.json()?;

    body["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| {
            CopernicusError::AuthenticationFailed(
                "response did not contain an access_token field".into(),
            )
        })
}

/// Convenience: read credentials from environment variables and authenticate.
///
/// Expects `COPERNICUS_USER` and `COPERNICUS_PASS` to be set.
///
/// `std::env::var` returns `Result<String, VarError>`.  We convert the
/// error into our `CopernicusError::AuthenticationFailed` using `map_err`.
pub fn get_access_token_from_env() -> Result<String> {
    let username = std::env::var("COPERNICUS_USER").map_err(|_| {
        CopernicusError::AuthenticationFailed("COPERNICUS_USER environment variable not set".into())
    })?;
    let password = std::env::var("COPERNICUS_PASS").map_err(|_| {
        CopernicusError::AuthenticationFailed("COPERNICUS_PASS environment variable not set".into())
    })?;

    get_access_token(&username, &password)
}
