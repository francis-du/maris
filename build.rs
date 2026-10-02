use sha2::{Digest, Sha256};
#[path = "src/devices/catalog_link.rs"]
mod catalog_link;
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

const AUTOEQ_COMMIT: &str = "7ae0f56d53074872b028649617a22bbb4232feb7";
const EQMAC_COMMIT: &str = "04e5a3a9bd3a65f2b5105cf54a76d2c72a1d00d7";
const MUSICNN_REVISION: &str = "7cff1a4f9899825ddba77130899dfac4c8cfe9d5";
const MUSICNN_SHA256: &str = "cc0b9400fcaed6e9ce7fbcfa97ec91e4fcb5f2ab34ca3a0cd6bef4af74753e1a";
const MUSICNN_BYTES: usize = 3_175_212;
const MUSICNN_URL: &str = "https://huggingface.co/oriyonay/musicnn-pytorch/resolve/7cff1a4f9899825ddba77130899dfac4c8cfe9d5/model.safetensors?download=true";

fn slug(name: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if separator && !result.is_empty() {
                result.push('-');
            }
            result.push(ch.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    result
}

fn extract_eqmac_presets(source: &str) -> Vec<(String, [f64; 10])> {
    let mut presets = Vec::new();
    let mut current: Option<(String, Vec<f64>)> = None;
    let mut in_table = false;
    for line in source.lines() {
        let line = line.trim();
        if line.starts_with("let ADVANCED_EQUALIZER_DEFAULT_PRESETS") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        if current.is_none() && line == "]" {
            break;
        }
        if let Some(rest) = line.strip_prefix('"') {
            let Some(end) = rest.find("\":") else {
                continue;
            };
            let name = rest[..end].to_owned();
            if line.contains("Array(repeating: 0, count: 10)") {
                presets.push((name, [0.0; 10]));
            } else if line.ends_with('[') {
                current = Some((name, Vec::with_capacity(10)));
            }
            continue;
        }
        if let Some((name, gains)) = current.as_mut() {
            if line == "]," || line == "]" {
                assert_eq!(
                    gains.len(),
                    10,
                    "eqMac preset {name} must contain ten gains"
                );
                let values: [f64; 10] = gains.clone().try_into().expect("checked length");
                presets.push((name.clone(), values));
                current = None;
                continue;
            }
            if !line.is_empty() {
                let number = line
                    .trim_end_matches(',')
                    .parse::<f64>()
                    .unwrap_or_else(|_| panic!("invalid eqMac preset gain in {name}: {line}"));
                assert!(number.is_finite(), "non-finite eqMac preset gain");
                gains.push(number);
            }
        }
    }
    assert!(current.is_none(), "unterminated eqMac preset");
    assert_eq!(
        presets.len(),
        22,
        "expected the fixed eqMac source to contain 22 presets"
    );
    let mut ids = BTreeSet::new();
    for (name, gains) in &presets {
        assert!(gains.iter().all(|gain| gain.is_finite()));
        assert!(
            ids.insert(slug(name)),
            "duplicate generated eqMac preset id"
        );
    }
    presets
}

fn generate_eqmac_presets(manifest: &Path) {
    let source_path = manifest.join("third_party/eqmac/AdvancedEqualizerDefaultPresets.swift");
    println!("cargo:rerun-if-changed={}", source_path.display());
    let source = fs::read_to_string(&source_path).expect("read vendored eqMac preset source");
    let presets = extract_eqmac_presets(&source);
    let mut generated = format!(
        "pub const EQMAC_SOURCE_COMMIT: &str = {:?};\n\npub const EQMAC_RAW_PRESETS: &[EqMacRawPreset] = &[\n",
        EQMAC_COMMIT
    );
    for (name, gains) in presets {
        generated.push_str(&format!(
            "    EqMacRawPreset {{ id: {:?}, original_name: {:?}, gains_db: {:?} }},\n",
            format!("eqmac:{}", slug(&name)),
            name,
            gains
        ));
    }
    generated.push_str("];\n");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("eqmac_presets.rs"), generated).expect("write generated eqMac presets");
}

fn verify_musicnn(bytes: &[u8]) {
    assert_eq!(
        bytes.len(),
        MUSICNN_BYTES,
        "pinned MusicNN weight size changed"
    );
    let digest = hex::encode(Sha256::digest(bytes));
    assert_eq!(
        digest, MUSICNN_SHA256,
        "pinned MusicNN weight digest changed"
    );
}

