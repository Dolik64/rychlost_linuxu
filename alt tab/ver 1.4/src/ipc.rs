//! One UI owner for the whole lifetime of its event loop, including teardown.
//! Other shortcut processes forward commands without taking the owner's lock.
use std::{
    fs::{File, OpenOptions},
    hash::{Hash, Hasher},
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::{fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt}, net::{UnixListener, UnixStream}}},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const START_TIMEOUT: Duration = Duration::from_secs(5);
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct UiListener(Arc<UnixListener>);

impl Hash for UiListener {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_raw_fd().hash(state);
    }
}

impl UiListener {
    pub fn try_clone(&self) -> io::Result<UnixListener> {
        self.0.try_clone()
    }
}

/// Removes only the socket inode we created, never a replacement at this path.
pub struct OwnedSocket {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl OwnedSocket {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let metadata = std::fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_socket() {
            return Err(io::Error::other("Expected a Unix socket"));
        }
        Ok(Self { path, device: metadata.dev(), inode: metadata.ino() })
    }
}

impl Drop for OwnedSocket {
    fn drop(&mut self) {
        if let Ok(metadata) = std::fs::symlink_metadata(&self.path) {
            if metadata.file_type().is_socket()
                && metadata.dev() == self.device && metadata.ino() == self.inode
            {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

pub struct UiOwner {
    // Fields drop in this order: listener, socket pathname, then lifetime lock.
    listener: UiListener,
    _socket: OwnedSocket,
    _lock: File,
}

impl UiOwner {
    pub fn listener(&self) -> UiListener {
        self.listener.clone()
    }

    fn try_claim(lock_path: &Path, socket_path: &Path) -> io::Result<Option<Self>> {
        let lock = OpenOptions::new().create(true).truncate(false)
            .read(true).write(true).mode(0o600).open(lock_path)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::WouldBlock { Ok(None) } else { Err(error) };
        }
        // Only the lifetime-lock owner may remove a stale socket and bind a new one.
        match std::fs::symlink_metadata(socket_path) {
            Ok(metadata) if metadata.file_type().is_socket() => std::fs::remove_file(socket_path)?,
            Ok(_) => return Err(io::Error::other("UI socket path is occupied by a non-socket")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {},
            Err(error) => return Err(error),
        }
        let listener = UnixListener::bind(socket_path)?;
        let socket = OwnedSocket::new(socket_path.to_owned())?;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        Ok(Some(Self { listener: UiListener(Arc::new(listener)), _socket: socket, _lock: lock }))
    }
}

/// None means a live UI acknowledged and applied the shortcut.
pub fn claim_or_forward(runtime: &Path, reverse: bool) -> io::Result<Option<UiOwner>> {
    let lock = runtime.join("cosmic-ext-app-switcher-ui.lock");
    let socket = runtime.join("cosmic-ext-app-switcher.sock");
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        if let Some(owner) = UiOwner::try_claim(&lock, &socket)? {
            return Ok(Some(owner));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "Previous switcher is still starting or closing"));
        }
        match forward(&socket, reverse, remaining.min(REPLY_TIMEOUT))? {
            Forward::Accepted => return Ok(None),
            Forward::Retry => std::thread::sleep(Duration::from_millis(10).min(remaining)),
        }
    }
}

#[derive(Debug, PartialEq)]
enum Forward { Accepted, Retry }

fn forward(path: &Path, reverse: bool, timeout: Duration) -> io::Result<Forward> {
    let mut stream = match UnixStream::connect(path) {
        Ok(stream) => stream,
        Err(error) if matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused) => return Ok(Forward::Retry),
        Err(error) => return Err(error),
    };
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(if reverse { b"prev" } else { b"next" })?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut reply = [0];
    // An ambiguous delivery is an error, not permission to replay the command or
    // delete another process's socket. Only an explicit busy reply is retried.
    stream.read_exact(&mut reply)?;
    match reply[0] {
        b'1' => Ok(Forward::Accepted),
        b'0' => Ok(Forward::Retry),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid UI acknowledgement")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("alt-tab-ipc-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn socket(&self) -> PathBuf { self.0.join("ui.sock") }
        fn lock(&self) -> PathBuf { self.0.join("ui.lock") }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

    #[test]
    fn owner_excludes_second_surface_until_cleanup_finishes() {
        let fixture = Fixture::new();
        let owner = UiOwner::try_claim(&fixture.lock(), &fixture.socket()).unwrap().unwrap();
        assert!(UiOwner::try_claim(&fixture.lock(), &fixture.socket()).unwrap().is_none());
        assert!(fixture.socket().exists());
        drop(owner);
        assert!(!fixture.socket().exists());
        assert!(UiOwner::try_claim(&fixture.lock(), &fixture.socket()).unwrap().is_some());
    }

    #[test]
    fn stale_cleanup_does_not_unlink_replacement_socket() {
        let fixture = Fixture::new();
        let first = UnixListener::bind(fixture.socket()).unwrap();
        let cleanup = OwnedSocket::new(fixture.socket()).unwrap();
        std::fs::remove_file(fixture.socket()).unwrap();
        let replacement = UnixListener::bind(fixture.socket()).unwrap();
        drop(cleanup);
        assert!(fixture.socket().exists());
        drop((first, replacement));
    }

    #[test]
    fn refuses_to_delete_regular_files() {
        let fixture = Fixture::new();
        std::fs::write(fixture.socket(), b"keep").unwrap();
        assert!(UiOwner::try_claim(&fixture.lock(), &fixture.socket()).is_err());
        assert_eq!(std::fs::read(fixture.socket()).unwrap(), b"keep");
    }

    #[test]
    fn forward_requires_ack_and_preserves_reverse_direction() {
        for (reverse, reply, expected) in [(false, b'1', Forward::Accepted), (true, b'0', Forward::Retry)] {
            let fixture = Fixture::new();
            let listener = UnixListener::bind(fixture.socket()).unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = Vec::new();
                stream.read_to_end(&mut bytes).unwrap();
                assert_eq!(bytes, if reverse { b"prev" } else { b"next" });
                stream.write_all(&[reply]).unwrap();
            });
            assert_eq!(forward(&fixture.socket(), reverse, REPLY_TIMEOUT).unwrap(), expected);
            server.join().unwrap();
        }
    }

    #[test]
    fn disconnected_ack_is_not_a_retry() {
        let fixture = Fixture::new();
        let listener = UnixListener::bind(fixture.socket()).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.read_to_end(&mut Vec::new()).unwrap();
        });
        assert!(forward(&fixture.socket(), false, REPLY_TIMEOUT).is_err());
        server.join().unwrap();
    }
}
