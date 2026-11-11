//! The file system Hubchat Web gives the core (`rt::fs` in a browser).
//!
//! A page has no `std::fs`. It has the origin's private file system (OPFS),
//! where a file is opened asynchronously and then read and written
//! synchronously, by one owner at a time, in a dedicated worker. This module
//! is everything about that except the browser calls themselves: paths, an
//! index of what exists (so the questions the core asks synchronously, such
//! as "is there a file here?", have an answer), one shared handle per open
//! file, and std's own error kinds. The browser calls sit behind `Backend`
//! (`rt_opfs.rs`); the tests here run natively against a backend in memory.
//!
//! One `Fs` owns its files: nothing else may change them while it lives (the
//! core's worker holds the Web Lock that makes it the only one).
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::{self, ErrorKind, SeekFrom};
use std::rc::{Rc, Weak};
use std::time::SystemTime;

/// One open file of the backend: synchronous, positioned reads and writes.
pub trait Handle: 'static {
    fn read_at(&self, buf: &mut [u8], at: u64) -> io::Result<usize>;
    fn write_at(&self, buf: &[u8], at: u64) -> io::Result<usize>;
    fn set_len(&self, len: u64) -> io::Result<()>;
    fn len(&self) -> io::Result<u64>;
    fn flush(&self) -> io::Result<()>;
    /// Give the file up; the handle is not used again.
    fn close(&self);
}

/// What `Backend::list` says of one entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// `a/b/c`: no leading slash
    pub key: String,
    pub dir: bool,
    pub len: u64,
    pub modified: SystemTime,
}

/// The browser's part. Keys are `a/b/c`; a parent always exists when a
/// child is asked for (the `Fs` checks its index first).
#[allow(async_fn_in_trait)]
pub trait Backend: 'static {
    type Handle: Handle;
    /// Everything there is, parents before children.
    async fn list(&self) -> io::Result<Vec<Listed>>;
    async fn open(&self, key: &str, create: bool) -> io::Result<Self::Handle>;
    async fn create_dir(&self, key: &str) -> io::Result<()>;
    /// A file, or an empty directory.
    async fn remove(&self, key: &str, dir: bool) -> io::Result<()>;
    /// Move a closed file. `to` does not exist.
    async fn rename(&self, from: &str, to: &str) -> io::Result<()>;
}

/// What is known of a path (std's `Metadata`, as far as the core asks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata {
    dir: bool,
    len: u64,
    modified: SystemTime,
}

impl Metadata {
    pub fn len(&self) -> u64 {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn is_file(&self) -> bool {
        !self.dir
    }
    pub fn is_dir(&self) -> bool {
        self.dir
    }
    pub fn modified(&self) -> io::Result<SystemTime> {
        Ok(self.modified)
    }
}

/// One entry of `read_dir`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub metadata: Metadata,
}

fn err(kind: ErrorKind, what: impl Into<String>) -> io::Error {
    io::Error::new(kind, what.into())
}

/// `/a/b`, `a\b`, `a/./b` -> `a/b`. `..` is refused: nothing the core asks
/// for needs it, and a path must never leave the files' own directory.
pub fn key(path: &str) -> io::Result<String> {
    let mut parts = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return Err(err(ErrorKind::InvalidInput, format!("{path}: a path here may not go up with .."))),
            p => parts.push(p),
        }
    }
    Ok(parts.join("/"))
}

fn parent(key: &str) -> &str {
    key.rsplit_once('/').map_or("", |(p, _)| p)
}

/// The handle of one open file, shared by every `File` on it (a browser
/// gives out one at a time), each with its own position.
struct Shared<H: Handle> {
    key: RefCell<String>,
    /// None once the file was removed under it
    handle: RefCell<Option<H>>,
}

impl<H: Handle> Shared<H> {
    fn with<T>(&self, f: impl FnOnce(&H) -> io::Result<T>) -> io::Result<T> {
        match &*self.handle.borrow() {
            Some(h) => f(h),
            None => Err(err(ErrorKind::NotFound, "the file was removed")),
        }
    }
}

impl<H: Handle> Drop for Shared<H> {
    fn drop(&mut self) {
        if let Some(h) = self.handle.borrow_mut().take() {
            let _ = h.flush();
            h.close();
        }
    }
}

struct State<H: Handle> {
    /// every file and directory, by key; the root is "" and always there
    index: BTreeMap<String, Metadata>,
    open: HashMap<String, Weak<Shared<H>>>,
}

