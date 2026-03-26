use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "netwatch", version = netwatch::VERSION, about = "Network monitoring and topology mapping")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "/etc/netwatch/netwatch.toml")]
    config: PathBuf,

    /// Listen address override
    #[arg(short, long)]
    listen: Option<String>,

    /// Data directory override
    #[arg(short, long)]
    data_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .compact()
        .init();

    let cli = Cli::parse();

    let mut config = netwatch::config::Config::load(&cli.config)?;

    if let Some(listen) = cli.listen {
        config.server.listen = listen;
    }
    if let Some(data_dir) = cli.data_dir {
        config.server.data_dir = data_dir;
    }

    // Ensure data directory exists
    std::fs::create_dir_all(&config.server.data_dir)?;

    tracing::info!("netwatch v{} starting", netwatch::VERSION);
    tracing::info!("data dir: {}", config.server.data_dir.display());

    // Open database
    let db_path = config.server.data_dir.join("netwatch.redb");
    let db = netwatch::db::Db::open(&db_path)?;
    let db = Arc::new(db);

    tracing::info!("database opened at {}", db_path.display());

    // Seed subnets from config
    for sub in &config.subnets {
        db.upsert_subnet_by_cidr(&sub.cidr, &sub.name, &sub.snmp_community)?;
    }

    let config = Arc::new(config);

    // Broadcast channel for WebSocket live updates
    let (ws_tx, _) = tokio::sync::broadcast::channel::<String>(256);

    let state = netwatch::web::AppState {
        db: db.clone(),
        config: config.clone(),
        ws_tx: ws_tx.clone(),
    };

    // Start background engines
    let monitor_db = db.clone();
    let monitor_cfg = config.clone();
    let monitor_ws = ws_tx.clone();
    tokio::spawn(async move {
        netwatch::monitor::run(monitor_db, monitor_cfg, monitor_ws).await;
    });

    let discovery_db = db.clone();
    let discovery_cfg = config.clone();
    let discovery_ws = ws_tx.clone();
    tokio::spawn(async move {
        netwatch::discovery::run(discovery_db, discovery_cfg, discovery_ws).await;
    });

    let alert_db = db.clone();
    let alert_cfg = config.clone();
    let alert_ws = ws_tx.clone();
    tokio::spawn(async move {
        netwatch::alert::run(alert_db, alert_cfg, alert_ws).await;
    });

    // Retention cleanup
    let retention_db = db.clone();
    let retention_cfg = config.clone();
    tokio::spawn(async move {
        netwatch::db::retention_loop(retention_db, retention_cfg).await;
    });

    // Start web server
    let listen = config.server.listen.clone();
    tracing::info!("listening on {}", listen);
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    let app = netwatch::web::router(state);
    axum::serve(listener, app).await?;

    Ok(())
}
