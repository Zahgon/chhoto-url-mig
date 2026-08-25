// SPDX-FileCopyrightText: 2023-2026 Sayantan Santra <sayantan.santra689@gmail.com>
// SPDX-License-Identifier: MIT

use rocket::http::{Header, Status};

use super::utils::*;

#[tokio::test]
async fn basic_site_config() {
    let test = "basic";
    let conf = default_config(test);
    let (_tempdir, app) = create_app(&conf, test).await;

    let resp = app.get("/api/siteurl").dispatch().await;
    let body = resp.into_string().await.unwrap();
    assert_eq!(body, conf.site_url.clone().unwrap());

    let resp = app.get("/api/whoami").dispatch().await;
    let body = resp.into_string().await.unwrap();
    assert_eq!(body, "nobody");

    let resp = app
        .get("/api/whoami")
        .header(Header::new("X-API-Key", conf.api_key.clone().unwrap()))
        .dispatch()
        .await;
    let body = resp.into_string().await.unwrap();
    assert_eq!(body, "admin");

    let resp = app.get("/api/version").dispatch().await;
    let body = resp.into_string().await.unwrap();
    assert!(body.starts_with(concat!("Chhoto URL v", env!("CARGO_PKG_VERSION"))));

    let resp = app
        .get("/api/getconfig")
        .header(Header::new("X-API-Key", conf.api_key.clone().unwrap()))
        .dispatch()
        .await;
    assert!(resp.status().class().is_success());
    let body = resp.into_string().await.unwrap();
    let conf: BackendConfig = serde_json::from_str(&body).unwrap();
    assert!(conf.version.starts_with(env!("CARGO_PKG_VERSION")));
    assert_eq!(conf.slug_length, 8);
}

#[tokio::test]
async fn auth_verification() {
    let test = "auth_verification";
    let conf = default_config(test);
    let (_tempdir, app) = create_app(&conf, test).await;

    let resp = app.get("/api/all").dispatch().await;
    assert_eq!(resp.status(), Status::Unauthorized);
    let body = resp.into_string().await.unwrap();
    assert_eq!(body, "Unauthorized");

    let status = edit_link(&app, "a", "test2", false, None, None).await;
    assert_eq!(status, Status::Unauthorized);

    let (status, reply) = add_link(&app, "a", "test1", 0, "").await;
    assert_eq!(status, Status::Unauthorized);
    assert_eq!(reply.reason, "API validation failed.");

    let resp = app.delete("/api/del/link").dispatch().await;
    assert_eq!(resp.status(), Status::Unauthorized);

    let resp = app.get("/api/getconfig").dispatch().await;
    assert_eq!(resp.status(), Status::Unauthorized);
}
