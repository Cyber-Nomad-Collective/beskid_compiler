//! A loopback package registry with scripted routes, request log and fault injection.
//!
//! Package archives are produced by the test (see the `zip` dev-dependency); this module only
//! serves bytes so every registry observation stays local and credential-free.
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Routes = BTreeMap<String, (u16, Vec<u8>)>;

pub struct Registry {
    url: String,
    routes: Arc<Mutex<Routes>>,
    requests: Arc<Mutex<Vec<String>>>,
    /// After this many served requests every further request answers 503 (0 disables).
    fail_after: Arc<AtomicUsize>,
    /// A route whose response is held until [`Registry::release`] (interleaving fixture).
    hold: Arc<Mutex<Option<String>>>,
    held: Arc<AtomicBool>,
    released: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Registry {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        listener.set_nonblocking(true).expect("nonblocking listener");
        let url = format!("http://{}", listener.local_addr().expect("local address"));
        let routes = Arc::new(Mutex::new(Routes::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let fail_after = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let hold = Arc::new(Mutex::new(None::<String>));
        let held = Arc::new(AtomicBool::new(false));
        let released = Arc::new(AtomicBool::new(false));
        let (r, q, f, s) = (routes.clone(), requests.clone(), fail_after.clone(), stop.clone());
        let (h, hd, rl) = (hold.clone(), held.clone(), released.clone());
        let thread = std::thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_read_timeout(Some(Duration::from_secs(2))).expect("read timeout");
                        let mut bytes = Vec::new();
                        let mut chunk = [0; 1024];
                        while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                            let count = stream.read(&mut chunk).unwrap_or(0);
                            if count == 0 {
                                break;
                            }
                            bytes.extend_from_slice(&chunk[..count]);
                            assert!(bytes.len() < 16384, "request header too large");
                        }
                        let text = String::from_utf8_lossy(&bytes).into_owned();
                        let path = text.split_whitespace().nth(1).unwrap_or("").to_owned();
                        let served = { let mut log = q.lock().expect("request log"); log.push(path.clone()); log.len() };
                        if h.lock().expect("hold").as_deref() == Some(path.as_str()) {
                            hd.store(true, Ordering::SeqCst);
                            while !rl.load(Ordering::SeqCst) && !s.load(Ordering::SeqCst) {
                                std::thread::sleep(Duration::from_millis(5));
                            }
                        }
                        let limit = f.load(Ordering::SeqCst);
                        let (status, body) = if limit != 0 && served > limit {
                            (503, b"service unavailable".to_vec())
                        } else {
                            r.lock().expect("routes").get(&path).cloned().unwrap_or((404, b"missing".to_vec()))
                        };
                        let _ = write!(stream, "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                        let _ = stream.write_all(&body);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(2)),
                    Err(error) => panic!("loopback accept: {error}"),
                }
            }
        });
        Self { url, routes, requests, fail_after, hold, held, released, stop, thread: Some(thread) }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Publish the version listing of `name` (registry JSON body).
    pub fn versions(&self, name: &str, body: &str) {
        self.route(&format!("/api/packages/{name}/versions"), 200, body.as_bytes().to_vec());
    }

    /// Publish one downloadable package archive holding a library with `Value()` returning `value`.
    pub fn package(&self, name: &str, version: &str, value: i32) {
        self.package_bytes(name, version, archive(name, version, &format!("pub i32 Value() {{ return {value}; }}\n"), &[]));
    }

    /// Publish a package whose library depends on another registry package.
    pub fn package_depending_on(&self, name: &str, version: &str, dependency: (&str, &str), source: &str) {
        self.package_bytes(name, version, archive(name, version, source, &[dependency]));
    }

    pub fn package_bytes(&self, name: &str, version: &str, bytes: Vec<u8>) {
        self.route(&format!("/api/packages/{name}/versions/{version}/download"), 200, bytes);
    }

    pub fn route(&self, path: &str, status: u16, body: Vec<u8>) {
        self.routes.lock().expect("routes").insert(path.to_owned(), (status, body));
    }

    pub fn fail_after(&self, served: usize) {
        self.fail_after.store(served, Ordering::SeqCst);
    }

    /// Hold the response of `path` until [`Registry::release`]; [`Registry::held`] reports the arrival.
    pub fn hold(&self, path: &str) {
        *self.hold.lock().expect("hold") = Some(path.to_owned());
    }

    pub fn held(&self) -> bool {
        self.held.load(Ordering::SeqCst)
    }

    pub fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("request log").clone()
    }

    pub fn count(&self) -> usize {
        self.requests.lock().expect("request log").len()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A zip archive of a library package `name@version` with one module `src/Main.bd`.
pub fn archive(name: &str, version: &str, source: &str, dependencies: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file(format!("{name}.bproj"), options).expect("manifest entry");
    let mut manifest = format!("{name} {{ name = \"{name}\" version = \"{version}\" }}\ntarget \"{name}\" {{ kind = \"Lib\" entry = \"Main.bd\" }}\n");
    for (dependency, dependency_version) in dependencies {
        manifest.push_str(&format!("dependency \"{dependency}\" {{ source = \"registry\" version = \"{dependency_version}\" }}\n"));
    }
    writer.write_all(manifest.as_bytes()).expect("manifest bytes");
    writer.start_file("src/Main.bd", options).expect("source entry");
    writer.write_all(source.as_bytes()).expect("source bytes");
    writer.finish().expect("finish archive").into_inner()
}
