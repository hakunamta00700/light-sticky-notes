#![forbid(unsafe_code)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Note {
    pub id: u64,
    pub body: String,
    pub color: NoteColor,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NoteColor {
    #[default]
    Yellow,
    Green,
    Blue,
    Pink,
    Purple,
    Gray,
}

impl NoteColor {
    fn key(self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Pink => "pink",
            Self::Purple => "purple",
            Self::Gray => "gray",
        }
    }

    fn from_key(value: &str) -> Self {
        match value.trim() {
            "green" => Self::Green,
            "blue" => Self::Blue,
            "pink" => Self::Pink,
            "purple" => Self::Purple,
            "gray" => Self::Gray,
            _ => Self::Yellow,
        }
    }
}

#[derive(Debug)]
pub struct NoteStore {
    root: PathBuf,
    notes: Vec<Note>,
}

impl NoteStore {
    pub fn open(root: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&root)?;
        let mut notes = Vec::new();

        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("txt") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|value| value.to_str())
                .and_then(|value| value.parse::<u64>().ok())
            else {
                continue;
            };
            let color = fs::read_to_string(color_path(&root, id))
                .map(|value| NoteColor::from_key(&value))
                .unwrap_or_default();
            notes.push(Note {
                id,
                body: fs::read_to_string(path)?,
                color,
            });
        }

        notes.sort_unstable_by(|left, right| right.id.cmp(&left.id));
        let mut store = Self { root, notes };
        if store.notes.is_empty() {
            store.create_note()?;
        }
        Ok(store)
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    pub fn note_by_id(&self, id: u64) -> Option<&Note> {
        self.notes.iter().find(|note| note.id == id)
    }

    pub fn note_by_id_mut(&mut self, id: u64) -> Option<&mut Note> {
        self.notes.iter_mut().find(|note| note.id == id)
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.notes.iter().position(|note| note.id == id)
    }

    pub fn create_note(&mut self) -> io::Result<usize> {
        let mut id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        while self.path_for(id).exists() {
            id = id.saturating_add(1);
        }

        let note = Note {
            id,
            body: String::new(),
            color: NoteColor::default(),
        };
        self.write(&note)?;
        self.notes.insert(0, note);
        Ok(0)
    }

    pub fn save(&self, index: usize) -> io::Result<()> {
        let note = self.notes.get(index).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "note index does not exist")
        })?;
        self.write(note)
    }

    pub fn set_color(&mut self, id: u64, color: NoteColor) -> io::Result<()> {
        let note = self.note_by_id_mut(id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "note id does not exist")
        })?;
        note.color = color;
        let note = note.clone();
        self.write_color(&note)
    }

    pub fn delete(&mut self, index: usize) -> io::Result<usize> {
        if index >= self.notes.len() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "note index does not exist",
            ));
        }

        let note = self.notes.remove(index);
        match fs::remove_file(self.path_for(note.id)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        match fs::remove_file(color_path(&self.root, note.id)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }

        if self.notes.is_empty() {
            return self.create_note();
        }
        Ok(index.min(self.notes.len() - 1))
    }

    fn write(&self, note: &Note) -> io::Result<()> {
        let final_path = self.path_for(note.id);
        let temporary_path = final_path.with_extension("tmp");
        fs::write(&temporary_path, note.body.as_bytes())?;
        if final_path.exists() {
            fs::remove_file(&final_path)?;
        }
        fs::rename(temporary_path, final_path)?;
        self.write_color(note)
    }

    fn write_color(&self, note: &Note) -> io::Result<()> {
        let final_path = color_path(&self.root, note.id);
        if fs::read_to_string(&final_path)
            .map(|value| value.trim() == note.color.key())
            .unwrap_or(false)
        {
            return Ok(());
        }
        let temporary_path = final_path.with_extension("color.tmp");
        fs::write(&temporary_path, note.color.key().as_bytes())?;
        if final_path.exists() {
            fs::remove_file(&final_path)?;
        }
        fs::rename(temporary_path, final_path)
    }

    fn path_for(&self, id: u64) -> PathBuf {
        self.root.join(format!("{id:020}.txt"))
    }
}

fn color_path(root: &Path, id: u64) -> PathBuf {
    root.join(format!("{id:020}.color"))
}

pub fn default_data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("FEATHER_NOTES_DATA_DIR") {
        return PathBuf::from(path);
    }
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join("FeatherNotes")
        .join("notes")
}

pub fn note_title(body: &str) -> String {
    let title = body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("새 메모");
    let mut chars = title.chars();
    let abbreviated: String = chars.by_ref().take(36).collect();
    if chars.next().is_some() {
        format!("{abbreviated}…")
    } else {
        abbreviated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_store() -> PathBuf {
        std::env::temp_dir().join(format!(
            "feather-notes-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn creates_saves_reopens_and_deletes_notes() {
        let root = temporary_store();
        let mut store = NoteStore::open(root.clone()).unwrap();
        assert_eq!(store.notes().len(), 1);

        let first_id = store.notes()[0].id;
        store.note_by_id_mut(first_id).unwrap().body = "첫 메모".into();
        store.save(0).unwrap();
        store.create_note().unwrap();
        let second_id = store.notes()[0].id;
        store.note_by_id_mut(second_id).unwrap().body = "둘째 메모".into();
        store.save(0).unwrap();
        store.set_color(second_id, NoteColor::Blue).unwrap();

        let mut reopened = NoteStore::open(root.clone()).unwrap();
        assert_eq!(reopened.notes().len(), 2);
        assert_eq!(reopened.notes()[0].body, "둘째 메모");
        assert_eq!(reopened.notes()[0].color, NoteColor::Blue);
        reopened.delete(0).unwrap();
        assert_eq!(reopened.notes().len(), 1);
        assert!(!color_path(&root, second_id).exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn title_uses_first_nonempty_line_and_limits_unicode_safely() {
        assert_eq!(note_title("\n  제목  \n본문"), "제목");
        assert_eq!(note_title("   \n"), "새 메모");
        assert_eq!(note_title(&"가".repeat(40)), format!("{}…", "가".repeat(36)));
    }
}