pub struct Fs<B: Backend> {
    backend: B,
    state: Rc<RefCell<State<B::Handle>>>,
    now: fn() -> SystemTime,
}

/// How a file is opened (std's `OpenOptions`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub read: bool,
    pub write: bool,
    pub append: bool,
    pub truncate: bool,
    pub create: bool,
    pub create_new: bool,
}

pub struct File<H: Handle> {
    shared: Rc<Shared<H>>,
    state: Weak<RefCell<State<H>>>,
    pos: u64,
    read: bool,
    write: bool,
    append: bool,
    now: fn() -> SystemTime,
}

impl<B: Backend> Fs<B> {
    /// Reads what is there. `now` is the clock for "modified".
    pub async fn load(backend: B, now: fn() -> SystemTime) -> io::Result<Self> {
        let mut index = BTreeMap::new();
        index.insert(String::new(), Metadata { dir: true, len: 0, modified: now() });
        for l in backend.list().await? {
            index.insert(l.key, Metadata { dir: l.dir, len: l.len, modified: l.modified });
        }
        Ok(Self { backend, state: Rc::new(RefCell::new(State { index, open: HashMap::new() })), now })
    }

    /// What keeps the files (for what only it can do with one).
    pub fn backend(&self) -> &B {
        &self.backend
    }

    // ---- what the core asks without waiting (from the index)

    pub fn metadata_now(&self, path: &str) -> io::Result<Metadata> {
        let k = key(path)?;
        self.state.borrow().index.get(&k).copied().ok_or_else(|| err(ErrorKind::NotFound, format!("{path}: no such file or directory")))
    }

    pub fn exists(&self, path: &str) -> bool {
        self.metadata_now(path).is_ok()
    }

    /// The entries directly inside a directory, by name.
    pub fn read_dir_now(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        let k = key(path)?;
        let state = self.state.borrow();
        match state.index.get(&k) {
            None => return Err(err(ErrorKind::NotFound, format!("{path}: no such directory"))),
            Some(m) if !m.dir => return Err(err(ErrorKind::NotADirectory, format!("{path}: not a directory"))),
            Some(_) => {}
        }
        let prefix = if k.is_empty() { String::new() } else { format!("{k}/") };
        Ok(state
            .index
            .range(prefix.clone()..)
            .take_while(|(name, _)| name.starts_with(&prefix))
            .filter(|(name, _)| !name.is_empty() && !name[prefix.len()..].contains('/'))
            .map(|(name, m)| DirEntry { name: name[prefix.len()..].to_string(), path: format!("/{name}"), metadata: *m })
            .collect())
    }

    // ---- files

    pub async fn open(&self, path: &str, o: Options) -> io::Result<File<B::Handle>> {
        let k = key(path)?;
        if k.is_empty() {
            return Err(err(ErrorKind::IsADirectory, format!("{path}: is a directory")));
        }
        if !(o.read || o.write || o.append) {
            return Err(err(ErrorKind::InvalidInput, "a file is opened to read or to write"));
        }
        let writes = o.write || o.append;
        if (o.truncate || o.create || o.create_new) && !writes {
            return Err(err(ErrorKind::InvalidInput, "creating or truncating needs write access"));
        }
        let known = self.state.borrow().index.get(&k).copied();
        match known {
            Some(m) if m.dir => return Err(err(ErrorKind::IsADirectory, format!("{path}: is a directory"))),
            Some(_) if o.create_new => return Err(err(ErrorKind::AlreadyExists, format!("{path}: already exists"))),
            Some(_) => {}
            None if !(o.create || o.create_new) => return Err(err(ErrorKind::NotFound, format!("{path}: no such file"))),
            None => match self.state.borrow().index.get(parent(&k)) {
                Some(m) if m.dir => {}
                Some(_) => return Err(err(ErrorKind::NotADirectory, format!("{path}: its parent is not a directory"))),
                None => return Err(err(ErrorKind::NotFound, format!("{path}: its directory does not exist"))),
            },
        }
        let already = self.state.borrow().open.get(&k).and_then(Weak::upgrade);
        let shared = match already {
            Some(s) => s,
            None => {
                let handle = self.backend.open(&k, known.is_none()).await?;
                // another task may have opened it while this one waited
                let raced = self.state.borrow().open.get(&k).and_then(Weak::upgrade);
                match raced {
                    Some(s) => {
                        handle.close();
                        s
                    }
                    None => {
                        let s = Rc::new(Shared { key: RefCell::new(k.clone()), handle: RefCell::new(Some(handle)) });
                        let mut state = self.state.borrow_mut();
                        state.open.insert(k.clone(), Rc::downgrade(&s));
                        state.index.entry(k.clone()).or_insert(Metadata { dir: false, len: 0, modified: (self.now)() });
                        s
                    }
                }
            }
        };
        let file = File { shared, state: Rc::downgrade(&self.state), pos: 0, read: o.read, write: writes, append: o.append, now: self.now };
        if o.truncate {
            file.set_len(0)?;
        }
        Ok(file)
    }

