//! 메모 도메인 모델과 파일 기반 저장소를 제공한다.
//!
//! 이 모듈은 Windows UI를 전혀 알지 못한다. 따라서 파일 입출력과 메모 규칙을 창 없이도
//! 단위 테스트할 수 있다.

#![forbid(unsafe_code)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 화면과 디스크에서 함께 사용하는 하나의 메모다.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Note {
    pub id: u64,
    pub body: String,
    pub color: NoteColor,
}

/// 사용자가 선택할 수 있는 포스트잇 색상이다.
///
/// `Copy`이므로 색상 값은 소유권 이동 없이 값 자체가 복사된다.
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
    /// 색상을 사람이 읽을 수 있는 안정적인 문자열로 직렬화한다.
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

    /// 알 수 없는 값은 기본 색상으로 복구하여 손상된 설정 때문에 앱이 중단되지 않게 한다.
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

/// 메모 컬렉션과 해당 컬렉션의 저장 경로를 함께 소유한다.
///
/// 외부 코드는 파일 경로를 직접 조작하지 않고 이 객체의 메서드를 통해서만 메모를 변경한다.
#[derive(Debug)]
pub struct NoteStore {
    root: PathBuf,
    notes: Vec<Note>,
}

impl NoteStore {
    /// 저장 폴더를 열고 기존 메모를 메모리로 읽어 들인다.
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

        // 최신 ID가 앞에 오도록 정렬한다. `sort_unstable_by`는 동일 ID의 순서를 보장할 필요가
        // 없는 대신 추가 메모리를 적게 사용하는 정렬이다.
        notes.sort_unstable_by_key(|note| std::cmp::Reverse(note.id));
        let mut store = Self { root, notes };
        if store.notes.is_empty() {
            store.create_note()?;
        }
        Ok(store)
    }

    /// 현재 메모를 읽기 전용 슬라이스로 노출한다.
    /// `&[Note]`는 호출자가 벡터의 크기나 원소를 직접 변경하지 못하게 한다.
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// ID를 통해 불변 메모 참조를 찾는다.
    pub fn note_by_id(&self, id: u64) -> Option<&Note> {
        self.notes.iter().find(|note| note.id == id)
    }

    /// ID를 통해 가변 메모 참조를 찾는다.
    /// 반환된 `&mut Note`가 살아 있는 동안에는 같은 저장소를 다른 방식으로 빌릴 수 없다.
    pub fn note_by_id_mut(&mut self, id: u64) -> Option<&mut Note> {
        self.notes.iter_mut().find(|note| note.id == id)
    }

    /// ID에 대응하는 벡터 인덱스를 찾는다.
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.notes.iter().position(|note| note.id == id)
    }

    /// 새 메모를 만들고 저장한 뒤 컬렉션에서의 인덱스를 반환한다.
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

    /// 인덱스로 선택한 메모를 디스크에 저장한다.
    pub fn save(&self, index: usize) -> io::Result<()> {
        let note = self
            .notes
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "note index does not exist"))?;
        self.write(note)
    }

    /// 색상을 메모리와 디스크에 동시에 반영한다.
    pub fn set_color(&mut self, id: u64, color: NoteColor) -> io::Result<()> {
        let note = self
            .note_by_id_mut(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "note id does not exist"))?;
        note.color = color;
        let note = note.clone();
        self.write_color(&note)
    }

    /// 메모 파일과 색상 보조 파일을 함께 삭제한다.
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

    /// 임시 파일을 완성한 뒤 최종 파일명으로 바꾸어 중간 상태의 노출을 줄인다.
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

    /// 본문과 별개인 `.color` 보조 파일에 색상을 저장한다.
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

    /// 숫자 ID를 고정 폭 파일명으로 변환한다.
    fn path_for(&self, id: u64) -> PathBuf {
        self.root.join(format!("{id:020}.txt"))
    }
}

/// 색상 보조 파일의 경로를 계산하는 순수 함수다.
fn color_path(root: &Path, id: u64) -> PathBuf {
    root.join(format!("{id:020}.color"))
}

/// 환경 변수 또는 Windows 사용자 데이터 폴더에서 기본 저장 위치를 계산한다.
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

/// 목록 카드에 표시할 첫 번째 비어 있지 않은 줄을 유니코드 문자 기준으로 자른다.
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
    // 테스트는 실제 사용자 폴더 대신 프로세스별 임시 경로를 사용한다.
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
        assert_eq!(
            note_title(&"가".repeat(40)),
            format!("{}…", "가".repeat(36))
        );
    }
}
