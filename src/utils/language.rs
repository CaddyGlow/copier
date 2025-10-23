use std::collections::HashMap;
use std::sync::LazyLock;

use camino::Utf8Path;

static LANGUAGE_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("rs", "rust"),
        ("c", "c"),
        ("h", "c"),
        ("hpp", "cpp"),
        ("hh", "cpp"),
        ("cxx", "cpp"),
        ("cpp", "cpp"),
        ("cc", "cpp"),
        ("py", "python"),
        ("rb", "ruby"),
        ("js", "javascript"),
        ("jsx", "jsx"),
        ("ts", "typescript"),
        ("tsx", "tsx"),
        ("java", "java"),
        ("kt", "kotlin"),
        ("swift", "swift"),
        ("go", "go"),
        ("php", "php"),
        ("sh", "bash"),
        ("bash", "bash"),
        ("zsh", "bash"),
        ("ps1", "powershell"),
        ("psm1", "powershell"),
        ("sql", "sql"),
        ("yaml", "yaml"),
        ("yml", "yaml"),
        ("toml", "toml"),
        ("json", "json"),
        ("md", "markdown"),
        ("html", "html"),
        ("css", "css"),
        ("scss", "scss"),
        ("less", "less"),
        ("xml", "xml"),
        ("ini", "ini"),
        ("conf", "conf"),
        ("cfg", "conf"),
        ("gradle", "groovy"),
        ("lua", "lua"),
        ("pl", "perl"),
        ("txt", "text"),
        ("bat", "bat"),
        ("cmd", "bat"),
        ("dockerfile", "dockerfile"),
        ("makefile", "makefile"),
        ("mk", "makefile"),
        ("cmake", "cmake"),
    ])
});

static LANGUAGE_FILENAMES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("Dockerfile", "dockerfile"),
        ("Makefile", "makefile"),
        ("CMakeLists.txt", "cmake"),
        ("BUILD", "starlark"),
        ("WORKSPACE", "starlark"),
    ])
});

pub fn language_for_path(path: &Utf8Path) -> Option<&'static str> {
    if let Some(name) = path.file_name() {
        if let Some(lang) = LANGUAGE_FILENAMES.get(name) {
            return Some(lang);
        }
        let lower_name = name.to_lowercase();
        if let Some(lang) = LANGUAGE_MAP.get(lower_name.as_str()) {
            return Some(lang);
        }
    }

    let ext = path.file_name().and_then(|_| path.extension()).or_else(|| {
        path.file_name()
            .and_then(|name| name.split('.').next_back())
    });

    if let Some(ext) = ext {
        let ext_lower = ext.to_lowercase();
        if let Some(lang) = LANGUAGE_MAP.get(ext_lower.as_str()) {
            return Some(lang);
        }
    }

    None
}