fn generate_bundled_models(manifest: &Path) {
    println!("cargo:rustc-check-cfg=cfg(maris_musicnn_bundled)");
    println!("cargo:rerun-if-env-changed=MARIS_BUNDLE_SMALL_MODELS");
    let local = manifest.join("third_party/musicnn/model.safetensors");
    println!("cargo:rerun-if-changed={}", local.display());
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let generated = out.join("bundled_models.rs");
    if std::env::var("MARIS_BUNDLE_SMALL_MODELS").as_deref() != Ok("1") {
        fs::write(
            generated,
            format!(
                "pub const MUSICNN_BUNDLED: bool = false;\npub const MUSICNN_SOURCE_REVISION: &str = {:?};\npub const MUSICNN_SOURCE_SHA256: &str = {:?};\npub static MUSICNN_MODEL_BYTES: &[u8] = &[];\n",
                MUSICNN_REVISION, MUSICNN_SHA256
            ),
        )
        .expect("write model bundle metadata");
        return;
    }
    let bytes = if local.is_file() {
        fs::read(&local).expect("read locally staged MusicNN weights")
    } else {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(90)))
            .https_only(true)
            .max_redirects(5)
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let mut response = agent
            .get(MUSICNN_URL)
            .header(
                "User-Agent",
                concat!("maris-build/", env!("CARGO_PKG_VERSION")),
            )
            .call()
            .expect("download pinned MusicNN build resource");
        if let Some(length) = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
        {
            assert_eq!(
                length, MUSICNN_BYTES,
                "MusicNN server size differs from pin"
            );
        }
        let mut reader = response
            .body_mut()
            .as_reader()
            .take((MUSICNN_BYTES + 1) as u64);
        let mut bytes = Vec::with_capacity(MUSICNN_BYTES);
        reader
            .read_to_end(&mut bytes)
            .expect("read pinned MusicNN build resource");
        bytes
    };
    verify_musicnn(&bytes);
    println!("cargo:rustc-cfg=maris_musicnn_bundled");
    let target = out.join("musicnn.safetensors");
    fs::write(&target, bytes).expect("write verified MusicNN build resource");
    fs::write(
        generated,
        format!(
            "pub const MUSICNN_BUNDLED: bool = true;\npub const MUSICNN_SOURCE_REVISION: &str = {:?};\npub const MUSICNN_SOURCE_SHA256: &str = {:?};\npub static MUSICNN_MODEL_BYTES: &[u8] = include_bytes!({:?});\n",
            MUSICNN_REVISION,
            MUSICNN_SHA256,
            target.to_string_lossy()
        ),
    )
    .expect("write bundled model module");
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = |byte: u8| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                b'A'..=b'F' => Some(byte - b'A' + 10),
                _ => None,
            };
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(decoded).expect("AutoEq index paths must decode as UTF-8")
}

fn autoeq_catalog_paths(index: &str) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for line in index.lines().map(str::trim) {
        let Some(rest) = line.strip_prefix("- [") else {
            continue;
        };
        let Some(close) = rest.find("](") else {
            continue;
        };
        let after = &rest[close + 2..];
        let Some(path) = catalog_link::destination(after) else {
            continue;
        };
        if path.starts_with("./") {
            paths.insert(path.to_owned());
        }
    }
    let paths: Vec<_> = paths.into_iter().collect();
    assert!(paths.len() > 5_000, "AutoEq catalog unexpectedly small");
    paths
}

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run git {}: {error}", args.join(" ")));
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        println!("cargo:warning=AutoEq git {}: {line}", args[0]);
    }
}

fn validate_autoeq_pack(pack: &[u8], expected_count: usize) {
    const HEADER: &[u8] = b"MARIS_AUTOEQ_V1\0";
    assert!(pack.starts_with(HEADER), "invalid AutoEq pack header");
    let mut cursor = HEADER.len();
    let read_u32 = |bytes: &[u8], cursor: &mut usize| {
        assert!(*cursor + 4 <= bytes.len(), "truncated AutoEq pack");
        let value = u32::from_le_bytes(bytes[*cursor..*cursor + 4].try_into().unwrap());
        *cursor += 4;
        value as usize
    };
    let count = read_u32(pack, &mut cursor);
    assert_eq!(count, expected_count, "AutoEq pack/catalog count mismatch");
    for _ in 0..count {
        let path = read_u32(pack, &mut cursor);
        let text = read_u32(pack, &mut cursor);
        assert!(path > 2 && path < 4_096, "invalid AutoEq profile path size");
        assert!(
            text > 16 && text < 64 * 1024,
            "invalid AutoEq profile text size"
        );
        assert!(
            cursor + path + text <= pack.len(),
            "truncated AutoEq profile record"
        );
        std::str::from_utf8(&pack[cursor..cursor + path]).expect("AutoEq path UTF-8");
        cursor += path;
        std::str::from_utf8(&pack[cursor..cursor + text]).expect("AutoEq profile UTF-8");
        cursor += text;
    }
    assert_eq!(cursor, pack.len(), "trailing AutoEq pack bytes");
    assert!(
        pack.len() < 8 * 1024 * 1024,
        "AutoEq profile pack is unexpectedly large"
    );
}

