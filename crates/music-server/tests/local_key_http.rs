mod common;

use serde_json::{Value, json};
use std::{net::TcpListener, time::Duration};
use tokio::{
    process::Command,
    time::{sleep, timeout},
};

/// The Debian settings (save locations, where the models and engines are kept) answer only with the local key,
/// and an agent naming a folder that is not one of the downloads is refused before anything moves.
#[tokio::test]
async fn the_settings_of_the_debian_build_need_the_local_key() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let data = root.path().join("data");
    let socket = TcpListener::bind(("127.0.0.1", 0))?;
    let address = socket.local_addr()?;
    drop(socket);
    let _server = Command::new(env!("CARGO_BIN_EXE_music-server"))
        .env("YUE_LOCAL_API_KEY", common::TEST_KEY)
        .env("YUE_STUDIO_PORT", address.port().to_string())
        .env("YUE_STUDIO_DATA_ROOT", &data)
        .env("YUE_ENGINE_BIN", root.path().join("absent-engine"))
        .env("TIMONCOOL_SHARED_DIR", root.path().join("registry"))
        .env("HOME", root.path())
        .env("DO_NOT_TRACK", "1")
        .env("TEMP", root.path())
        .env("TMP", root.path())
        .env("TMPDIR", root.path())
        .kill_on_drop(true)
        .spawn()?;
    let http = common::keyed().no_proxy().build()?;
    let stranger = reqwest::Client::builder().no_proxy().build()?;
    let base = format!("http://{address}");
    timeout(Duration::from_secs(30), async {
        while !http.get(format!("{base}/health")).send().await.is_ok_and(|r| r.status().is_success()) {
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;

    for path in ["/v1/save-locations", "/v1/storage", "/v1/transfer-rates"] {
        assert_eq!(stranger.get(format!("{base}{path}")).send().await?.status(), 401, "{path} without the key");
        assert!(http.get(format!("{base}{path}")).send().await?.status().is_success(), "{path} with the key");
    }
    for path in ["/v1/save-locations/pick", "/v1/storage/pick"] {
        let status = stranger.post(format!("{base}{path}")).json(&json!({})).send().await?.status();
        assert_eq!(status, 401, "{path} opens no dialog without the key");
    }
    let wrong = stranger.get(format!("{base}/v1/storage?studio_key=not-the-key")).send().await?.status();
    assert_eq!(wrong, 401);

    // the folders of engines and their models are listed with where they are
    let storage: Value = http.get(format!("{base}/v1/storage")).send().await?.json().await?;
    let names: Vec<&str> = storage["storage"]["folders"].as_array().unwrap().iter().filter_map(|folder| folder["name"].as_str()).collect();
    assert!(names.contains(&"music-engines") && names.contains(&"music-models"), "{names:?}");
    assert!(storage["storage"]["folders"][0]["location"].is_string());

    // only download folders move
    let refused = http
        .put(format!("{base}/v1/storage"))
        .json(&json!({"directory": root.path().join("elsewhere"), "folders": ["media"]}))
        .send()
        .await?;
    assert_eq!(refused.status(), 400);
    assert!(refused.text().await?.contains("no download folder is called media"));
    assert!(!root.path().join("elsewhere").exists(), "nothing was made for a refused move");

    // a place for the engines' models chosen before anything was downloaded: the studio's own path leads there
    let models_disk = root.path().join("models disk");
    let started = http.put(format!("{base}/v1/storage")).json(&json!({"directory": models_disk, "folders": ["music-models"]})).send().await?;
    assert!(started.status().is_success(), "{}", started.text().await?);
    let storage = timeout(Duration::from_secs(30), async {
        loop {
            let now: Value = http.get(format!("{base}/v1/storage")).send().await?.json().await?;
            if now["move"]["running"] != true {
                return anyhow::Ok(now);
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await??;
    assert_eq!(storage["move"]["error"], Value::Null, "{storage}");
    let models = storage["storage"]["folders"].as_array().unwrap().iter().find(|folder| folder["name"] == "music-models").unwrap();
    assert_eq!(models["elsewhere"], true, "{models}");
    let kept = models_disk.join("YuE2 Studio models").join("music-models");
    assert_eq!(models["location"], kept.display().to_string());
    // no link is left in the data folder: the studio writes down where the folder is, and goes there
    assert!(std::fs::symlink_metadata(data.join("music-models")).is_err(), "no link in the data folder");
    let written: Value = serde_json::from_slice(&std::fs::read(data.join("storage-folders.json"))?)?;
    assert_eq!(written["music-models"], kept.display().to_string());
    assert_eq!(written["music-model-objects"], models_disk.join("YuE2 Studio models").join("music-model-objects").display().to_string(), "the shared weights go with the models");

    // save locations are kept on disk, whole
    let saved = http
        .put(format!("{base}/v1/save-locations"))
        .json(&json!({"folders": {"midi": root.path().join("midi")}, "auto_export": true}))
        .send()
        .await?;
    assert!(saved.status().is_success());
    let file: Value = serde_json::from_slice(&std::fs::read(data.join("save-locations.json"))?)?;
    assert_eq!(file["auto_export"], true);
    assert_eq!(file["folders"]["midi"], root.path().join("midi").display().to_string());
    let relative = http.put(format!("{base}/v1/save-locations")).json(&json!({"folders": {"midi": "Muzyka"}})).send().await?;
    assert_eq!(relative.status(), 400, "a path that is not a full one is refused");
    let kept: Value = serde_json::from_slice(&std::fs::read(data.join("save-locations.json"))?)?;
    assert_eq!(kept, file, "and what was kept stays");
    Ok(())
}
