use anyhow::Result;

#[tokio::test]
#[ignore = "this test takes up to 30 minutes to complete"]
async fn it_downloads_model_files() -> Result<()> {
    tracing_subscriber::fmt().init();

    std::fs::remove_dir_all(steno_daemon::model::parakeet_model_dir()?)?;
    steno_daemon::model::ensure_parakeet_model().await
}