fn generate_autoeq_profiles(manifest: &Path) {
    println!("cargo:rerun-if-env-changed=MARIS_BUNDLE_AUTOEQ_PROFILES");
    let index_path = manifest.join("third_party/autoeq/results-index.md");
    println!("cargo:rerun-if-changed={}", index_path.display());
    let index = fs::read_to_string(&index_path).expect("read pinned AutoEq catalog");
    let keys = autoeq_catalog_paths(&index);
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let generated = out.join("autoeq_bundle.rs");
    if std::env::var("MARIS_BUNDLE_AUTOEQ_PROFILES").as_deref() != Ok("1") {
        fs::write(
            generated,
            format!(
                "pub const AUTOEQ_PROFILES_BUNDLED: bool = false;\npub const AUTOEQ_PROFILE_PACK_SHA256: &str = \"\";\npub const AUTOEQ_PROFILE_SOURCE_COMMIT: &str = {:?};\npub static AUTOEQ_PROFILE_PACK: &[u8] = &[];\n",
                AUTOEQ_COMMIT
            ),
        )
        .expect("write AutoEq bundle metadata");
        return;
    }

    let staged = manifest.join("third_party/autoeq/profiles.pack");
    let pack = if staged.is_file() {
        println!("cargo:rerun-if-changed={}", staged.display());
        fs::read(&staged).expect("read staged AutoEq profile pack")
    } else {
        let checkout = out.join("autoeq-profile-source");
        if checkout.exists() {
            fs::remove_dir_all(&checkout).expect("remove prior AutoEq build checkout");
        }
        fs::create_dir_all(&checkout).expect("create AutoEq build checkout");
        git(&checkout, &["init", "--quiet"]);
        if cfg!(windows) {
            // Profile names can exceed MAX_PATH below Cargo's nested OUT_DIR.
            // Opt only this temporary checkout into Git for Windows long paths.
            git(&checkout, &["config", "--local", "core.longpaths", "true"]);
        }
        git(
            &checkout,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/jaakkopasanen/AutoEq.git",
            ],
        );
        git(
            &checkout,
            &[
                "fetch",
                "--quiet",
                "--depth=1",
                "--filter=blob:none",
                "origin",
                AUTOEQ_COMMIT,
            ],
        );
        git(&checkout, &["sparse-checkout", "init", "--no-cone"]);
        git(
            &checkout,
            &["sparse-checkout", "set", "--no-cone", "/results/**/*.txt"],
        );
        git(
            &checkout,
            &["checkout", "--quiet", "--detach", "FETCH_HEAD"],
        );
        let revision = Command::new("git")
            .current_dir(&checkout)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("read AutoEq checkout revision");
        assert!(
            revision.status.success(),
            "cannot read AutoEq checkout revision"
        );
        assert_eq!(
            String::from_utf8_lossy(&revision.stdout).trim(),
            AUTOEQ_COMMIT,
            "AutoEq checkout revision differs from pin"
        );

        let mut pack = Vec::with_capacity(4 * 1024 * 1024);
        pack.extend_from_slice(b"MARIS_AUTOEQ_V1\0");
        pack.extend_from_slice(&(keys.len() as u32).to_le_bytes());
        for key in &keys {
            let decoded = percent_decode(key.trim_start_matches("./"));
            assert!(
                !decoded.contains('\0') && !decoded.contains(".."),
                "unsafe AutoEq path"
            );
            let leaf = decoded.rsplit('/').next().expect("AutoEq result leaf");
            let source = checkout
                .join("results")
                .join(&decoded)
                .join(format!("{leaf} ParametricEQ.txt"));
            let text = fs::read(&source).unwrap_or_else(|error| {
                panic!("read pinned AutoEq profile {}: {error}", source.display())
            });
            assert!(
                !text.is_empty() && text.len() < 64 * 1024,
                "invalid AutoEq profile size"
            );
            std::str::from_utf8(&text).expect("AutoEq profile must be UTF-8");
            pack.extend_from_slice(&(key.len() as u32).to_le_bytes());
            pack.extend_from_slice(&(text.len() as u32).to_le_bytes());
            pack.extend_from_slice(key.as_bytes());
            pack.extend_from_slice(&text);
        }
        pack
    };

    validate_autoeq_pack(&pack, keys.len());
    let digest = hex::encode(Sha256::digest(&pack));
    let target = out.join("autoeq-profiles.pack");
    fs::write(&target, pack).expect("write verified AutoEq profile pack");
    fs::write(
        generated,
        format!(
            "pub const AUTOEQ_PROFILES_BUNDLED: bool = true;\npub const AUTOEQ_PROFILE_PACK_SHA256: &str = {:?};\npub const AUTOEQ_PROFILE_SOURCE_COMMIT: &str = {:?};\npub static AUTOEQ_PROFILE_PACK: &[u8] = include_bytes!({:?});\n",
            digest,
            AUTOEQ_COMMIT,
            target.to_string_lossy()
        ),
    )
    .expect("write AutoEq bundle module");
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("manifest directory");
    let manifest = std::path::Path::new(&manifest_dir);
    generate_eqmac_presets(manifest);
    generate_bundled_models(manifest);
    generate_autoeq_profiles(manifest);
    println!("cargo:rerun-if-changed=Maris-Info.plist");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let path = manifest.join("Maris-Info.plist");
        // Embed the same audio permission purpose for direct CLI and bundled application launches.
        println!(
            "cargo:rustc-link-arg-bin=maris=-Wl,-sectcreate,__TEXT,__info_plist,{}",
            path.display()
        );
    }
}
