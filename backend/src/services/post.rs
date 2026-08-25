// SPDX-FileCopyrightText: 2023-2026 Sayantan Santra <sayantan.santra689@gmail.com>
// SPDX-License-Identifier: MIT

use argon2::{Argon2, PasswordVerifier, password_hash::PasswordHash};
use log::{debug, info, warn};
use rocket::State;
use rocket::http::{ContentType, CookieJar, Status};
use rocket::post;

use crate::{
    AppState,
    auth::{self, Auth},
    config::HashAlgorithm,
    database,
    services::types::{
        AddLinkResponse,
        ChhotoError::{ClientError, ServerError},
        CreatedURL, JSONResponse, LinkInfo,
    },
    utils,
};

const SERVER_ERROR_RES: &str = "Something went wrong when adding the link.";

// Add new links
#[post("/api/new", data = "<req>")]
pub(crate) async fn add_links(
    req: String,
    auth: Auth,
    data: &State<AppState>,
) -> (Status, (ContentType, String)) {
    let config = &data.config;
    let cookie_response = async |public_mode| {
        let result =
            utils::add_links_helper(&req, &mut *data.writer.lock().await, config, public_mode)
                .and_then(|(v, _)| v.into_iter().next().unwrap_or(Err(ServerError)));
        match result {
            Ok((shorturl, _)) => (Status::Created, (ContentType::Plain, shorturl)),
            Err(ClientError { reason }) => (Status::Conflict, (ContentType::Plain, reason)),
            Err(ServerError) => (
                Status::InternalServerError,
                (ContentType::Plain, SERVER_ERROR_RES.to_owned()),
            ),
        }
    };
    match auth {
        Auth::ValidAPIKey => {
            let to_response = |res| match res {
                Ok((shortlink, expiry_time)) => {
                    let site_url = config.site_url.to_owned();
                    let shorturl = if let Some(url) = &site_url {
                        format!("{url}/{shortlink}")
                    } else {
                        let protocol = if config.port == 443 { "https" } else { "http" };
                        let port_text = if [80, 443].contains(&config.port) {
                            String::new()
                        } else {
                            format!(":{}", config.port)
                        };
                        format!("{protocol}://localhost{port_text}/{shortlink}")
                    };

                    (
                        Status::Ok,
                        AddLinkResponse::Success(CreatedURL {
                            success: true,
                            error: false,
                            shorturl,
                            expiry_time,
                        }),
                    )
                }
                Err(ClientError { reason }) => (
                    Status::BadRequest,
                    AddLinkResponse::Error(JSONResponse {
                        success: false,
                        error: true,
                        reason,
                    }),
                ),
                Err(ServerError) => (
                    Status::InternalServerError,
                    AddLinkResponse::Error(JSONResponse {
                        success: false,
                        error: true,
                        reason: SERVER_ERROR_RES.to_owned(),
                    }),
                ),
            };

            match utils::add_links_helper(&req, &mut *data.writer.lock().await, config, false) {
                Ok((reply, single_request)) => {
                    if single_request {
                        let (status, response) = to_response(
                            reply
                                .into_iter()
                                .next()
                                .expect("There should be one response here."),
                        );
                        let body = serde_json::to_string(&response).unwrap_or_default();
                        (status, (ContentType::JSON, body))
                    } else {
                        let response: Vec<_> =
                            reply.into_iter().map(to_response).map(|(_, r)| r).collect();
                        let body = serde_json::to_string(&response).unwrap_or_default();
                        (Status::Ok, (ContentType::JSON, body))
                    }
                }
                Err(error) => {
                    let (status, response) = to_response(Err(error));
                    let body = serde_json::to_string(&response).unwrap_or_default();
                    (status, (ContentType::JSON, body))
                }
            }
        }
        Auth::InvalidAPIKey { result } => (
            Status::Unauthorized,
            (
                ContentType::JSON,
                serde_json::to_string(&result).unwrap_or_default(),
            ),
        ),
        // If password authentication or public mode is used - keeps backwards compatibility
        Auth::ValidSession => cookie_response(false).await,
        Auth::None { result: _ } => {
            if data.config.public_mode {
                cookie_response(true).await
            } else {
                (
                    Status::Unauthorized,
                    (ContentType::Plain, "Not logged in!".to_owned()),
                )
            }
        }
    }
}

