use super::*;


struct LocalMockHttp {
    addr: std::net::SocketAddr,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    requests: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl LocalMockHttp {
    fn start<F>(handler: F) -> Self
    where
        F: Fn(&str) -> (u16, &'static str, Vec<u8>) + Send + Sync + 'static,
    {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock port");
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));
        let handler = Arc::new(handler);
        let s_clone = shutdown.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let r_clone = requests.clone();

        let handle = thread::spawn(move || {
            while !s_clone.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buf = [0u8; 4096];
                        if let Ok(n) = stream.read(&mut buf) {
                            if n > 0 {
                                let req = String::from_utf8_lossy(&buf[..n]);
                                let path = req.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/").to_string();
                                r_clone.lock().unwrap().push(path.clone());
                                let (status, ct, body) = handler(&path);
                                let status_text = match status {
                                    200 => "OK",
                                    404 => "Not Found",
                                    _ => "Custom",
                                };
                                let resp = format!(
                                    "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    status, status_text, ct, body.len()
                                );
                                let _ = stream.write_all(resp.as_bytes());
                                let _ = stream.write_all(&body);
                                let _ = stream.flush();
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            shutdown,
            handle: Some(handle),
            requests,
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for LocalMockHttp {
    fn drop(&mut self) {
        self.shutdown.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[test]
fn test_favicon_fetcher_non_https_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let res = crate::favicon::fetch_and_cache("http://example.invalid/favicon.ico", tmp.path(), false);
    assert!(res.is_err(), "non-HTTPS URL must be refused");
    let err = res.err().unwrap().to_string();
    assert!(err.contains("non-HTTPS refused"), "err: {}", err);
}

#[test]
fn test_favicon_fetcher_size_cap() {
    let tmp = tempfile::tempdir().unwrap();
    // Return > 100 KB
    let huge = vec![0x89; 105 * 1024];
    let server = LocalMockHttp::start(move |_path| (200, "image/png", huge.clone()));

    let res = crate::favicon::fetch_and_cache(&server.url(), tmp.path(), true).unwrap();
    assert_eq!(res, None, "oversize responses must be rejected");
}

#[test]
fn test_favicon_fetcher_caching_ttl_and_clear() {
    let tmp = tempfile::tempdir().unwrap();
    let png_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D];
    let p_clone = png_bytes.clone();
    let server = LocalMockHttp::start(move |path| {
        if path == "/favicon.ico" {
            (200, "image/png", p_clone.clone())
        } else {
            (404, "text/plain", b"not found".to_vec())
        }
    });

    // 1. Initial fetch -> hits server and caches
    let res1 = crate::favicon::fetch_and_cache(&server.url(), tmp.path(), true).unwrap();
    assert!(res1.is_some());
    let url1 = res1.unwrap();
    assert!(url1.starts_with("data:image/png;base64,"));
    assert_eq!(server.requests.lock().unwrap().len(), 1);

    // 2. Second fetch -> cache hit, no server call
    let res2 = crate::favicon::fetch_and_cache(&server.url(), tmp.path(), true).unwrap();
    assert_eq!(res2, Some(url1.clone()));
    assert_eq!(server.requests.lock().unwrap().len(), 1, "must be served from cache");

    // 3. TTL test: artificially expire the cache entry (31 days ago)
    let host_hash = crate::favicon::host_hash(&server.addr.ip().to_string());
    let cache_file = tmp.path().join(format!("{}.json", host_hash));
    assert!(cache_file.exists());
    let expired_entry = serde_json::json!({
        "ts": crate::favicon::now_unix_secs() - (31 * 24 * 60 * 60),
        "data_url": url1,
    });
    std::fs::write(&cache_file, serde_json::to_vec(&expired_entry).unwrap()).unwrap();

    // Third fetch -> cache expired, re-fetches from server
    let res3 = crate::favicon::fetch_and_cache(&server.url(), tmp.path(), true).unwrap();
    assert!(res3.is_some());
    assert_eq!(server.requests.lock().unwrap().len(), 2, "expired cache must trigger re-fetch");

    // 4. Clear cache test
    crate::favicon::clear_favicon_cache(tmp.path()).unwrap();
    assert!(!cache_file.exists(), "cache file must be deleted");

    // Fourth fetch -> re-fetches after cache clear
    let res4 = crate::favicon::fetch_and_cache(&server.url(), tmp.path(), true).unwrap();
    assert!(res4.is_some());
    assert_eq!(server.requests.lock().unwrap().len(), 3, "cleared cache must trigger re-fetch");
}

#[test]
fn test_favicon_fetcher_html_link_rel_icon() {
    let tmp = tempfile::tempdir().unwrap();
    let png_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x02];
    let p_clone = png_bytes.clone();
    let server = LocalMockHttp::start(move |path| {
        match path {
            "/favicon.ico" => (404, "text/plain", b"not found".to_vec()),
            "/" => (
                200,
                "text/html",
                b"<html><head><link rel=\"icon\" href=\"/assets/icon.png\"></head><body>hello</body></html>".to_vec(),
            ),
            "/assets/icon.png" => (200, "image/png", p_clone.clone()),
            _ => (404, "text/plain", b"not found".to_vec()),
        }
    });

    let res = crate::favicon::fetch_and_cache(&server.url(), tmp.path(), true).unwrap();
    assert!(res.is_some());
    let url = res.unwrap();
    assert!(url.starts_with("data:image/png;base64,"));
}

#[test]
fn test_favicon_fetcher_never_follows_redirects() {
    // The site redirects /favicon.ico and / to a cleartext loopback target that serves a valid PNG.
    // The fetcher must not follow either redirect (target sees no request) and must return no icon.
    let tmp = tempfile::tempdir().unwrap();
    let target = LocalMockHttp::start(|_path| (200, "image/png", vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]));
    let loc: &'static str = Box::leak(format!("text/plain\r\nLocation: http://127.0.0.1:{}/favicon.ico", target.addr.port()).into_boxed_str());
    let site = LocalMockHttp::start(move |_path| (302, loc, Vec::new()));
    let res = crate::favicon::fetch_and_cache(&site.url(), tmp.path(), true).unwrap();
    assert_eq!(res, None, "redirects must not be followed");
    assert!(!site.requests.lock().unwrap().is_empty(), "the site itself was asked");
    assert_eq!(target.requests.lock().unwrap().len(), 0, "redirect target must never be contacted");
}

#[test]
fn test_favicon_icon_href_to_private_ip_refused() {
    // Private addresses refused (production policy for addresses), http allowed only so the page can be served:
    // the page lives on loopback too, so it must not even be fetched; and a page pointing at a private-IP href
    // cannot make the fetcher connect there.
    let tmp = tempfile::tempdir().unwrap();
    let icon = LocalMockHttp::start(|_path| (200, "image/png", vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]));
    let html: &'static str = Box::leak(format!("<html><head><link rel=\"icon\" href=\"http://127.0.0.1:{}/i.png\"></head></html>", icon.addr.port()).into_boxed_str());
    let page = LocalMockHttp::start(move |path| if path == "/" { (200, "text/html", html.as_bytes().to_vec()) } else { (404, "text/plain", Vec::new()) });
    let res = crate::favicon::fetch_and_cache_with(&page.url(), tmp.path(), true, false).unwrap();
    assert_eq!(res, None);
    assert_eq!(page.requests.lock().unwrap().len(), 0, "loopback page must not be contacted");
    assert_eq!(icon.requests.lock().unwrap().len(), 0, "private-IP icon href must not be contacted");
}

#[test]
fn test_favicon_toggle_off_makes_no_network_calls_and_toggle_off_clears_cache() {
    let e = env();
    let cache_dir = tempfile::tempdir().unwrap();
    e.app.set_favicon_cache_dir(cache_dir.path().to_path_buf());

    let server = LocalMockHttp::start(|_path| (200, "image/png", vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]));

    // By default, show_website_icons is false
    assert!(!e.app.get_settings().show_website_icons);

    // favicon_url must return None immediately and make 0 network calls
    let res = e.app.favicon_url(&format!("https://{}", server.addr));
    assert_eq!(res, None);
    assert_eq!(server.requests.lock().unwrap().len(), 0, "no network calls when toggle is off");

    // Enable toggle
    let mut s = e.app.get_settings();
    s.show_website_icons = true;
    e.app.set_settings(s.clone()).unwrap();
    assert!(e.app.get_settings().show_website_icons);

    // Artificially populate a cache file in the cache directory
    let host_hash = crate::favicon::host_hash(&server.addr.ip().to_string());
    let cached_file = cache_dir.path().join(format!("{}.json", host_hash));
    std::fs::write(&cached_file, b"{\"ts\":1234,\"data_url\":\"data:image/png;base64,abc\"}").unwrap();
    assert!(cached_file.exists());

    // Disable toggle -> must clear cache
    s.show_website_icons = false;
    e.app.set_settings(s).unwrap();
    assert!(!cached_file.exists(), "turning toggle off must clear the favicon cache");
}
