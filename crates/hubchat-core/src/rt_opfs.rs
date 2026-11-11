//! The browser's side of Hubchat Web's files (`webfs::Backend`): the
//! origin's private file system (OPFS), in the core's dedicated worker.
//! Only the browser calls live here; what a path means, what exists and who
//! holds a file is `webfs`. Compiled for wasm32 only, so nothing here has a
//! native test: the browser test (tools/web-e2e.mjs) is its proof.
//!
//! The calls are made by name (`Reflect`), not through `web-sys`: the few
//! that are needed are stable web API, and the core gains no dependency.
use crate::webfs::{Backend, Handle, Listed};
use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use std::io::{self, ErrorKind};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wasm_bindgen_futures::wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// What a browser's refusal means in std's terms.
fn failed(e: JsValue) -> io::Error {
    let name = Reflect::get(&e, &"name".into()).ok().and_then(|n| n.as_string()).unwrap_or_default();
    let message = Reflect::get(&e, &"message".into()).ok().and_then(|n| n.as_string()).unwrap_or_else(|| format!("{e:?}"));
    let kind = match name.as_str() {
        "NotFoundError" => ErrorKind::NotFound,
        "QuotaExceededError" => ErrorKind::StorageFull,
        // a file another handle holds, or a place that takes no changes
        "NoModificationAllowedError" | "InvalidStateError" => ErrorKind::ResourceBusy,
        "InvalidModificationError" => ErrorKind::DirectoryNotEmpty,
        "NotAllowedError" | "SecurityError" => ErrorKind::PermissionDenied,
        "TypeError" => ErrorKind::Unsupported,
        _ => ErrorKind::Other,
    };
    let said = if name == "QuotaExceededError" { "this browser's storage for Hubchat is full".to_string() } else { format!("{name}: {message}") };
    io::Error::new(kind, said)
}

fn get(of: &JsValue, name: &str) -> io::Result<JsValue> {
    Reflect::get(of, &name.into()).map_err(failed)
}

fn function(of: &JsValue, name: &str) -> io::Result<Function> {
    get(of, name)?.dyn_into::<Function>().map_err(|_| io::Error::new(ErrorKind::Unsupported, format!("this browser has no {name}()")))
}

fn call(of: &JsValue, name: &str, args: &[JsValue]) -> io::Result<JsValue> {
    let list = Array::new();
    for a in args {
        list.push(a);
    }
    function(of, name)?.apply(of, &list).map_err(failed)
}

async fn wait(promise: JsValue) -> io::Result<JsValue> {
    JsFuture::from(Promise::resolve(&promise)).await.map_err(failed)
}

fn options(pairs: &[(&str, JsValue)]) -> JsValue {
    let o = Object::new();
    for (name, value) in pairs {
        let _ = Reflect::set(&o, &(*name).into(), value);
    }
    o.into()
}

fn split(key: &str) -> (&str, &str) {
    key.rsplit_once('/').unwrap_or(("", key))
}

/// The files' own directory in the origin's private file system.
pub struct Opfs {
    root: JsValue,
}

impl Opfs {
    /// `directory`: its name there (apart from the database's own).
    pub async fn open(directory: &str) -> io::Result<Opfs> {
        let global: JsValue = js_sys::global().into();
        let storage = get(&get(&global, "navigator")?, "storage")?;
        if storage.is_undefined() {
            return Err(io::Error::new(ErrorKind::Unsupported, "this browser gives a page no storage for files"));
        }
        let top = wait(call(&storage, "getDirectory", &[])?).await?;
        let root = wait(call(&top, "getDirectoryHandle", &[directory.into(), options(&[("create", true.into())])])?).await?;
        Ok(Opfs { root })
    }

    async fn directory(&self, key: &str) -> io::Result<JsValue> {
        let mut dir = self.root.clone();
        for part in key.split('/').filter(|p| !p.is_empty()) {
            dir = wait(call(&dir, "getDirectoryHandle", &[part.into()])?).await?;
        }
        Ok(dir)
    }

    async fn file(&self, key: &str, create: bool) -> io::Result<JsValue> {
        let (parent, name) = split(key);
        let dir = self.directory(parent).await?;
        wait(call(&dir, "getFileHandle", &[name.into(), options(&[("create", create.into())])])?).await
    }

    async fn hold(&self, key: &str, create: bool) -> io::Result<OpfsHandle> {
        let file = self.file(key, create).await?;
        let handle = wait(call(&file, "createSyncAccessHandle", &[])?).await?;
        Ok(OpfsHandle {
            read: function(&handle, "read")?,
            write: function(&handle, "write")?,
            truncate: function(&handle, "truncate")?,
            size: function(&handle, "getSize")?,
            flush: function(&handle, "flush")?,
            close: function(&handle, "close")?,
            handle,
        })
    }
}

/// One held file: the browser's synchronous access handle.
pub struct OpfsHandle {
    handle: JsValue,
    read: Function,
    write: Function,
    truncate: Function,
    size: Function,
    flush: Function,
    close: Function,
}

fn at(pos: u64) -> JsValue {
    options(&[("at", (pos as f64).into())])
}