    pub async fn create_dir_all(&self, path: &str) -> io::Result<()> {
        let k = key(path)?;
        let mut at = String::new();
        for part in k.split('/').filter(|p| !p.is_empty()) {
            if !at.is_empty() {
                at.push('/');
            }
            at.push_str(part);
            let known = self.state.borrow().index.get(&at).copied();
            match known {
                Some(m) if m.dir => {}
                Some(_) => return Err(err(ErrorKind::NotADirectory, format!("{path}: /{at} is a file"))),
                None => {
                    self.backend.create_dir(&at).await?;
                    self.state.borrow_mut().index.insert(at.clone(), Metadata { dir: true, len: 0, modified: (self.now)() });
                }
            }
        }
        Ok(())
    }

    /// One directory; its parent must exist and it must not.
    pub async fn create_dir(&self, path: &str) -> io::Result<()> {
        let k = key(path)?;
        if self.state.borrow().index.contains_key(&k) {
            return Err(err(ErrorKind::AlreadyExists, format!("{path}: already exists")));
        }
        match self.state.borrow().index.get(parent(&k)) {
            Some(m) if m.dir => {}
            _ => return Err(err(ErrorKind::NotFound, format!("{path}: its parent does not exist"))),
        }
        self.backend.create_dir(&k).await?;
        self.state.borrow_mut().index.insert(k, Metadata { dir: true, len: 0, modified: (self.now)() });
        Ok(())
    }

    pub async fn remove_file(&self, path: &str) -> io::Result<()> {
        let k = key(path)?;
        match self.state.borrow().index.get(&k) {
            None => return Err(err(ErrorKind::NotFound, format!("{path}: no such file"))),
            Some(m) if m.dir => return Err(err(ErrorKind::IsADirectory, format!("{path}: is a directory"))),
            Some(_) => {}
        }
        // an open file is given up first: the browser removes no file that is held
        let held = self.state.borrow_mut().open.remove(&k).and_then(|w| w.upgrade());
        if let Some(s) = held {
            if let Some(h) = s.handle.borrow_mut().take() {
                h.close();
            }
        }
        self.backend.remove(&k, false).await?;
        self.state.borrow_mut().index.remove(&k);
        Ok(())
    }

    /// An empty directory.
    pub async fn remove_dir(&self, path: &str) -> io::Result<()> {
        let k = key(path)?;
        if k.is_empty() {
            return Err(err(ErrorKind::InvalidInput, "the files' own directory is not removed"));
        }
        match self.state.borrow().index.get(&k) {
            None => return Err(err(ErrorKind::NotFound, format!("{path}: no such directory"))),
            Some(m) if !m.dir => return Err(err(ErrorKind::NotADirectory, format!("{path}: not a directory"))),
            Some(_) => {}
        }
        if !self.read_dir_now(path)?.is_empty() {
            return Err(err(ErrorKind::DirectoryNotEmpty, format!("{path}: the directory is not empty")));
        }
        self.backend.remove(&k, true).await?;
        self.state.borrow_mut().index.remove(&k);
        Ok(())
    }

    /// A directory with everything in it. Nothing there: Ok, as the core's
    /// cleanups expect of a directory that is already gone.
    pub async fn remove_dir_all(&self, path: &str) -> io::Result<()> {
        let k = key(path)?;
        if k.is_empty() {
            return Err(err(ErrorKind::InvalidInput, "the files' own directory is not removed"));
        }
        let Ok(entries) = self.read_dir_now(path) else { return Ok(()) };
        for e in entries {
            if e.metadata.is_dir() {
                Box::pin(self.remove_dir_all(&e.path)).await?;
            } else {
                self.remove_file(&e.path).await?;
            }
        }
        self.remove_dir(path).await
    }