// Get information about a single shortlink
#[post("/api/expand", data = "<req>")]
pub(crate) async fn expand(
    req: String,
    auth: Auth,
    data: &State<AppState>,
) -> (Status, (ContentType, String)) {
    match auth {
        Auth::ValidAPIKey => {
            let result = database::find_url(&req, &*data.reader.lock().await);
            match result {
                Ok(chunks) => {
                    let body = LinkInfo {
                        success: true,
                        error: false,
                        longurl: chunks.longlink,
                        hits: chunks.hits,
                        expiry_time: chunks.expiry_time,
                        notes: chunks.notes,
                    };
                    (
                        Status::Ok,
                        (
                            ContentType::JSON,
                            serde_json::to_string(&body).unwrap_or_default(),
                        ),
                    )
                }
                Err(ServerError) => {
                    let body = JSONResponse {
                        success: false,
                        error: true,
                        reason: SERVER_ERROR_RES.to_owned(),
                    };
                    (
                        Status::BadRequest,
                        (
                            ContentType::JSON,
                            serde_json::to_string(&body).unwrap_or_default(),
                        ),
                    )
                }
                Err(ClientError { reason }) => {
                    let body = JSONResponse {
                        success: false,
                        error: true,
                        reason,
                    };
                    (
                        Status::BadRequest,
                        (
                            ContentType::JSON,
                            serde_json::to_string(&body).unwrap_or_default(),
                        ),
                    )
                }
            }
        }
        Auth::ValidSession => {
            let body = JSONResponse {
                success: false,
                error: true,
                reason: "This route needs API auth.".to_owned(),
            };
            (
                Status::Unauthorized,
                (
                    ContentType::JSON,
                    serde_json::to_string(&body).unwrap_or_default(),
                ),
            )
        }
        Auth::None { result } | Auth::InvalidAPIKey { result } => (
            Status::Unauthorized,
            (
                ContentType::JSON,
                serde_json::to_string(&result).unwrap_or_default(),
            ),
        ),
    }
}

// Handle login
#[post("/api/login", data = "<req>")]
pub(crate) async fn login(
    auth: Auth,
    req: String,
    cookies: &CookieJar<'_>,
    data: &State<AppState>,
) -> (Status, (ContentType, String)) {
    let config = &data.config;
    if matches!(auth, Auth::ValidSession) {
        return (
            Status::Ok,
            (ContentType::Plain, "Already authorized.".to_owned()),
        );
    }

    // Check if password is hashed using Argon2. More algorithms maybe added later.
    let authorized = if let Some(password) = &config.password {
        match config.hash_algorithm {
            HashAlgorithm::Argon2 => {
                debug!("Using Argon2 hash for password validation.");
                let hash =
                    PasswordHash::new(password).expect("The provided password hash is invalid.");
                Some(
                    Argon2::default()
                        .verify_password(req.as_bytes(), &hash)
                        .is_ok(),
                )
            }
            HashAlgorithm::None => {
                // If hashing is not enabled, use the plaintext password for matching
                Some(password == &req)
            }
        }
    } else {
        None
    };
    if config.api_key.is_some() {
        if let Some(valid_pass) = authorized
            && !valid_pass
        {
            warn!("Failed login attempt!");
            let response = JSONResponse {
                success: false,
                error: true,
                reason: "Wrong password!".to_owned(),
            };
            return (
                Status::Unauthorized,
                (
                    ContentType::JSON,
                    serde_json::to_string(&response).unwrap_or_default(),
                ),
            );
        }
        // Return Ok if no password was set on the server side
        cookies.add_private(rocket::http::Cookie::new(
            "chhoto-url-auth",
            auth::gen_token_text(),
        ));

        let response = JSONResponse {
            success: true,
            error: false,
            reason: "Correct password!".to_owned(),
        };
        info!("Successful login.");
        (
            Status::Ok,
            (
                ContentType::JSON,
                serde_json::to_string(&response).unwrap_or_default(),
            ),
        )
    } else {
        // Keep this function backwards compatible
        if let Some(valid_pass) = authorized
            && !valid_pass
        {
            warn!("Failed login attempt!");
            return (
                Status::Unauthorized,
                (ContentType::Plain, "Wrong password!".to_owned()),
            );
        }
        // Return Ok if no password was set on the server side
        cookies.add_private(rocket::http::Cookie::new(
            "chhoto-url-auth",
            auth::gen_token_text(),
        ));

        info!("Successful login.");
        (
            Status::Ok,
            (ContentType::Plain, "Correct password!".to_owned()),
        )
    }
}
