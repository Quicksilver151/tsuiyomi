use std::error::Error;

use tsuiyomi_core::{
    ensure_runtime, host_data_dir, load_config, parse_mihon_backup, BackupManga, HostClient,
    HostProcess, Library,
};
use tsuiyomi_tui::AppState;

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("TSUIYOMI_HOST_SELFTEST").is_some() {
        return host_selftest();
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(command) = args.first() {
        match command.as_str() {
            "backup-info" => {
                let Some(path) = args.get(1) else {
                    return Err("usage: tsuiyomi backup-info <backup.tachibk>".into());
                };
                return backup_info(path);
            }
            "--help" | "-h" | "help" => {
                println!("tsuiyomi                     launch the TUI");
                println!("tsuiyomi backup-info <file>  inspect a Mihon backup without importing");
                return Ok(());
            }
            _ => return Err(format!("unknown command: {command}").into()),
        }
    }

    let config = load_config();
    let library_path = config.library_path.clone();

    let (library, status) = match library_path {
        Some(path) => (
            Library {
                root: path,
                ..Library::default()
            },
            Some("Scanning the library…".to_owned()),
        ),
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

/// Print a Mihon backup summary without touching the library or host.
///
/// This is the verification aid for a real Android backup: it lists the fields the
/// importer understands and flags tracker services that tsuiyomi cannot map yet.
fn backup_info(path: &str) -> Result<(), Box<dyn Error>> {
    let bytes = std::fs::read(path)?;
    let compressed = bytes.starts_with(&[0x1f, 0x8b]);
    let backup = parse_mihon_backup(&bytes)?;
    let source_names: std::collections::BTreeMap<i64, &str> = backup
        .sources
        .iter()
        .map(|source| (source.source_id, source.name.as_str()))
        .collect();

    println!("File          {path}");
    println!(
        "Container     {}",
        if compressed {
            "gzip protobuf"
        } else {
            "plain protobuf"
        }
    );
    println!("Manga         {}", backup.mangas.len());
    println!("Categories    {}", backup.categories.len());
    println!("Sources       {}", backup.sources.len());

    let mut chapters = 0usize;
    let mut read = 0usize;
    let mut bookmarked = 0usize;
    let mut history = 0usize;
    let mut tracking = 0usize;
    let mut viewer_flags = 0usize;
    let mut chapter_flags = 0usize;
    let mut excluded_scanlators = 0usize;
    let mut next_updates = 0usize;
    let mut date_fetch = 0usize;
    for manga in &backup.mangas {
        let source = source_names
            .get(&manga.source_id)
            .copied()
            .unwrap_or("(unnamed source)");
        chapters += manga.chapters.len();
        read += manga.chapters.iter().filter(|chapter| chapter.read).count();
        bookmarked += manga
            .chapters
            .iter()
            .filter(|chapter| chapter.bookmark)
            .count();
        history += manga.history.len();
        tracking += manga.tracking.len();
        if manga.viewer_flags.is_some() || manga.viewer != 0 {
            viewer_flags += 1;
        }
        if manga.chapter_flags != 0 {
            chapter_flags += 1;
        }
        if !manga.excluded_scanlators.is_empty() {
            excluded_scanlators += 1;
        }
        if manga.next_update != 0 {
            next_updates += 1;
        }
        date_fetch += manga
            .chapters
            .iter()
            .filter(|chapter| chapter.date_fetch != 0)
            .count();
        print_manga_summary(manga, source);
    }

    println!();
    println!("Chapters      {chapters} ({read} read, {bookmarked} bookmarked)");
    println!("History       {history}");
    println!("Tracking      {tracking}");
    println!("Viewer flags  {viewer_flags}");
    println!("Chapter flags {chapter_flags}");
    println!("Excluded scan {excluded_scanlators}");
    println!("Next updates  {next_updates}");
    println!("Date fetched  {date_fetch}");
    Ok(())
}

fn print_manga_summary(manga: &BackupManga, source: &str) {
    let tracking = manga
        .tracking
        .iter()
        .map(|tracking| {
            format!(
                "{}:{}",
                tracker_name(tracking.sync_id),
                if tracking.media_id != 0 {
                    tracking.media_id
                } else {
                    tracking.media_id_int
                }
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let flags = [
        manga.viewer_flags.map(|_| "viewer"),
        (manga.chapter_flags != 0).then_some("chapters"),
        (manga.update_strategy != 0).then_some("update"),
        (!manga.excluded_scanlators.is_empty()).then_some("excluded"),
        (manga.next_update != 0).then_some("next"),
        (!manga.tracking.is_empty()).then_some("tracking"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("+");
    println!(
        "- {} | source {source} ({}) | {} chapters | notes {} | history {} | tracking [{tracking}] | flags [{}]",
        manga.title,
        manga.source_id,
        manga.chapters.len(),
        if manga.notes.trim().is_empty() { "no" } else { "yes" },
        manga.history.len(),
        if flags.is_empty() { "none" } else { &flags },
    );
}

fn tracker_name(sync_id: i64) -> &'static str {
    match sync_id {
        1 => "mal",
        2 => "anilist",
        3 => "kitsu",
        4 => "shikimori",
        5 => "bangumi",
        6 => "komga",
        7 => "mangaupdates",
        8 => "kavita",
        9 => "suwayomi",
        10 => "hikka",
        11 => "mangabaka",
        12 => "comick",
        _ => "unknown",
    }
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
