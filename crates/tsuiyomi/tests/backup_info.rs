use std::process::Command;

use tsuiyomi_core::{write_mihon_backup, BackupChapter, BackupManga, BackupTracking, MihonBackup};

#[test]
fn backup_info_reports_tracking_and_flags() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("backup.tachibk");
    let backup = MihonBackup {
        mangas: vec![BackupManga {
            source_id: 42,
            url: "/manga/alpha".to_owned(),
            title: "Alpha".to_owned(),
            chapters: vec![BackupChapter {
                url: "/chapter/1".to_owned(),
                name: "Chapter 1".to_owned(),
                read: true,
                date_fetch: 1_650_000_000_000,
                ..BackupChapter::default()
            }],
            viewer_flags: Some(4),
            chapter_flags: 0x22,
            excluded_scanlators: vec!["Old Scan".to_owned()],
            next_update: 1_700_000_000_000,
            tracking: vec![BackupTracking {
                sync_id: 2,
                media_id: 99,
                title: "Alpha".to_owned(),
                last_chapter_read: 1.0,
                ..BackupTracking::default()
            }],
            ..BackupManga::default()
        }],
        ..MihonBackup::default()
    };
    write_mihon_backup(&path, &backup).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tsuiyomi"))
        .arg("backup-info")
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Manga         1"));
    assert!(text.contains("Alpha"));
    assert!(text.contains("tracking [anilist:99]"));
    assert!(text.contains("flags [viewer+chapters"));
    assert!(text.contains("Date fetched  1"));
}
