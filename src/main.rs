use anyhow::{Result, bail};
use rust_knowledge::{
    api::{self, AppState},
    config::Config,
    use_cases::crawls::recover,
};
use sqlx::{Connection, postgres::PgPoolOptions};
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    // Load configuration before Tokio starts worker threads. Existing process
    // variables take precedence over the local file.
    match dotenvy::from_path(".env") {
        Ok(()) => {}
        Err(error) if error.not_found() => {}
        // dotenvy's parse errors include the offending line, which may hold a secret.
        Err(_) => bail!("Could not load .env; check file permissions and syntax"),
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rust_knowledge=info,tower_http=info".into()),
        )
        .init();
    let config = Config::from_env()?;
    run(config)
}

#[tokio::main]
async fn run(config: Config) -> Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    {
        let mut connection = pool.acquire().await?;
        connection.close_on_drop();
        let mut tx = connection.begin().await?;
        recover::execute(&mut tx).await?;
        tx.commit().await?;
    }
    let state = AppState::new(pool, &config);
    let crawler = state.crawler.clone();
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(address = %listener.local_addr()?, "local server ready; crawling starts only on an explicit API request");
    let result = axum::serve(listener, api::router(state, &config.web_dist))
        .with_graceful_shutdown(shutdown_signal())
        .await;
    crawler.shutdown().await;
    result?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl+C handler");
    };
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
