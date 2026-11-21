use std::error::Error;

use tsuiyomi_core::{
    ensure_runtime, host_data_dir, load_config, scan_library, HostClient, HostProcess, Library,
};
use tsuiyomi_tui::AppState;

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("TSUIYOMI_HOST_SELFTEST").is_some() {
        return host_selftest();
    }

    let config = load_config();
    let library_path = config.library_path.clone();

    let (library, status) = match library_path {
        Some(path) => {
            let placeholder = Library {
                root: path.clone(),
                ..Library::default()
            };
            match scan_library(&path) {
                Ok(scanned) => {
                    let status = scan_status(&scanned);
                    (scanned, status)
                }
                Err(error) => (placeholder, Some(error.to_string())),
            }
        }
        None => (
            Library::default(),
            Some("Set TSUIYOMI_LIBRARY to a Mihon local/ folder.".to_owned()),
        ),
    };

    let mut app = AppState::new(library, status);
    app.set_config(config);
    tsuiyomi_tui::run(app)?;
    Ok(())
}

/// Verify that the shipped binary can extract and run its extension host.
///
/// `TSUIYOMI_HOST_SELFTEST=1 tsuiyomi` needs no terminal and exits after the health
/// check, which makes it usable in packaging tests.
fn host_selftest() -> Result<(), Box<dyn Error>> {
    let config = load_config();
    let Some(runtime) = ensure_runtime(config.host_jar.as_deref())? else {
        return Err("no embedded or cached extension host".into());
    };
    println!("jar:  {}", runtime.jar.display());
    println!("java: {}", runtime.java.display());

    let data_dir = host_data_dir().ok_or("cannot resolve the host data directory")?;
    let process = HostProcess::start(&runtime, &data_dir, config.flaresolverr_url.as_deref())?;
    let client = HostClient::new(process.base_url());
    client.health()?;

    let sources = client.sources()?;
    println!(
        "host: {} ({} extension sources)",
        process.base_url(),
        sources.len()
    );
    for source in sources.iter().take(10) {
        println!("  {}", source.display_name());
    }
    Ok(())
}

fn scan_status(library: &Library) -> Option<String> {
    if library.errors.is_empty() {
        None
    } else {
        Some(format!(
            "{} series, {} could not be read",
            library.series.len(),
            library.errors.len()
        ))
    }
}
