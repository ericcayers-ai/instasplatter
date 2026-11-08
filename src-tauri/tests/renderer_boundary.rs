//! Renderer-boundary guard (RealityLog seam, migration step M7).
//!
//! The seam crosses the **artefact**, not the renderer. InstaSplatter keeps its
//! hand-written WebGL2 renderer (`src/splat/renderer.ts`), and the source's
//! three.js + Spark `viewer/` is deliberately not ported, because the host's CSP
//! (`src-tauri/tauri.conf.json`, `script-src 'self' 'unsafe-eval'`) admits
//! neither a remote module origin nor an inline import map.
//!
//! That is a *runtime* fact with a confusing failure mode: if someone later
//! embeds the source's viewer, nothing complains until the webview silently
//! refuses to load it. These tests make the boundary fail loudly instead —
//! at test time, over the frontend sources, over the built assets, and over the
//! CSP itself.

use std::fs;
use std::path::{Path, PathBuf};

/// Remote origins that serve ES modules. Loading a module from any of these
/// violates `script-src 'self'`.
const REMOTE_SCRIPT_HOSTS: [&str; 7] = [
    "cdn.jsdelivr.net",
    "unpkg.com",
    "cdnjs.cloudflare.com",
    "esm.sh",
    "skypack.dev",
    "jspm.io",
    "cdn.skypack.dev",
];

/// Frontend source extensions the guard reads.
const SOURCE_EXTENSIONS: [&str; 7] = ["ts", "tsx", "js", "jsx", "mts", "mjs", "css"];

/// The one in-app renderer. Its existence is itself part of the boundary: the
/// seam crossing PLY means this file, and no second renderer, draws the cloud.
const SOLE_RENDERER: &str = "src/splat/renderer.ts";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn tauri_conf() -> String {
    fs::read_to_string(repo_root().join("src-tauri/tauri.conf.json"))
        .expect("tauri.conf.json should be readable")
}

/// The `script-src` directive of the app's CSP.
fn script_src_directive(csp: &str) -> String {
    directive(csp, "script-src").expect("the CSP must declare script-src")
}

/// One named directive of the app's CSP, trimmed, or `None` when absent.
fn directive(csp: &str, name: &str) -> Option<String> {
    csp.split(';')
        .map(str::trim)
        .find(|d| d == &name || d.starts_with(&format!("{name} ")))
        .map(str::to_string)
}

fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sources(&path, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if SOURCE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) {
                out.push(path);
            }
        }
    }
}

/// Every line of `text` that loads a module from a remote origin.
///
/// Deliberately narrow: `https:` fetches of *data* (tiles, DEM, imagery,
/// catalogs) are allowed by `connect-src 'self' https:` and are how the
/// Geospatial suite works. Only *script* loads are boundary violations.
fn remote_script_loads(text: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let is_module_load = trimmed.starts_with("import ")
            || trimmed.starts_with("export ")
            || trimmed.contains("from \"http")
            || trimmed.contains("from 'http")
            || trimmed.contains("import(\"http")
            || trimmed.contains("import('http")
            || trimmed.contains("<script")
            || trimmed.contains("src=\"http")
            || trimmed.contains("src='http")
            || trimmed.contains("script.src")
            || trimmed.contains("type=\"importmap\"")
            || trimmed.contains("type='importmap'");
        if !is_module_load {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        let remote = REMOTE_SCRIPT_HOSTS.iter().any(|h| lower.contains(h))
            || lower.contains("https://") && (lower.contains(".js") || lower.contains("importmap"));
        if remote {
            hits.push(format!("{}: {}", index + 1, trimmed));
        }
    }
    hits
}

#[test]
fn the_csp_admits_no_remote_script_origin() {
    let conf = tauri_conf();
    let json: serde_json::Value =
        serde_json::from_str(&conf).expect("tauri.conf.json must be valid JSON");
    let csp = json["app"]["security"]["csp"]
        .as_str()
        .expect("the app must declare a CSP");
    let script_src = script_src_directive(csp);

    assert!(
        script_src.contains("'self'"),
        "script-src must keep 'self': {script_src}"
    );
    for host in REMOTE_SCRIPT_HOSTS {
        assert!(
            !script_src.contains(host),
            "script-src admits the remote module origin {host}: {script_src}"
        );
    }
    assert!(
        !script_src.contains("'unsafe-inline'"),
        "script-src must not permit inline scripts (that is the second half of the \
         import-map problem): {script_src}"
    );
    // The directives the seam actually needs: `connect-src` for the pull (the
    // app's own IPC origin plus the data hosts), and no remote script origin.
    let connect_src = directive(csp, "connect-src").expect("the CSP must declare connect-src");
    assert!(
        connect_src.contains("'self'"),
        "connect-src must keep 'self': {connect_src}"
    );
    assert!(
        connect_src.contains("http://ipc.localhost"),
        "connect-src must keep the Tauri IPC origin or no command can be called: {connect_src}"
    );
}