impl Handle for OpfsHandle {
    fn read_at(&self, buf: &mut [u8], pos: u64) -> io::Result<usize> {
        let into = Uint8Array::new_with_length(buf.len() as u32);
        let n = self.read.call2(&self.handle, &into, &at(pos)).map_err(failed)?.as_f64().unwrap_or(0.0) as usize;
        let n = n.min(buf.len());
        into.subarray(0, n as u32).copy_to(&mut buf[..n]);
        Ok(n)
    }
    fn write_at(&self, buf: &[u8], pos: u64) -> io::Result<usize> {
        let from = Uint8Array::from(buf);
        let n = self.write.call2(&self.handle, &from, &at(pos)).map_err(failed)?.as_f64().unwrap_or(0.0) as usize;
        Ok(n.min(buf.len()))
    }
    fn set_len(&self, len: u64) -> io::Result<()> {
        self.truncate.call1(&self.handle, &(len as f64).into()).map(|_| ()).map_err(failed)
    }
    fn len(&self) -> io::Result<u64> {
        Ok(self.size.call0(&self.handle).map_err(failed)?.as_f64().unwrap_or(0.0) as u64)
    }
    fn flush(&self) -> io::Result<()> {
        self.flush.call0(&self.handle).map(|_| ()).map_err(failed)
    }
    fn close(&self) {
        let _ = self.close.call0(&self.handle);
    }
}

impl Backend for Opfs {
    type Handle = OpfsHandle;

    async fn list(&self) -> io::Result<Vec<Listed>> {
        let mut all = Vec::new();
        // (the directory's key, its handle): parents are listed before their children
        let mut todo = vec![(String::new(), self.root.clone())];
        while let Some((prefix, dir)) = todo.pop() {
            let entries = call(&dir, "entries", &[])?;
            loop {
                let step = wait(call(&entries, "next", &[])?).await?;
                if get(&step, "done")?.is_truthy() {
                    break;
                }
                let pair: Array = get(&step, "value")?.unchecked_into();
                let Some(name) = pair.get(0).as_string() else { continue };
                let handle = pair.get(1);
                let key = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
                if get(&handle, "kind")?.as_string().as_deref() == Some("directory") {
                    all.push(Listed { key: key.clone(), dir: true, len: 0, modified: UNIX_EPOCH });
                    todo.push((key, handle));
                } else {
                    let file = wait(call(&handle, "getFile", &[])?).await?;
                    let len = get(&file, "size")?.as_f64().unwrap_or(0.0) as u64;
                    let ms = get(&file, "lastModified")?.as_f64().unwrap_or(0.0);
                    let modified = UNIX_EPOCH + Duration::from_millis(if ms.is_finite() && ms > 0.0 { ms as u64 } else { 0 });
                    all.push(Listed { key, dir: false, len, modified });
                }
            }
        }
        all.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(all)
    }

    async fn open(&self, key: &str, create: bool) -> io::Result<OpfsHandle> {
        self.hold(key, create).await
    }

    async fn create_dir(&self, key: &str) -> io::Result<()> {
        let (parent, name) = split(key);
        let dir = self.directory(parent).await?;
        wait(call(&dir, "getDirectoryHandle", &[name.into(), options(&[("create", true.into())])])?).await.map(|_| ())
    }

    async fn remove(&self, key: &str, _dir: bool) -> io::Result<()> {
        let (parent, name) = split(key);
        let dir = self.directory(parent).await?;
        wait(call(&dir, "removeEntry", &[name.into()])?).await.map(|_| ())
    }

    async fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        let file = self.file(from, false).await?;
        let (parent, name) = split(to);
        let dir = self.directory(parent).await?;
        // FileSystemFileHandle.move(directory, name): every current browser has it for these files
        let moved = match call(&file, "move", &[dir, name.into()]) {
            Ok(promise) => wait(promise).await.map(|_| ()),
            Err(e) => Err(e),
        };
        match moved {
            Ok(()) => Ok(()),
            // a browser without it: the bytes are copied and the old file removed
            Err(e) if e.kind() == ErrorKind::Unsupported => {
                let (source, dest) = (self.hold(from, false).await?, self.hold(to, true).await?);
                let copied = (|| {
                    dest.set_len(0)?;
                    let mut buf = vec![0u8; 256 * 1024];
                    let mut pos = 0u64;
                    loop {
                        let n = source.read_at(&mut buf, pos)?;
                        if n == 0 {
                            break;
                        }
                        let mut done = 0;
                        while done < n {
                            match dest.write_at(&buf[done..n], pos + done as u64)? {
                                0 => return Err(io::Error::new(ErrorKind::WriteZero, "the browser wrote nothing (its storage may be full)")),
                                w => done += w,
                            }
                        }
                        pos += n as u64;
                    }
                    dest.flush()
                })();
                source.close();
                dest.close();
                match copied {
                    Ok(()) => self.remove(from, false).await,
                    Err(e) => {
                        let _ = self.remove(to, false).await;
                        Err(e)
                    }
                }
            }
            Err(e) => Err(e),
        }
    }
}

/// The wall clock for "modified" (`SystemTime::now()` panics in a browser).
pub fn now() -> SystemTime {
    crate::rt::system_now()
}
