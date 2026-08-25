// SPDX-FileCopyrightText: 2023-2026 Sayantan Santra <sayantan.santra689@gmail.com>
// SPDX-License-Identifier: MIT

use regex::Regex;
use rocket::http::Header;
use tokio::time::{Duration, sleep};

use super::utils::*;
use crate::*;

#[tokio::test]
async fn adding_link_with_shortlink() {
    let test = "adding";
    let conf = default_config(test);
    let (_tempdir, app) = create_app(&conf, test).await;
    let api_key = conf.api_key.unwrap();
    for shortlink in ["test1", "test2", "test3"] {
        let (status, reply) = add_link(&app, &api_key, shortlink, 10, "").await;
        assert!(status.class().is_success());
        assert_eq!(reply.shortlink, format!("https://mydomain.com/{shortlink}"));
    }

    let (status, reply) = add_link(&app, &api_key, "test1", 10, "").await;
    assert!(status.class().is_client_error());
    assert_eq!(reply.reason, "Short URL is already in use!");
}

#[tokio::test]
async fn adding_link_with_shortlink_capital_letters() {
    let test = "adding-capital";
    let mut conf = default_config(test);
    conf.allow_capital_letters = true;
    let (_tempdir, app) = create_app(&conf, test).await;
    let api_key = conf.api_key.unwrap();
    for shortlink in ["Test1", "Test2", "Test3"] {
        let (status, reply) = add_link(&app, &api_key, shortlink, 10, "").await;
        assert!(status.class().is_success());
        assert_eq!(reply.shortlink, format!("https://mydomain.com/{shortlink}"));
    }

    let (status, reply) = add_link(&app, &api_key, "Test1", 10, "").await;
    assert!(status.class().is_client_error());
    assert_eq!(reply.reason, "Short URL is already in use!");
}

#[tokio::test]
async fn adding_link_with_generated_shortlink_with_pair_slug() {
    let test = "shortlink-with-pair-slug";
    let conf = default_config(test);
    let (_tempdir, app) = create_app(&conf, test).await;
    let (status, reply) = add_link(&app, &conf.api_key.unwrap(), "", 10, "").await;

    assert!(status.class().is_success());
    let re = Regex::new(r"^https://mydomain.com/[a-z]+-[a-z]+$").unwrap();
    assert!(re.is_match(reply.shortlink.as_str()));
}

#[tokio::test]
async fn adding_link_with_generated_shortlink_with_uid_slug() {
    let test = "autogen-with-uid-slug";
    let mut conf = default_config(test);
    conf.slug_style = config::SlugStyle::Uid;
    conf.slug_length = 12;
    let (_tempdir, app) = create_app(&conf, test).await;
    let (status, reply) = add_link(&app, &conf.api_key.unwrap(), "", 10, "").await;

    assert!(status.class().is_success());
    let re = Regex::new(r"^https://mydomain.com/[a-z0-9]{12}$").unwrap();
    assert!(re.is_match(reply.shortlink.as_str()));
}

#[tokio::test]
async fn empty_insertion() {
    let test = "batch-insertion";
    let conf = default_config(test);
    let (_tempdir, app) = create_app(&conf, test).await;
    let resp = app
        .post("/api/new")
        .header(Header::new("X-API-Key", conf.api_key.unwrap()))
        .body("[]")
        .dispatch()
        .await;
    let status = resp.status();
    let body = resp.into_string().await.unwrap();
    let response: URLData = serde_json::from_str(&body).unwrap();

    assert!(status.class().is_client_error());
    assert_eq!(response.reason, "An empty array of links was provided!");
}

#[tokio::test]
async fn bad_inserts() {
    let test = "bad-inserts";
    let conf = default_config(test);
    let api_key = conf.api_key.clone().unwrap();
    let (_tempdir, app) = create_app(&conf, test).await;
    for (shortlink, longlink, notes) in [
        ("bad_&1", "https://example.com", "note"),
        ("*bad_)", "https://example.com", "note"),
        ("Bad3", "https://example.com", "note"),
        ("good1", "file:///example.com", "note"),
        ("good1", "ftps://example.com", "note"),
        ("good1", "https://example.com", "note\x00"),
        ("good1", "https://example.com", "note\t"),
    ] {
        let resp = app
            .post("/api/new")
            .header(Header::new("X-API-Key", api_key.clone()))
            .body(format!(
                r#"{{"shortlink":"{shortlink}","longlink":"{longlink}","notes":"{notes}"}}"#
            ))
            .dispatch()
            .await;
        let status = resp.status();
        assert!(status.class().is_client_error());
    }
}

