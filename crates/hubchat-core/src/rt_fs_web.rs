//! `rt::fs` in a browser: the names of `tokio::fs` the core uses, over
//! Hubchat Web's own files (`webfs` on the origin's private file system,
//! `rt_opfs`). Until `init` has run (the web shell awaits it before it
//! starts the core), and in a browser that gives no such storage, every
//! call answers `ErrorKind::Unsupported`: file work fails cleanly.
//!
//! Paths are the core's own (`/downloads/...`): they name places inside the
//! files' directory, never anything outside it.
use crate::rt_opfs::{Opfs, OpfsHandle};
use crate::webfs;
use std::cell::RefCell;
use std::io::{self, SeekFrom};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};

pub use webfs::Metadata;

thread_local! {
    static FILES: RefCell<Option<Rc<webfs::Fs<Opfs>>>> = const { RefCell::new(None) };
}

/// Open the files' directory and read what it holds. Once, before the core
/// starts; an error says why this browser keeps no files for Hubchat.
pub async fn init(directory: &str) -> io::Result<()> {
    if FILES.with(|f| f.borrow().is_some()) {
        return Ok(());
    }
    let fs = webfs::Fs::load(Opfs::open(directory).await?, crate::rt_opfs::now).await?;
    FILES.with(|f| *f.borrow_mut() = Some(Rc::new(fs)));
    Ok(())
}

fn files() -> io::Result<Rc<webfs::Fs<Opfs>>> {
    FILES.with(|f| f.borrow().clone()).ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "Hubchat Web has no file storage in this browser"))
}

fn text(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}

/// An open file. The core's worker has one thread, so it may cross the
/// `Send` bounds the native code is written with.
pub struct File(webfs::File<OpfsHandle>);
// SAFETY: wasm32-unknown-unknown has one thread; nothing here is ever on another.
unsafe impl Send for File {}
unsafe impl Sync for File {}

impl File {
    pub async fn open(path: impl AsRef<Path>) -> io::Result<File> {
        OpenOptions::new().read(true).open(path).await
    }
    pub async fn create(path: impl AsRef<Path>) -> io::Result<File> {
        OpenOptions::new().write(true).create(true).truncate(true).open(path).await
    }
    pub async fn metadata(&self) -> io::Result<Metadata> {
        self.0.metadata()
    }
    /// The same, without waiting (the file is open: its handle answers).
    pub fn metadata_now(&self) -> io::Result<Metadata> {
        self.0.metadata()
    }
    pub async fn set_len(&self, size: u64) -> io::Result<()> {
        self.0.set_len(size)
    }
    pub async fn sync_all(&self) -> io::Result<()> {
        self.0.try_clone().flush()
    }
    pub async fn sync_data(&self) -> io::Result<()> {
        self.0.try_clone().flush()
    }
    pub async fn try_clone(&self) -> io::Result<File> {
        Ok(File(self.0.try_clone()))
    }
}

impl AsyncRead for File {
    fn poll_read(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let n = self.0.read(buf.initialize_unfilled())?;
        buf.advance(n);
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for File {
    fn poll_write(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Poll::Ready(self.0.write(buf))
    }
    fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.0.flush())
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.0.flush())
    }
}

impl AsyncSeek for File {
    fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
        self.0.seek(position).map(|_| ())
    }
    fn poll_complete(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        Poll::Ready(Ok(self.0.position()))
    }
}

#[derive(Clone, Debug, Default)]
pub struct OpenOptions(webfs::Options);

impl OpenOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn read(&mut self, on: bool) -> &mut Self {
        self.0.read = on;
        self
    }
    pub fn write(&mut self, on: bool) -> &mut Self {
        self.0.write = on;
        self
    }
    pub fn append(&mut self, on: bool) -> &mut Self {
        self.0.append = on;
        self
    }
    pub fn truncate(&mut self, on: bool) -> &mut Self {
        self.0.truncate = on;
        self
    }
    pub fn create(&mut self, on: bool) -> &mut Self {
        self.0.create = on;
        self
    }
    pub fn create_new(&mut self, on: bool) -> &mut Self {
        self.0.create_new = on;
        self
    }
    pub async fn open(&self, path: impl AsRef<Path>) -> io::Result<File> {
        files()?.open(&text(path), self.0).await.map(File)
    }
}

pub async fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    files()?.remove_file(&text(path)).await
}
pub async fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
    files()?.remove_dir(&text(path)).await
}
pub async fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    files()?.remove_dir_all(&text(path)).await
}
pub async fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    files()?.rename(&text(from), &text(to)).await
}
/// A browser has no hard links: a second file with the same bytes.
pub async fn hard_link(original: impl AsRef<Path>, link: impl AsRef<Path>) -> io::Result<()> {
    files()?.copy(&text(original), &text(link)).await
}
pub async fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    files()?.create_dir_all(&text(path)).await
}
pub async fn create_dir(path: impl AsRef<Path>) -> io::Result<()> {
    files()?.create_dir(&text(path)).await
}
pub async fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let mut file = files()?.open(&text(path), webfs::Options { write: true, create: true, truncate: true, ..Default::default() }).await?;
    file.write_all(contents.as_ref())?;
    file.flush()
}
pub async fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let mut file = files()?.open(&text(path), webfs::Options { read: true, ..Default::default() }).await?;
    let mut all = Vec::with_capacity(file.metadata()?.len() as usize);
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        match file.read(&mut buf)? {
            0 => return Ok(all),
            n => all.extend_from_slice(&buf[..n]),
        }
    }
}
pub async fn metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    metadata_now(path)
}
pub async fn symlink_metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    metadata_now(path)
}

// ---- what the core asks without waiting (natively: the same std calls as always)

/// What is at `path` (std's `symlink_metadata`; there are no links here).
pub fn metadata_now(path: impl AsRef<Path>) -> io::Result<Metadata> {
    files()?.metadata_now(&text(path))
}
pub fn exists_now(path: impl AsRef<Path>) -> bool {
    files().is_ok_and(|f| f.exists(&text(path)))
}
/// One entry of a directory.
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub metadata: Metadata,
}
/// The entries directly inside a directory.
pub fn read_dir_now(path: impl AsRef<Path>) -> io::Result<Vec<Entry>> {
    Ok(files()?.read_dir_now(&text(path))?.into_iter().map(|e| Entry { path: PathBuf::from(e.path), name: e.name, metadata: e.metadata }).collect())
}
