//! Plugin loader for erdfa ZOS .so plugins
//! Loads from ~/git/erdfa-plugins/target/release/*.so

use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct ErdfaPlugin {
    _lib: Library,
    pub name: String,
    pub coords: [u64; 6],
}

pub struct PluginLoader {
    plugins: HashMap<String, ErdfaPlugin>,
}

impl PluginLoader {
    pub fn new() -> Self {
        Self { plugins: HashMap::new() }
    }

    pub fn load(&mut self, path: &Path) -> Result<String, String> {
        unsafe {
            let lib = Library::new(path).map_err(|e| e.to_string())?;
            let init: Symbol<extern "C" fn() -> *const i8> =
                lib.get(b"plugin_init").map_err(|e| e.to_string())?;
            let coords: Symbol<extern "C" fn() -> [u64; 6]> =
                lib.get(b"plugin_a11y_coords").map_err(|e| e.to_string())?;
            let name_ptr = init();
            let name = std::ffi::CStr::from_ptr(name_ptr).to_string_lossy().to_string();
            let c = coords();
            let n = name.clone();
            self.plugins.insert(name.clone(), ErdfaPlugin { _lib: lib, name, coords: c });
            Ok(n)
        }
    }

    pub fn load_dir(&mut self, dir: &Path) -> Vec<String> {
        let mut loaded = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map_or(false, |e| e == "so") {
                    match self.load(&p) {
                        Ok(name) => loaded.push(name),
                        Err(e) => eprintln!("skip {}: {}", p.display(), e),
                    }
                }
            }
        }
        loaded
    }

    pub fn default_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/mdupont".into());
        PathBuf::from(home).join("git/erdfa-plugins/target/release")
    }

    pub fn get(&self, name: &str) -> Option<&ErdfaPlugin> {
        self.plugins.get(name)
    }

    pub fn list(&self) -> Vec<(&str, [u64; 6])> {
        self.plugins.iter().map(|(k, v)| (k.as_str(), v.coords)).collect()
    }
}