    /// Move a file; an existing file at `to` is replaced, as std does.
    pub async fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        let (f, t) = (key(from)?, key(to)?);
        let meta = match self.state.borrow().index.get(&f) {
            None => return Err(err(ErrorKind::NotFound, format!("{from}: no such file"))),
            Some(m) if m.dir => return Err(err(ErrorKind::Unsupported, format!("{from}: a directory is not moved here"))),
            Some(m) => *m,
        };
        if f == t {
            return Ok(());
        }
        let target = self.state.borrow().index.get(&t).copied();
        match target {
            Some(m) if m.dir => return Err(err(ErrorKind::IsADirectory, format!("{to}: is a directory"))),
            Some(_) => self.remove_file(to).await?,
            None => match self.state.borrow().index.get(parent(&t)) {
                Some(m) if m.dir => {}
                _ => return Err(err(ErrorKind::NotFound, format!("{to}: its directory does not exist"))),
            },
        }
        // a held file is given up for the move and taken again where it now is
        let held = self.state.borrow_mut().open.remove(&f).and_then(|w| w.upgrade());
        let len = match &held {
            Some(s) => {
                let taken = s.handle.borrow_mut().take();
                match taken {
                    Some(h) => {
                        h.flush()?;
                        let len = h.len()?;
                        h.close();
                        len
                    }
                    None => meta.len,
                }
            }
            None => meta.len,
        };
        self.backend.rename(&f, &t).await?;
        {
            let mut state = self.state.borrow_mut();
            state.index.remove(&f);
            state.index.insert(t.clone(), Metadata { len, ..meta });
        }
        if let Some(s) = held {
            let handle = self.backend.open(&t, false).await?;
            *s.handle.borrow_mut() = Some(handle);
            *s.key.borrow_mut() = t.clone();
            self.state.borrow_mut().open.insert(t, Rc::downgrade(&s));
        }
        Ok(())
    }

    /// A second file with the same bytes (a browser has no hard links).
    pub async fn copy(&self, from: &str, to: &str) -> io::Result<()> {
        if self.exists(to) {
            return Err(err(ErrorKind::AlreadyExists, format!("{to}: already exists")));
        }
        let mut source = self.open(from, Options { read: true, ..Default::default() }).await?;
        let mut dest = self.open(to, Options { write: true, create_new: true, ..Default::default() }).await?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = source.read(&mut buf)?;
            if n == 0 {
                break;
            }
            dest.write_all(&buf[..n])?;
        }
        dest.flush()
    }
}

impl<H: Handle> File<H> {
    fn note(&self, len: u64) {
        if let Some(state) = self.state.upgrade() {
            if let Some(m) = state.borrow_mut().index.get_mut(&*self.shared.key.borrow()) {
                m.len = len;
                m.modified = (self.now)();
            }
        }
    }

