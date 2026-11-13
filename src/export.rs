//! The overwrite check and the all-or-nothing write of a set of outputs. The rollback follows Tessalith's
//! `write_all` (`crates/gui/src/model/from_normal.rs` at 36384560, GPL-3.0).
use crate::marker::is_ours;
use engine::dds::write_atomic;
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
    /// A file open in Texalith, in any tab: always kept, even when it is Texalith's own.
    Source,
}

/// The last part of `p` as text, or nothing when it has none.
pub fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

impl Verdict {
    /// The file is kept unless the user says otherwise, so Save asks before it writes anything.
    pub fn asks(&self) -> bool {
        matches!(self, Verdict::Foreign | Verdict::Unreadable(_) | Verdict::Source)
    }
}

/// The verdict, and the bytes of a file that may be replaced. `sources` are the files open in Texalith.
fn inspect(p: &Planned, sources: &[PathBuf]) -> (Verdict, Option<Vec<u8>>) {
    // Canonical paths, so another spelling (case on Windows, `..`) of an open file still matches.
    if let Ok(a) = fs::canonicalize(&p.path) {
        if sources.iter().any(|s| fs::canonicalize(s).is_ok_and(|b| a == b)) {
            return (Verdict::Source, None);
        }
    }
    // One read: the same bytes give the verdict and are what a failed write puts back.
    match fs::read(&p.path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Verdict::New, None),
        Err(e) => (Verdict::Unreadable(e.to_string()), None),
        Ok(b) if b == p.bytes => (Verdict::Same, None),
        Ok(b) if is_ours(&b) => (Verdict::Ours, Some(b)),
        Ok(b) => (Verdict::Foreign, Some(b)),
    }
}

/// `sources` are the files open in Texalith.
pub fn check(p: &Planned, sources: &[PathBuf]) -> Verdict {
    inspect(p, sources).0
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

/// An entry to write and the bytes it replaces, `None` for a new file.
type Todo<'a> = (&'a Planned, Option<Vec<u8>>);

/// Writes every entry. If a write fails, the files already written get their old bytes back, or are removed when
/// they were new, and the error names the failing path. Returns the paths written.
fn commit(todo: Vec<Todo>) -> Result<Vec<PathBuf>, String> {
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

/// Writes every `New` and `Ours` entry, and each `Foreign` entry whose `overwrite_foreign[i]` is true (first copied to
/// `<dir>/_sdc/texalith-backup/` with `backup`, see [`backup_path`]). `Same` is skipped; `Unreadable` and `Source`
/// are kept. All or nothing, see [`commit`]. The disk is checked here, not taken from an earlier check: the dialog
/// may have been open for a while.
pub fn write<'a>(
    plan: impl IntoIterator<Item = &'a Planned>,
    sources: &[PathBuf],
    overwrite_foreign: &[bool],
    backup: bool,
) -> Result<Vec<PathBuf>, String> {
    let mut todo: Vec<Todo> = vec![];
    for (i, p) in plan.into_iter().enumerate() {
        match inspect(p, sources) {
            (Verdict::New, _) => todo.push((p, None)),
            (Verdict::Ours, old) => todo.push((p, old)),
            (Verdict::Foreign, Some(old)) if overwrite_foreign.get(i).copied().unwrap_or(false) => {
                if backup {
                    let b = backup_path(&p.path, &old);
                    write_atomic(&b, &old).map_err(|e| format!("backup {}: {e}", b.display()))?;
                }
                todo.push((p, Some(old)));
            }
            _ => {}
        }
    }
    commit(todo)
}

/// What [`write_or_ask`] did.
pub enum Unasked {
    /// No file needed a choice: these paths were written.
    Written(Vec<PathBuf>),
    /// Some file would be kept, so nothing was written. One verdict per entry.
    Ask(Vec<Verdict>),
}

/// The write of a Save nobody was asked about yet, with one look at the disk: when a file would be kept
/// ([`Verdict::asks`]) nothing is written and every verdict comes back for the dialog.
pub fn write_or_ask<'a>(plan: impl IntoIterator<Item = &'a Planned>, sources: &[PathBuf]) -> Result<Unasked, String> {
    let found: Vec<_> = plan.into_iter().map(|p| (p, inspect(p, sources))).collect();
    if found.iter().any(|(_, (v, _))| v.asks()) {
        return Ok(Unasked::Ask(found.into_iter().map(|(_, (v, _))| v).collect()));
    }
    let todo = found.into_iter().filter(|(_, (v, _))| *v != Verdict::Same).map(|(p, (_, old))| (p, old)).collect();
    commit(todo).map(Unasked::Written)
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
