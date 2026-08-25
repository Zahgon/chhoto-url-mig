// SPDX-FileCopyrightText: 2023-2026 Sayantan Santra <sayantan.santra689@gmail.com>
// SPDX-License-Identifier: MIT

use log::info;
use rocket::State;
use rocket::http::{ContentType, CookieJar, Status};
use rocket::delete;

use crate::{
    AppState,
    auth::Auth,
    services::types::{
        ChhotoError::{ClientError, ServerError},
        JSONResponse,
    },
    utils,
};

// Handle logout
// There's no reason to be calling this route with an API key
#[delete("/api/logout")]
pub(crate) async fn logout(cookies: &CookieJar<'_>) -> (Status, (ContentType, String)) {
    if let Some(cookie) = cookies.get_private("chhoto-url-auth") {
        cookies.remove_private(cookie);
        info!("Successful logout.");
        (Status::Ok, (ContentType::Plain, "Logged out!".to_owned()))
    } else {
        (
            Status::Unauthorized,
            (
                ContentType::Plain,
                "You don't seem to be logged in.".to_owned(),
            ),
        )
    }
}

// Delete a given shortlink
#[delete("/api/del/<shortlink>")]
pub(crate) async fn delete_link(
    shortlink: &str,
    auth: Auth,
    data: &State<AppState>,
) -> (Status, (ContentType, String)) {
    match auth {
        Auth::ValidAPIKey => {
            match utils::delete_link_helper(
                shortlink,
                &*data.writer.lock().await,
                data.config.allow_capital_letters,
            ) {
                Ok(()) => {
                    let response = JSONResponse {
                        success: true,
                        error: false,
                        reason: format!("Deleted {shortlink}"),
                    };
                    (
                        Status::Ok,
                        (
                            ContentType::JSON,
                            serde_json::to_string(&response).unwrap_or_default(),
                        ),
                    )
                }
                Err(ServerError) => {
                    let response = JSONResponse {
                        success: false,
                        error: true,
                        reason: "Something went wrong when deleting the link.".to_owned(),
                    };
                    (
                        Status::InternalServerError,
                        (
                            ContentType::JSON,
                            serde_json::to_string(&response).unwrap_or_default(),
                        ),
                    )
                }
                Err(ClientError { reason }) => {
                    let response = JSONResponse {
                        success: false,
                        error: true,
                        reason,
                    };
                    (
                        Status::NotFound,
                        (
                            ContentType::JSON,
                            serde_json::to_string(&response).unwrap_or_default(),
                        ),
                    )
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
        // If using password - keeps backwards compatibility
        Auth::ValidSession => {
            if utils::delete_link_helper(
                shortlink,
                &*data.writer.lock().await,
                data.config.allow_capital_letters,
            )
            .is_ok()
            {
                (
                    Status::Ok,
                    (ContentType::Plain, format!("Deleted {shortlink}")),
                )
            } else {
                (
                    Status::NotFound,
                    (ContentType::Plain, "Not found!".to_owned()),
                )
            }
        }
        Auth::None { result: _ } => (
            Status::Unauthorized,
            (ContentType::Plain, "Not logged in!".to_owned()),
        ),
    }
}
