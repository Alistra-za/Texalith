//! The overwrite check and the all-or-nothing write of a set of outputs. The rollback follows Tessalith's
//! `write_all` (`crates/gui/src/model/from_normal.rs` at 36384560, GPL-3.0).
use crate::marker::is_ours;
use engine::dds::{may_replace, write_atomic, Kept};
use std::fs;
use std::path::{Path, PathBuf};

/// One output to write.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// What is at a planned path now.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Nothing there.
    New,
    /// The planned bytes already.
    Same,
    /// A Texalith file whose marker proves its pixels: replaced without asking.
    Ours,
    /// Anything else: kept unless the user ticks it.
    Foreign,
    /// Something that cannot be read: always kept.
    Unreadable(String),
    /// The file the outputs are made from: always kept, even when it is Texalith's own.
    Source,
}

/// `source` is the opened input.
pub fn check(p: &Planned, source: &Path) -> Verdict {
    // Canonical paths, so another spelling (case on Windows, `..`) of the opened file still matches.
    if let (Ok(a), Ok(b)) = (fs::canonicalize(&p.path), fs::canonicalize(source)) {
        if a == b {
            return Verdict::Source;
        }
    }
    match fs::read(&p.path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Verdict::New,
        Ok(b) if b == p.bytes => return Verdict::Same,
        _ => {}
    }
    match may_replace(&p.path, &p.bytes, is_ours) {
        Ok(()) => Verdict::Ours,
        Err(Kept::Foreign) => Verdict::Foreign,
        Err(Kept::Unreadable(e)) => Verdict::Unreadable(e.to_string()),
    }
}

/// `<dir>/_sdc/texalith-backup/<name>`, or `<stem>-2.<ext>`, `-3`, … when that name already holds other bytes; a
/// backup that already holds `old` is reused.
fn backup_path(path: &Path, old: &[u8]) -> PathBuf {
    let dir = path.parent().unwrap_or(Path::new(".")).join("_sdc").join("texalith-backup");
    let name = Path::new(path.file_name().unwrap_or_default());
    let (stem, ext) = (name.file_stem().unwrap_or_default().to_string_lossy(), name.extension());
    (1..)
        .map(|n| match (n, ext) {
            (1, _) => dir.join(name),
            (_, Some(e)) => dir.join(format!("{stem}-{n}.{}", e.to_string_lossy())),
            (_, None) => dir.join(format!("{stem}-{n}")),
        })
        .find(|b| fs::read(b).map_or(true, |have| have == old))
        .expect("an unused backup name")
}

/// Writes every `New` and `Ours` entry, and each `Foreign` entry whose `overwrite_foreign[i]` is true (first copied to
/// `<dir>/_sdc/texalith-backup/` with `backup`, see [`backup_path`]). `Same` is skipped; `Unreadable` and `Source`
/// are kept. If a write fails,
/// the files this call already wrote get their old bytes back, or are removed when they were new, and the error
/// names the failing path. Returns the paths written.
pub fn write(
    plan: &[Planned],
    source: &Path,
    overwrite_foreign: &[bool],
    backup: bool,
) -> Result<Vec<PathBuf>, String> {
    let read = |p: &Path| fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
    // (entry, its previous bytes when it existed)
    let mut todo: Vec<(&Planned, Option<Vec<u8>>)> = vec![];
    for (i, p) in plan.iter().enumerate() {
        match check(p, source) {
            Verdict::Same | Verdict::Unreadable(_) | Verdict::Source => {}
            Verdict::New => todo.push((p, None)),
            Verdict::Ours => todo.push((p, Some(read(&p.path)?))),
            Verdict::Foreign if overwrite_foreign.get(i).copied().unwrap_or(false) => {
                let old = read(&p.path)?;
                if backup {
                    let b = backup_path(&p.path, &old);
                    write_atomic(&b, &old).map_err(|e| format!("backup {}: {e}", b.display()))?;
                }
                todo.push((p, Some(old)));
            }
            Verdict::Foreign => {}
        }
    }
    // shortcut: a file created between the check and the rename is replaced, as in Tessalith.
    for (i, (p, _)) in todo.iter().enumerate() {
        if let Err(e) = write_atomic(&p.path, &p.bytes) {
            let mut msg = format!("{}: {e}", p.path.display());
            for (q, old) in todo[..i].iter().rev() {
                let undo = match old {
                    Some(old) => write_atomic(&q.path, old),
                    None => fs::remove_file(&q.path),
                };
                if let Err(e) = undo {
                    msg.push_str(&format!("; {} could not be undone: {e}", q.path.display()));
                }
            }
            return Err(msg);
        }
    }
    Ok(todo.into_iter().map(|(p, _)| p.path.clone()).collect())
}

/// `name`, or `name` with 1, 2, … before its extension: the first whose path in `folder` is none of `taken`.
/// Paths are compared without regard to case or slash direction, and files that exist also by their real path,
/// because one file can be reached by several spellings (a junction, a mapped drive, `..`).
// shortcut: every call canonicalizes each taken file, and the tabs call this once a frame; cache per folder and
// file list if a column on a network drive stutters.
pub fn free_name(folder: &Path, name: &str, taken: &[&Path]) -> String {
    let key = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
    let taken: Vec<(String, Option<PathBuf>)> = taken.iter().map(|p| (key(p), p.canonicalize().ok())).collect();
    let is_taken = |p: &Path| {
        let (k, real) = (key(p), p.canonicalize().ok());
        taken.iter().any(|(tk, treal)| *tk == k || (real.is_some() && *treal == real))
    };
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    (0u32..)
        .map(|n| match (n, ext) {
            (0, _) => name.to_string(),
            (_, "") => format!("{stem}{n}"),
            _ => format!("{stem}{n}.{ext}"),
        })
        .find(|c| !is_taken(&folder.join(c)))
        .expect("an unbounded range")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_name_gets_the_first_free_number() {
        let d = Path::new("C:/tex/rock");
        let (base, layer) = (d.join("rock_n.png"), d.join("ROCK_N1.png"));
        assert_eq!(free_name(d, "rock_n.png", &[]), "rock_n.png");
        assert_eq!(free_name(Path::new("C:/tex/out"), "rock_n.png", &[&base]), "rock_n.png");
        assert_eq!(free_name(d, "rock_n.png", &[&base]), "rock_n1.png");
        assert_eq!(free_name(Path::new("C:\\tex\\rock"), "rock_n.png", &[&base, &layer]), "rock_n2.png");
        assert_eq!(free_name(d, "Rock_cnr.png", &[&d.join("rock_cnr.png")]), "Rock_cnr1.png");
    }

    #[test]
    fn a_taken_file_reached_by_another_spelling_of_the_folder_moves_on() {
        let d = std::env::temp_dir().join(format!("texalith-free-name-{}", std::process::id()));
        let sub = d.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let (base, layer) = (d.join("rock_n.png"), d.join("rock_n1.png"));
        std::fs::write(&base, b"b").unwrap();
        std::fs::write(&layer, b"l").unwrap();
        // The same folder through `sub\..`: the text differs, the files are the same.
        assert_eq!(free_name(&sub.join(".."), "rock_n.png", &[&base, &layer]), "rock_n2.png");
        let _ = std::fs::remove_dir_all(&d);
    }
}
