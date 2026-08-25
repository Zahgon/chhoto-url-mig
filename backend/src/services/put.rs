// SPDX-FileCopyrightText: 2023-2026 Sayantan Santra <sayantan.santra689@gmail.com>
// SPDX-License-Identifier: MIT

use rocket::State;
use rocket::http::{ContentType, Status};
use rocket::put;

use crate::{
    AppState,
    auth::Auth,
    services::types::{
        ChhotoError::{ClientError, ServerError},
        JSONResponse,
    },
    utils,
};

// Edit a shortlink
#[put("/api/edit", data = "<req>")]
pub(crate) async fn edit_link(
    req: String,
    auth: Auth,
    data: &State<AppState>,
) -> (Status, (ContentType, String)) {
    let config = &data.config;
    match auth {
        Auth::ValidAPIKey | Auth::ValidSession => {
            let edit_result = {
                let writer = data.writer.lock().await;
                utils::edit_link_helper(&req, &writer, &data.hits_tx, config)
            };
            match edit_result {
                Ok(()) => {
                    let body = JSONResponse {
                        success: true,
                        error: false,
                        reason: String::from("Edit was successful."),
                    };
                    (
                        Status::Created,
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
                        reason: "Something went wrong when editing the link.".to_owned(),
                    };
                    (
                        Status::InternalServerError,
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
        Auth::None { result } | Auth::InvalidAPIKey { result } => (
            Status::Unauthorized,
            (
                ContentType::JSON,
                serde_json::to_string(&result).unwrap_or_default(),
            ),
        ),
    }
}
