const NOISE_EXACT: &[&str] = &[
    "pnpm-lock.yaml",
    "package-lock.json",
    "yarn.lock",
    "npm-shrinkwrap.json",
    "bun.lockb",
    "Cargo.lock",
    "Gemfile.lock",
    "poetry.lock",
    "Pipfile.lock",
    "uv.lock",
    "go.sum",
    "composer.lock",
    "packages.lock.json",
    "pubspec.lock",
    "Package.resolved",
    "mix.lock",
    ".DS_Store",
];

const NOISE_EXTENSIONS: &[&str] = &[
    ".min.js",
    ".min.css",
    ".map",
    ".chunk.js",
    ".bundle.js",
];

const NOISE_PREFIXES: &[&str] = &[
    "dist/",
    ".next/",
    "build/",
    "__generated__/",
    ".turbo/",
];

pub fn is_noise_file(path: &str) -> bool {
    let filename = path.rsplit('/').next().unwrap_or(path);

    if NOISE_EXACT.iter().any(|n| filename == *n) {
        return true;
    }

    if NOISE_EXTENSIONS.iter().any(|ext| path.ends_with(ext)) {
        return true;
    }

    if NOISE_PREFIXES.iter().any(|prefix| path.starts_with(prefix)) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_files_are_noise() {
        assert!(is_noise_file("Cargo.lock"));
        assert!(is_noise_file("package-lock.json"));
        assert!(is_noise_file("some/path/yarn.lock"));
    }

    #[test]
    fn minified_files_are_noise() {
        assert!(is_noise_file("app.min.js"));
        assert!(is_noise_file("dist/styles.min.css"));
    }

    #[test]
    fn build_dirs_are_noise() {
        assert!(is_noise_file("dist/bundle.js"));
        assert!(is_noise_file("build/output.js"));
        assert!(is_noise_file("__generated__/types.ts"));
    }

    #[test]
    fn source_files_are_not_noise() {
        assert!(!is_noise_file("src/main.rs"));
        assert!(!is_noise_file("lib/utils.ts"));
    }
}
