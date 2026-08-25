// SPDX-FileCopyrightText: 2023-2026 Sayantan Santra <sayantan.santra689@gmail.com>
// SPDX-License-Identifier: MIT

use argon2::{Argon2, PasswordVerifier, password_hash::PasswordHash};
use log::{debug, warn};
use passwords::PasswordGenerator;
use rocket::Request;
use rocket::request::{FromRequest, Outcome};
use std::rc::Rc;
use std::time::SystemTime;

use crate::{
    AppState,
    config::{Config, HashAlgorithm},
    services::types::JSONResponse,
};

// Read API key from header and process it
fn is_api_ok(api_header: Option<&str>, config: &Config) -> JSONResponse {
    // If the api_key environment variable exists
    if config.api_key.is_some() {
        // If the header exists
        if let Some(header) = api_header {
            // If the header is correct
            if is_key_valid(header, config) {
                JSONResponse {
                    success: true,
                    error: false,
                    reason: "Correct API key.".to_owned(),
                }
            } else {
                JSONResponse {
                    success: false,
                    error: true,
                    reason: "API validation failed.".to_owned(),
                }
            }
        // The header may not exist when the user logs in through the web interface, so allow a request with no header.
        // Further authentication checks will be conducted in services.rs
        } else {
            // Due to the implementation of this result in services.rs, this JSON object will not be outputted.
            JSONResponse {
                success: false,
                error: false,
                reason: "No valid authentication.".to_owned(),
            }
        }
    } else {
        // If the API key isn't set, but an API Key header is provided
        if api_header.is_some() {
            JSONResponse {
                success: false,
                error: true,
                reason: "API validation failed.".to_owned(),
            }
        } else {
            JSONResponse {
                success: false,
                error: false,
                reason: "No valid authentication.".to_owned(),
            }
        }
    }
}

// Validate API key
fn is_key_valid(key: &str, config: &Config) -> bool {
    if let Some(api_key) = &config.api_key {
        // Check if API Key is hashed using Argon2. More algorithms maybe added later.
        let authorized = match config.hash_algorithm {
            HashAlgorithm::Argon2 => {
                debug!("Using Argon2 hash for API key validation.");
                let hash =
                    PasswordHash::new(api_key).expect("The provided password hash is invalid.");
                Argon2::default()
                    .verify_password(key.as_bytes(), &hash)
                    .is_ok()
            }
            HashAlgorithm::None => {
                // If hashing is not enabled, use the plaintext API key for matching
                api_key == key
            }
        };
        if !authorized {
            warn!("Incorrect API key was provided.");
            false
        } else {
            debug!("Server accessed with API key.");
            true
        }
    } else {
        warn!("API was accessed with API key validation but no API key was configured.");
        false
    }
}

// Generate an API key if the user doesn't specify a secure key
pub(crate) fn gen_key() -> String {
    let key = PasswordGenerator {
        length: 128,
        numbers: true,
        lowercase_letters: true,
        uppercase_letters: true,
        symbols: false,
        spaces: false,
        exclude_similar_characters: false,
        strict: true,
    };
    key.generate_one().unwrap()
}

// Validate a session
fn is_session_valid(token: Option<&str>, config: &Config) -> bool {
    // If there's no password provided, just return true
    if config.password.is_none() {
        return true;
    }

    is_token_valid(token)
}

// Check a token cryptographically
fn is_token_valid(token: Option<&str>) -> bool {
    if let Some(token_body) = token {
        let token_parts: Rc<[&str]> = token_body.split(';').collect();
        if token_parts.len() < 2 {
            false
        } else {
            let token_text = token_parts[0];
            let token_expiry_time = token_parts[1].parse::<u64>().unwrap_or(0);
            let time_now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("Time went backwards!")
                .as_secs();
            token_text == "chhoto-url-auth" && time_now < token_expiry_time
            // 7 days
        }
    } else {
        false
    }
}

// Enum for auth state
pub(crate) enum Auth {
    ValidAPIKey,
    ValidSession,
    None { result: JSONResponse },
    InvalidAPIKey { result: JSONResponse },
}

// Extractor for authentication
#[rocket::async_trait]
impl<'r> FromRequest<'r> for Auth {
    type Error = std::convert::Infallible;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let config = &req
            .rocket()
            .state::<AppState>()
            .expect("Appstate wasn't created yet. THIS SHOULD NEVER OCCUR!!!")
            .config;

        // API key auth
        let api_header = req.headers().get_one("X-API-Key");
        let api_result = is_api_ok(api_header, config);
        if api_result.success {
            return Outcome::Success(Auth::ValidAPIKey);
        } else if api_result.error {
            return Outcome::Success(Auth::InvalidAPIKey { result: api_result });
        }

        // Session auth
        let token = req
            .cookies()
            .get_private("chhoto-url-auth")
            .map(|cookie| cookie.value().to_owned());
        if is_session_valid(token.as_deref(), config) {
            return Outcome::Success(Auth::ValidSession);
        }

        Outcome::Success(Auth::None { result: api_result })
    }
}

// Generate a new token for usage in cookie
pub(crate) fn gen_token_text() -> String {
    let token_text = String::from("chhoto-url-auth");
    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Time went backwards!")
        .as_secs()
        + 604800; // Valid for 7 days
    format!("{token_text};{time}")
}