#[test]
fn the_csp_closes_the_plugin_and_base_uri_holes() {
    let conf = tauri_conf();
    let json: serde_json::Value =
        serde_json::from_str(&conf).expect("tauri.conf.json must be valid JSON");
    let csp = json["app"]["security"]["csp"]
        .as_str()
        .expect("the app must declare a CSP");

    // Without these, an injected <object>/<embed> or a rewritten <base> can load a
    // remote document or reroute every relative URL. None of the three can break
    // script execution, so they are unconditional.
    assert_eq!(
        directive(csp, "object-src").as_deref(),
        Some("object-src 'none'"),
        "the CSP must forbid plugin content: {csp}"
    );
    assert_eq!(
        directive(csp, "base-uri").as_deref(),
        Some("base-uri 'none'"),
        "the CSP must forbid <base> rewriting: {csp}"
    );
    assert_eq!(
        directive(csp, "frame-ancestors").as_deref(),
        Some("frame-ancestors 'none'"),
        "the CSP must forbid being framed: {csp}"
    );
}

#[test]
fn no_frontend_source_loads_a_remote_script() {
    let root = repo_root();
    let mut sources = Vec::new();
    collect_sources(&root.join("src"), &mut sources);
    assert!(
        sources.len() > 20,
        "expected the frontend sources to be present, found {}",
        sources.len()
    );

    // The document itself, since it is where an import map would live.
    let index = root.join("index.html");
    if index.is_file() {
        sources.push(index);
    }

    let mut violations = Vec::new();
    for source in &sources {
        let text = fs::read_to_string(source)
            .unwrap_or_else(|e| panic!("{} should be readable UTF-8: {e}", source.display()));
        for hit in remote_script_loads(&text) {
            violations.push(format!("{}: {hit}", source.display()));
        }
    }
    assert!(
        violations.is_empty(),
        "the frontend loads script modules from a remote origin, which the Tauri CSP \
         (script-src 'self') will refuse at run time:\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_source_viewer_is_not_ported_and_the_host_keeps_one_renderer() {
    let root = repo_root();

    // The single in-app renderer exists...
    assert!(
        root.join(SOLE_RENDERER).is_file(),
        "{SOLE_RENDERER} is the seam's renderer and must exist"
    );
    // ... and the source's viewer was not copied in beside it.
    for not_ported in [
        "viewer/main.js",
        "viewer/index.html",
        "viewer/xr.js",
        "viewer/splat-formats.js",
    ] {
        assert!(
            !root.join(not_ported).is_file(),
            "{not_ported} must not be ported into the host: the source's viewer needs a remote \
             module origin and its own HTTP origin, and the owner's roadmap forbids a second renderer"
        );
    }

    // No frontend source may import the source's renderer stack.
    let mut sources = Vec::new();
    collect_sources(&root.join("src"), &mut sources);
    let mut offenders = Vec::new();
    for source in &sources {
        let text = fs::read_to_string(source).unwrap();
        for needle in ["three", "@sparkjsdev/spark"] {
            if text.contains(&format!("from \"{needle}\""))
                || text.contains(&format!("from '{needle}'"))
                || text.contains(&format!("import(\"{needle}\""))
            {
                offenders.push(format!("{}: imports {needle}", source.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a second renderer stack entered the host:\n{}",
        offenders.join("\n")
    );

    // The decision is written down where a contributor will find it.
    let doc =
        fs::read_to_string(root.join("docs/REALITYLOG-SEAM.md")).expect("the seam doc must exist");
    assert!(
        doc.contains("renderer.ts"),
        "the seam doc must name the sole renderer"
    );
    assert!(
        doc.contains("not ported"),
        "the seam doc must record that the source's viewer is not ported"
    );
}

/// External ES-module references in a *built* asset.
///
/// Narrower than [`remote_script_loads`] on purpose: a minified bundle contains
/// plenty of `https://` string literals (Cesium's error messages, tile URLs,
/// documentation), and none of them are script loads. What is unambiguously a
/// violation is (a) one of the CDN origins that serve ES modules, or (b) an
/// actual `import`/`<script src>` naming an absolute https URL.
fn external_module_references(text: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for host in REMOTE_SCRIPT_HOSTS {
        if text.contains(host) {
            hits.push(format!("references the remote module origin {host}"));
        }
    }
    for pattern in [
        "from\"https://",
        "from'https://",
        "from \"https://",
        "from 'https://",
        "import(\"https://",
        "import('https://",
        "import\"https://",
        "<script src=\"https://",
        "<script src='https://",
        "<script type=\"module\" src=\"https://",
    ] {
        if text.contains(pattern) {
            hits.push(format!("contains the external module load {pattern:?}"));
        }
    }
    hits
}

/// The built-asset half of the guard. Ignored by default because it needs
/// `npm run build` to have produced `dist/`; run it with
/// `cargo test -- --ignored renderer_boundary` after a build.
#[test]
#[ignore = "needs `npm run build` to have produced dist/"]
fn built_assets_carry_no_remote_script_origin() {
    let dist = repo_root().join("dist");
    assert!(
        dist.is_dir(),
        "{} does not exist -- run `npm run build` before this gate",
        dist.display()
    );

    let mut assets = Vec::new();
    collect_sources(&dist, &mut assets);
    for extension in ["html", "map"] {
        glob_extension(&dist, extension, &mut assets);
    }
    assert!(!assets.is_empty(), "dist/ contains no assets to check");

    // The document is checked structurally, because that is where a remote
    // `<script src>` or an inline import map would have to appear.
    let index = dist.join("index.html");
    let html = fs::read_to_string(&index).expect("dist/index.html must exist and be UTF-8");
    assert!(
        !html.contains("type=\"importmap\"") && !html.contains("type='importmap'"),
        "dist/index.html carries an inline import map, which script-src 'self' refuses"
    );
    for (n, chunk) in html.split("<script").skip(1).enumerate() {
        assert!(
            !chunk.contains("src=\"http") && !chunk.contains("src='http"),
            "dist/index.html script #{n} loads from a remote origin: {chunk}"
        );
    }

    let mut violations = Vec::new();
    let mut checked = 0usize;
    for asset in &assets {
        let Ok(text) = fs::read_to_string(asset) else {
            continue;
        };
        checked += 1;
        for hit in external_module_references(&text) {
            violations.push(format!("{}: {hit}", asset.display()));
        }
    }
    assert!(
        violations.is_empty(),
        "built assets reference a remote script origin:\n{}",
        violations.join("\n")
    );

    println!("checked {checked} built assets under {}", dist.display());
}

/// Walk `dir` collecting files with `extension`.
fn glob_extension(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            glob_extension(&path, extension, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            out.push(path);
        }
    }
}

/// Sinks that turn a string into markup or into code. The app ships
/// `maplibre-gl@5`, whose one standing advisory (GHSA-jrc7-96c5-q579) lives in
/// `DOM.sanitize()` — the path behind `Popup#setHTML`. That advisory is
/// unreachable only while the renderer has no HTML sink, which is a fact a later
/// change can silently break. This test makes the boundary loud at test time.
#[test]
fn the_renderer_has_no_html_or_code_injection_sink() {
    const FORBIDDEN: [(&str, &str); 6] = [
        ("dangerouslySetInnerHTML", "React raw-HTML prop"),
        (".innerHTML =", "raw HTML assignment"),
        (".outerHTML =", "raw HTML assignment"),
        ("document.write(", "document.write"),
        ("new Function(", "string-to-code"),
        ("setHTML(", "MapLibre/Cesium HTML popup sink"),
    ];

    let root = repo_root();
    let mut sources = Vec::new();
    collect_sources(&root.join("src"), &mut sources);
    assert!(
        sources.len() > 20,
        "expected the frontend sources to be present, found {}",
        sources.len()
    );

    let mut hits = Vec::new();
    for source in &sources {
        let text = fs::read_to_string(source)
            .unwrap_or_else(|e| panic!("{} should be readable UTF-8: {e}", source.display()));
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            // Prose in a comment may name a sink without being one.
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }
            for (needle, why) in FORBIDDEN {
                if trimmed.contains(needle) {
                    hits.push(format!(
                        "{}:{}  {why}: {trimmed}",
                        source.display(),
                        index + 1
                    ));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "the renderer gained an HTML/code injection sink, which makes the shipped \
         sanitizer advisories reachable:\n{}",
        hits.join("\n")
    );
}
