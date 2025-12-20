use std::fs::File;
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, Registry};
use directories::BaseDirs;

pub fn init_logging() -> anyhow::Result<()> {
    let log_dir = if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
        BaseDirs::new().map(|base| {
            base.home_dir()
                .join(".config")
                .join("tenere")
                .join("logs")
        })
    } else {
        // Fallback
        None
    };

    if let Some(dir) = log_dir {
        std::fs::create_dir_all(&dir)?;
        let log_file = dir.join("tenere.log");
        let file = File::create(log_file)?;
        
        let file_layer = fmt::layer()
            .with_writer(Arc::new(file))
            .with_ansi(false);

        Registry::default()
            .with(file_layer)
            .init();
    }

    Ok(())
}