    pub fn metadata(&self) -> io::Result<Metadata> {
        let len = self.shared.with(|h| h.len())?;
        let modified = self.state.upgrade().and_then(|s| s.borrow().index.get(&*self.shared.key.borrow()).map(|m| m.modified)).unwrap_or_else(self.now);
        Ok(Metadata { dir: false, len, modified })
    }

    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.read {
            return Err(err(ErrorKind::PermissionDenied, "the file was not opened to read"));
        }
        let n = self.shared.with(|h| h.read_at(buf, self.pos))?;
        self.pos += n as u64;
        Ok(n)
    }

    pub fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.write {
            return Err(err(ErrorKind::PermissionDenied, "the file was not opened to write"));
        }
        if self.append {
            self.pos = self.shared.with(|h| h.len())?;
        }
        let n = self.shared.with(|h| h.write_at(buf, self.pos))?;
        self.pos += n as u64;
        let len = self.shared.with(|h| h.len())?;
        self.note(len);
        Ok(n)
    }

    pub fn write_all(&mut self, mut buf: &[u8]) -> io::Result<()> {
        while !buf.is_empty() {
            match self.write(buf)? {
                0 => return Err(err(ErrorKind::WriteZero, "the browser wrote nothing (its storage may be full)")),
                n => buf = &buf[n..],
            }
        }
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.shared.with(|h| h.flush())
    }

    pub fn set_len(&self, len: u64) -> io::Result<()> {
        if !self.write {
            return Err(err(ErrorKind::PermissionDenied, "the file was not opened to write"));
        }
        self.shared.with(|h| h.set_len(len))?;
        self.note(len);
        Ok(())
    }

    pub fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let from = |base: u64, by: i64| base.checked_add_signed(by).ok_or_else(|| err(ErrorKind::InvalidInput, "a position before the start of the file"));
        self.pos = match to {
            SeekFrom::Start(n) => n,
            SeekFrom::Current(by) => from(self.pos, by)?,
            SeekFrom::End(by) => from(self.shared.with(|h| h.len())?, by)?,
        };
        Ok(self.pos)
    }

    pub fn position(&self) -> u64 {
        self.pos
    }

    /// The same open file, with its own position (at the start).
    pub fn try_clone(&self) -> File<H> {
        File { shared: self.shared.clone(), state: self.state.clone(), pos: self.pos, read: self.read, write: self.write, append: self.append, now: self.now }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    /// A backend in memory that behaves like the browser where it matters:
    /// one handle per file at a time, no removing or moving a held file.
    #[derive(Default)]
    struct Mem {
        files: Rc<RefCell<BTreeMap<String, Rc<RefCell<Vec<u8>>>>>>,
        dirs: Rc<RefCell<Vec<String>>>,
        held: Rc<RefCell<Vec<String>>>,
        opened: Rc<RefCell<usize>>,
    }
    struct MemHandle {
        key: String,
        bytes: Rc<RefCell<Vec<u8>>>,
        held: Rc<RefCell<Vec<String>>>,
    }
    impl Handle for MemHandle {
        fn read_at(&self, buf: &mut [u8], at: u64) -> io::Result<usize> {
            let b = self.bytes.borrow();
            let at = (at as usize).min(b.len());
            let n = buf.len().min(b.len() - at);
            buf[..n].copy_from_slice(&b[at..at + n]);
            Ok(n)
        }
        fn write_at(&self, buf: &[u8], at: u64) -> io::Result<usize> {
            let mut b = self.bytes.borrow_mut();
            let end = at as usize + buf.len();
            if b.len() < end {
                b.resize(end, 0);
            }
            b[at as usize..end].copy_from_slice(buf);
            Ok(buf.len())
        }
        fn set_len(&self, len: u64) -> io::Result<()> {
            self.bytes.borrow_mut().resize(len as usize, 0);
            Ok(())
        }
        fn len(&self) -> io::Result<u64> {
            Ok(self.bytes.borrow().len() as u64)
        }
        fn flush(&self) -> io::Result<()> {
            Ok(())
        }
        fn close(&self) {
            self.held.borrow_mut().retain(|k| k != &self.key);
        }
    }
    impl Backend for Mem {
        type Handle = MemHandle;
        async fn list(&self) -> io::Result<Vec<Listed>> {
            let mut all: Vec<Listed> = self.dirs.borrow().iter().map(|d| Listed { key: d.clone(), dir: true, len: 0, modified: UNIX_EPOCH }).collect();
            all.extend(self.files.borrow().iter().map(|(k, b)| Listed { key: k.clone(), dir: false, len: b.borrow().len() as u64, modified: UNIX_EPOCH }));
            all.sort_by(|a, b| a.key.cmp(&b.key));
            Ok(all)
        }
        async fn open(&self, key: &str, create: bool) -> io::Result<MemHandle> {
            if self.held.borrow().iter().any(|k| k == key) {
                return Err(err(ErrorKind::ResourceBusy, format!("{key} is held: the browser gives out one handle")));
            }
            let bytes = match self.files.borrow().get(key) {
                Some(b) => b.clone(),
                None if create => Rc::default(),
                None => return Err(err(ErrorKind::NotFound, key)),
            };
            self.files.borrow_mut().insert(key.to_string(), bytes.clone());
            self.held.borrow_mut().push(key.to_string());
            *self.opened.borrow_mut() += 1;
            Ok(MemHandle { key: key.to_string(), bytes, held: self.held.clone() })
        }
        async fn create_dir(&self, key: &str) -> io::Result<()> {
            self.dirs.borrow_mut().push(key.to_string());
            Ok(())
        }
        async fn remove(&self, key: &str, dir: bool) -> io::Result<()> {
            if self.held.borrow().iter().any(|k| k == key) {
                return Err(err(ErrorKind::ResourceBusy, format!("{key} is held")));
            }
            if dir {
                self.dirs.borrow_mut().retain(|d| d != key);
            } else {
                self.files.borrow_mut().remove(key).ok_or_else(|| err(ErrorKind::NotFound, key))?;
            }
            Ok(())
        }
        async fn rename(&self, from: &str, to: &str) -> io::Result<()> {
            if self.held.borrow().iter().any(|k| k == from) {
                return Err(err(ErrorKind::ResourceBusy, format!("{from} is held")));
            }
            let b = self.files.borrow_mut().remove(from).ok_or_else(|| err(ErrorKind::NotFound, from))?;
            self.files.borrow_mut().insert(to.to_string(), b);
            Ok(())
        }
    }

    fn clock() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_794_420_307)
    }
    const READ: Options = Options { read: true, write: false, append: false, truncate: false, create: false, create_new: false };
    const CREATE: Options = Options { read: true, write: true, append: false, truncate: true, create: true, create_new: false };
    const NEW: Options = Options { read: false, write: true, append: false, truncate: false, create: false, create_new: true };

    async fn fresh() -> Fs<Mem> {
        Fs::load(Mem::default(), clock).await.unwrap()
    }
    fn kind<T>(r: io::Result<T>) -> ErrorKind {
        match r {
            Ok(_) => panic!("it worked"),
            Err(e) => e.kind(),
        }
    }
    fn names(fs: &Fs<Mem>, path: &str) -> Vec<String> {
        fs.read_dir_now(path).unwrap().into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn paths_are_keys_and_never_go_up() {
        assert_eq!(key("/downloads/a b.txt").unwrap(), "downloads/a b.txt");
        assert_eq!(key("downloads\\x\\.\\y//z").unwrap(), "downloads/x/y/z");
        assert_eq!(key("/").unwrap(), "");
        assert_eq!(key("").unwrap(), "");
        assert_eq!(kind(key("/downloads/../keys")), ErrorKind::InvalidInput);
        assert_eq!(kind(key("..")), ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn write_then_read_and_the_index_follows() {
        let fs = fresh().await;
        assert_eq!(kind(fs.open("/staging/a.bin", CREATE).await), ErrorKind::NotFound, "its directory does not exist yet");
        fs.create_dir_all("/staging/one").await.unwrap();
        fs.create_dir_all("/staging/one").await.unwrap();
        assert!(fs.metadata_now("/staging").unwrap().is_dir() && fs.metadata_now("staging/one").unwrap().is_dir());

        let mut f = fs.open("/staging/one/0.bin", CREATE).await.unwrap();
        f.write_all(b"hello ").unwrap();
        f.write_all(b"world").unwrap();
        assert_eq!(fs.metadata_now("/staging/one/0.bin").unwrap().len(), 11, "known while the file is still open");
        assert_eq!(fs.metadata_now("/staging/one/0.bin").unwrap().modified().unwrap(), clock());
        assert_eq!(f.seek(SeekFrom::Start(6)).unwrap(), 6);
        let mut buf = [0u8; 16];
        assert_eq!(f.read(&mut buf).unwrap(), 5);
        assert_eq!(&buf[..5], b"world");
        assert_eq!(f.read(&mut buf).unwrap(), 0, "the end");
        assert_eq!(f.seek(SeekFrom::End(-5)).unwrap(), 6);
        assert_eq!(f.seek(SeekFrom::Current(-6)).unwrap(), 0);
        assert_eq!(kind(f.seek(SeekFrom::Current(-1))), ErrorKind::InvalidInput);
        f.set_len(5).unwrap();
        assert_eq!((f.metadata().unwrap().len(), fs.metadata_now("/staging/one/0.bin").unwrap().len()), (5, 5));
        drop(f);

        // closed and opened again: the bytes are the file's
        let mut f = fs.open("/staging/one/0.bin", READ).await.unwrap();
        let mut all = Vec::new();
        loop {
            let n = f.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            all.extend_from_slice(&buf[..n]);
        }
        assert_eq!(all, b"hello");
        assert_eq!(kind(f.write(b"x")), ErrorKind::PermissionDenied);
        assert_eq!(kind(f.set_len(0)), ErrorKind::PermissionDenied);
        assert!(f.metadata().unwrap().is_file());
    }

    #[tokio::test]
    async fn the_ways_to_open() {
        let fs = fresh().await;
        fs.create_dir_all("/d").await.unwrap();
        assert_eq!(kind(fs.open("/d/none", READ).await), ErrorKind::NotFound);
        assert_eq!(kind(fs.open("/d", READ).await), ErrorKind::IsADirectory);
        assert_eq!(kind(fs.open("/", READ).await), ErrorKind::IsADirectory);
        assert_eq!(kind(fs.open("/d/x", Options::default()).await), ErrorKind::InvalidInput);
        assert_eq!(kind(fs.open("/d/x", Options { read: true, create: true, ..Default::default() }).await), ErrorKind::InvalidInput);
        fs.open("/d/x", NEW).await.unwrap().write_all(b"abc").unwrap();
        assert_eq!(kind(fs.open("/d/x", NEW).await), ErrorKind::AlreadyExists);
        assert_eq!(kind(fs.open("/d/x/y", NEW).await), ErrorKind::NotADirectory);
        // create without truncate keeps what is there; with it, empties
        let keep = Options { write: true, create: true, ..Default::default() };
        assert_eq!(fs.open("/d/x", keep).await.unwrap().metadata().unwrap().len(), 3);
        assert_eq!(fs.open("/d/x", CREATE).await.unwrap().metadata().unwrap().len(), 0);
        // append writes at the end wherever the position is
        let mut a = fs.open("/d/x", Options { append: true, ..Default::default() }).await.unwrap();
        a.write_all(b"12").unwrap();
        a.seek(SeekFrom::Start(0)).unwrap();
        a.write_all(b"34").unwrap();
        let mut f = fs.open("/d/x", READ).await.unwrap();
        let mut buf = [0u8; 8];
        let n = f.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"1234");
    }

    #[tokio::test]
    async fn one_handle_serves_every_open_of_a_file() {
        let fs = fresh().await;
        fs.create_dir_all("/d").await.unwrap();
        let mut w = fs.open("/d/f", CREATE).await.unwrap();
        w.write_all(b"0123456789").unwrap();
        // a second open while the first is alive: the browser would refuse a second handle
        let mut r = fs.open("/d/f", READ).await.unwrap();
        let mut c = r.try_clone();
        assert_eq!(*fs.backend.opened.borrow(), 1);
        let mut buf = [0u8; 4];
        assert_eq!(r.read(&mut buf).unwrap(), 4);
        assert_eq!(&buf, b"0123");
        assert_eq!(c.read(&mut buf).unwrap(), 4);
        assert_eq!(&buf, b"0123", "its own position");
        assert_eq!(r.read(&mut buf).unwrap(), 4);
        assert_eq!(&buf, b"4567");
        w.write_all(b"ab").unwrap();
        assert_eq!(r.metadata().unwrap().len(), 12);
        drop((w, r));
        assert_eq!(fs.backend.held.borrow().len(), 1, "held while one file is left");
        drop(c);
        assert!(fs.backend.held.borrow().is_empty(), "given up with the last one");
        fs.open("/d/f", READ).await.unwrap();
        assert_eq!(*fs.backend.opened.borrow(), 2);
    }

    #[tokio::test]
    async fn rename_replaces_and_follows_an_open_file() {
        let fs = fresh().await;
        fs.create_dir_all("/dl").await.unwrap();
        let mut part = fs.open("/dl/photo.jpg.part", CREATE).await.unwrap();
        part.write_all(b"new bytes").unwrap();
        fs.open("/dl/photo.jpg", NEW).await.unwrap().write_all(b"old").unwrap();
        // the download's own file is still open when it is moved into place
        fs.rename("/dl/photo.jpg.part", "/dl/photo.jpg").await.unwrap();
        assert_eq!(names(&fs, "/dl"), ["photo.jpg"]);
        assert_eq!(fs.metadata_now("/dl/photo.jpg").unwrap().len(), 9);
        part.write_all(b"!").unwrap();
        assert_eq!(fs.metadata_now("/dl/photo.jpg").unwrap().len(), 10, "the open file is the moved one");
        drop(part);
        let mut f = fs.open("/dl/photo.jpg", READ).await.unwrap();
        let mut buf = [0u8; 16];
        let n = f.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"new bytes!");

        assert_eq!(kind(fs.rename("/dl/none", "/dl/x").await), ErrorKind::NotFound);
        assert_eq!(kind(fs.rename("/dl/photo.jpg", "/nowhere/x").await), ErrorKind::NotFound);
        fs.create_dir_all("/dl/sub").await.unwrap();
        assert_eq!(kind(fs.rename("/dl/photo.jpg", "/dl/sub").await), ErrorKind::IsADirectory);
        assert_eq!(kind(fs.rename("/dl/sub", "/dl/sub2").await), ErrorKind::Unsupported);
        fs.rename("/dl/photo.jpg", "/dl/photo.jpg").await.unwrap();
    }

    #[tokio::test]
    async fn removing() {
        let fs = fresh().await;
        fs.create_dir_all("/out/one").await.unwrap();
        let mut f = fs.open("/out/one/0.bin", CREATE).await.unwrap();
        f.write_all(b"x").unwrap();
        assert_eq!(kind(fs.remove_dir("/out/one").await), ErrorKind::DirectoryNotEmpty);
        assert_eq!(kind(fs.remove_file("/out/one").await), ErrorKind::IsADirectory);
        assert_eq!(kind(fs.remove_dir("/out/one/0.bin").await), ErrorKind::NotADirectory);
        assert_eq!(kind(fs.remove_file("/out/none").await), ErrorKind::NotFound);
        // removed while open: the file is gone, and the open one says so
        fs.remove_file("/out/one/0.bin").await.unwrap();
        assert!(!fs.exists("/out/one/0.bin"));
        assert_eq!(kind(f.write(b"y")), ErrorKind::NotFound);
        drop(f);
        fs.remove_dir("/out/one").await.unwrap();
        assert_eq!(names(&fs, "/out"), Vec::<String>::new());

        fs.create_dir_all("/out/two/deep").await.unwrap();
        fs.open("/out/two/a", NEW).await.unwrap();
        fs.open("/out/two/deep/b", NEW).await.unwrap();
        fs.remove_dir_all("/out/two").await.unwrap();
        assert!(!fs.exists("/out/two") && fs.exists("/out"));
        fs.remove_dir_all("/out/two").await.unwrap();
        assert_eq!(kind(fs.remove_dir("/").await), ErrorKind::InvalidInput);
        assert!(fs.backend.files.borrow().is_empty() && fs.backend.held.borrow().is_empty());
    }

    #[tokio::test]
    async fn listing_one_directory() {
        let fs = fresh().await;
        fs.create_dir_all("/a/b").await.unwrap();
        fs.create_dir_all("/ab").await.unwrap();
        fs.open("/a/1", NEW).await.unwrap().write_all(b"12345").unwrap();
        fs.open("/a/b/2", NEW).await.unwrap();
        fs.open("/ab/3", NEW).await.unwrap();
        assert_eq!(names(&fs, "/"), ["a", "ab"]);
        assert_eq!(names(&fs, "/a"), ["1", "b"], "its own entries, not those below or beside");
        let e = &fs.read_dir_now("/a").unwrap()[0];
        assert_eq!((e.path.as_str(), e.metadata.len(), e.metadata.is_file()), ("/a/1", 5, true));
        assert_eq!(kind(fs.read_dir_now("/a/1")), ErrorKind::NotADirectory);
        assert_eq!(kind(fs.read_dir_now("/none")), ErrorKind::NotFound);
        assert_eq!(kind(fs.create_dir("/a").await), ErrorKind::AlreadyExists);
        assert_eq!(kind(fs.create_dir("/x/y").await), ErrorKind::NotFound);
        fs.create_dir("/a/c").await.unwrap();
        assert_eq!(names(&fs, "/a"), ["1", "b", "c"]);
    }

    #[tokio::test]
    async fn a_copy_and_what_is_found_at_the_next_start() {
        let mem = Mem::default();
        let (files, dirs) = (mem.files.clone(), mem.dirs.clone());
        let fs = Fs::load(mem, clock).await.unwrap();
        fs.create_dir_all("/dl").await.unwrap();
        fs.open("/dl/a", NEW).await.unwrap().write_all(&vec![7u8; 300_000]).unwrap();
        fs.copy("/dl/a", "/dl/b").await.unwrap();
        assert_eq!(fs.metadata_now("/dl/b").unwrap().len(), 300_000);
        assert_eq!(kind(fs.copy("/dl/a", "/dl/b").await), ErrorKind::AlreadyExists);
        drop(fs);
        // a new start reads what the browser kept
        let again = Fs::load(Mem { files, dirs, ..Default::default() }, clock).await.unwrap();
        assert_eq!(names(&again, "/dl"), ["a", "b"]);
        assert_eq!(again.metadata_now("/dl/b").unwrap().len(), 300_000);
        let mut f = again.open("/dl/b", READ).await.unwrap();
        let mut buf = [0u8; 4];
        f.seek(SeekFrom::End(-4)).unwrap();
        assert_eq!(f.read(&mut buf).unwrap(), 4);
        assert_eq!(buf, [7; 4]);
    }
}