#[tokio::test]
async fn bad_edits() {
    let test = "bad-edits";
    let conf = default_config(test);
    let api_key = conf.api_key.clone().unwrap();
    let (_tempdir, app) = create_app(&conf, test).await;

    let (status, _) = add_link(&app, &api_key, "test1", 0, "note").await;
    status.class().is_success();

    for (shortlink, notes) in [
        ("bad_&1", "note"),
        ("*bad_)", "note"),
        ("Bad3", "note"),
        ("good1", "note"),
        ("good1", "note"),
        ("good1", "note\x00"),
        ("good1", "note\t"),
    ] {
        let resp = edit_link(&app, &api_key, shortlink, false, None, Some(notes));
        assert!(resp.await.class().is_client_error());
    }

    let resp = app
        .put("/api/edit")
        .header(Header::new("X-API-Key", api_key))
        .body(r#"[{"shortlink":"test1","longlink":"ftps://example.com/test1"}"#)
        .dispatch()
        .await;
    assert!(resp.status().class().is_client_error());
}

#[tokio::test]
async fn batch_insertion() {
    let test = "batch-insertion";
    let mut conf = default_config(test);
    conf.slug_style = config::SlugStyle::Uid;
    conf.slug_length = 12;
    let (_tempdir, app) = create_app(&conf, test).await;
    let resp = app
        .post("/api/new")
        .header(Header::new("X-API-Key", conf.api_key.unwrap()))
        .body(
            r#"[{"shortlink":"test1","longlink":"https://example.com/test1"},
        {"shortlink":"test2","longlink":"https://example.com/test2"},
        {"longlink":"https://example.com/test2", "expiry_delay": 10},
        {"shortlink":"test1","longlink":"https://example.com/test3"}]"#,
        )
        .dispatch()
        .await;
    let status = resp.status();
    let body = resp.into_string().await.unwrap();
    let urls: Vec<URLData> = serde_json::from_str(&body).unwrap();
    let mut urls = urls.into_iter();

    assert!(status.class().is_success());
    assert_eq!(urls.next().unwrap().shortlink, "https://mydomain.com/test1");
    assert_eq!(urls.next().unwrap().shortlink, "https://mydomain.com/test2");
    assert!(urls.next().unwrap().expiry_time > 0);
    assert_eq!(urls.next().unwrap().reason, "Short URL is already in use!");
}

#[tokio::test]
async fn adding_link_with_generated_shortlink_with_uid_slug_capital_letters() {
    let test = "autogen-with-uid-slug-capital";
    let mut conf = default_config(test);
    conf.slug_style = config::SlugStyle::Uid;
    conf.slug_length = 12;
    conf.allow_capital_letters = true;
    let (_tempdir, app) = create_app(&conf, test).await;
    let (status, reply) = add_link(&app, &conf.api_key.unwrap(), "", 10, "").await;

    assert!(status.class().is_success());
    let re = Regex::new(r"^https://mydomain.com/[A-Za-z0-9]{12}$").unwrap();
    assert!(re.is_match(reply.shortlink.as_str()));
}

#[tokio::test]
async fn adding_link_with_retry_on_collision() {
    let test = "retry_on_collision";
    let mut conf = default_config(test);
    conf.slug_style = config::SlugStyle::Uid;
    conf.slug_length = 1;
    conf.try_longer_slug = true;

    let (_tempdir, app) = create_app(&conf, test).await;
    let api_key = &conf.api_key.unwrap();

    // Add every possible single-character shortlink
    {
        #[rustfmt::skip]
        static CHARS: [char; 36] = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j',
            'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x','y',
            'z', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

        for c in CHARS.iter() {
            let (status, _) = add_link(&app, api_key, c, 10, "").await;
            assert!(status.class().is_success());
        }
    }

    // Generated shortlinks should now be 5 characters
    {
        let (status, reply) = add_link(&app, api_key, "", 10, "").await;
        assert!(status.class().is_success());
        assert_eq!(
            reply.shortlink.chars().count(),
            "https://mydomain.com/".len() + 5
        );
    }

    // But a colliding provided shortlink should fail
    {
        let (status, _) = add_link(&app, api_key, "a", 10, "").await;
        assert!(status.class().is_client_error());
    }
}

#[tokio::test]
async fn adding_links_with_custom_protocol() {
    let test = "custom-protocols";
    let mut conf = default_config(test);
    conf.allowed_protocols.push("ftps".to_string());
    let (_tempdir, app) = create_app(&conf, test).await;
    let api_key = conf.api_key.clone().unwrap();
    let resp = app
        .post("/api/new")
        .header(Header::new("X-API-Key", api_key.clone()))
        .body(r#"{{"shortlink":"test","longlink":"ftps://example.com","notes":"note"}}"#)
        .dispatch()
        .await;
    let status = resp.status();
    assert!(status.class().is_client_error());
}

#[tokio::test]
async fn link_editing() {
    let test = "link-editing";
    let conf = default_config(test);
    let (_tempdir, app) = create_app(&conf, test).await;
    let api_key = conf.api_key.clone().unwrap();

    let (status, _) = add_link(&app, &api_key, "test1", 0, "").await;
    assert!(status.class().is_success());
    let (status, _) = add_link(&app, &api_key, "test2", 10, "").await;
    assert!(status.class().is_success());

    let resp = app.get("/test2").dispatch().await;
    assert!(resp.status().class().is_redirection());

    let resp = app.get("/test1").dispatch().await;

    let timer = Duration::from_millis(800);
    sleep(timer).await;

    let now = chrono::Utc::now().timestamp();
    let status = edit_link(&app, &api_key, "test2", false, Some(now + 1), None).await;
    assert!(status.class().is_success());

    let (status, reply) = expand(&app, &api_key, "test2").await;
    assert!(status.class().is_success());
    assert_eq!(reply.longlink, "https://edited-test2.com");
    assert_eq!(reply.hits, 1);
    assert_eq!(reply.expiry_time, now + 1);

    assert!(resp.status().class().is_redirection());
    let status = edit_link(&app, &api_key, "test1", true, None, None).await;
    assert!(status.class().is_success());

    let (status, reply) = expand(&app, &api_key, "test1").await;
    assert!(status.class().is_success());
    assert_eq!(reply.longlink, "https://edited-test1.com");
    assert_eq!(reply.hits, 0);
}
