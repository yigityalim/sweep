use std::{fs, io, path::Path};

use sweep_core::{CandidateKind, Decision, Evidence, RecoveryContract, RecoveryKind};

use super::{GitEvidencePolicy, Provider, ProviderAssessment, ProviderContext};

const CACHE_TAG_SIGNATURE: &str = "Signature: 8a477f597d28d172789f06886806bc55";

pub struct PythonProvider;

impl Provider for PythonProvider {
    fn assess(&self, path: &Path, _context: &ProviderContext) -> Option<ProviderAssessment> {
        let name = path.file_name()?.to_str()?;
        let parent = path.parent()?;
        let marker = python_project_marker(parent)?;

        match name {
            ".pytest_cache" => Some(tool_cache(
                path,
                CandidateKind::PytestCache,
                "pytest",
                marker,
                "pytest recreates its cache during test runs.",
            )),
            ".mypy_cache" => Some(tool_cache(
                path,
                CandidateKind::MypyCache,
                "mypy",
                marker,
                "mypy recreates incremental type-checking cache data.",
            )),
            ".ruff_cache" => Some(tool_cache(
                path,
                CandidateKind::RuffCache,
                "Ruff",
                marker,
                "Ruff recreates lint and formatting cache data.",
            )),
            ".venv" if path.join("pyvenv.cfg").is_file() => Some(virtual_environment(marker)),
            _ => None,
        }
    }
}

fn tool_cache(
    path: &Path,
    kind: CandidateKind,
    tool: &str,
    marker: &'static str,
    detail: &str,
) -> ProviderAssessment {
    let mut assessment = ProviderAssessment::new(
        kind,
        RecoveryContract {
            kind: RecoveryKind::Rebuild,
            command: None,
            detail: String::from(detail),
        },
        GitEvidencePolicy::Required,
        format!("{marker} + {tool} cache root"),
    );

    match cache_tag_evidence(path) {
        CacheTagEvidence::Proven => assessment.prove(
            "cache_directory_tag",
            format!("{tool} cache contains the standard cache-directory signature."),
        ),
        CacheTagEvidence::Missing => assessment.review(
            "cache_directory_tag",
            format!("{tool} cache signature is missing; tool ownership is not fully proven."),
        ),
        CacheTagEvidence::Unreadable(error) => assessment.review(
            "cache_directory_tag",
            format!("Could not read {tool} cache signature: {error}"),
        ),
        CacheTagEvidence::Invalid => {
            assessment.decision = Decision::Protected;
            assessment.evidence.push(Evidence::refuted(
                "cache_directory_tag",
                format!("{tool} cache signature does not match the standard cache-directory tag."),
            ));
        }
    }

    assessment
}

fn virtual_environment(marker: &'static str) -> ProviderAssessment {
    let mut assessment = ProviderAssessment::new(
        CandidateKind::PythonVirtualEnv,
        RecoveryContract::none(
            "A Python virtual environment may contain manually installed or otherwise undeclared packages.",
        ),
        GitEvidencePolicy::Required,
        format!("{marker} + .venv/pyvenv.cfg"),
    );
    assessment.review(
        "environment_recovery",
        "The environment boundary is recognized, but exact package recovery is not proven.",
    );
    assessment
}

fn python_project_marker(parent: &Path) -> Option<&'static str> {
    const MARKERS: [&str; 12] = [
        "pyproject.toml",
        "uv.lock",
        "poetry.lock",
        "Pipfile",
        "requirements.txt",
        "setup.py",
        "setup.cfg",
        "tox.ini",
        "pytest.ini",
        "mypy.ini",
        ".mypy.ini",
        "ruff.toml",
    ];

    MARKERS
        .into_iter()
        .find(|marker| parent.join(marker).is_file())
        .or_else(|| parent.join(".ruff.toml").is_file().then_some(".ruff.toml"))
}

enum CacheTagEvidence {
    Proven,
    Missing,
    Unreadable(io::Error),
    Invalid,
}

fn cache_tag_evidence(path: &Path) -> CacheTagEvidence {
    match fs::read_to_string(path.join("CACHEDIR.TAG")) {
        Ok(content) if content.contains(CACHE_TAG_SIGNATURE) => CacheTagEvidence::Proven,
        Ok(_) => CacheTagEvidence::Invalid,
        Err(error) if error.kind() == io::ErrorKind::NotFound => CacheTagEvidence::Missing,
        Err(error) => CacheTagEvidence::Unreadable(error),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use sweep_core::Decision;
    use tempfile::tempdir;

    use super::*;

    fn context() -> ProviderContext {
        ProviderContext { home: None }
    }

    #[test]
    fn tagged_ruff_cache_is_safe_at_provider_boundary() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname='demo'\n",
        )
        .unwrap();
        fs::create_dir(root.path().join(".ruff_cache")).unwrap();
        fs::write(
            root.path().join(".ruff_cache/CACHEDIR.TAG"),
            format!("{CACHE_TAG_SIGNATURE}\n"),
        )
        .unwrap();

        let assessment = PythonProvider
            .assess(&root.path().join(".ruff_cache"), &context())
            .unwrap();

        assert_eq!(assessment.kind, CandidateKind::RuffCache);
        assert_eq!(assessment.decision, Decision::Safe);
    }

    #[test]
    fn missing_cache_tag_requires_review() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("mypy.ini"), "[mypy]\n").unwrap();
        fs::create_dir(root.path().join(".mypy_cache")).unwrap();

        let assessment = PythonProvider
            .assess(&root.path().join(".mypy_cache"), &context())
            .unwrap();

        assert_eq!(assessment.kind, CandidateKind::MypyCache);
        assert_eq!(assessment.decision, Decision::Review);
    }

    #[test]
    fn invalid_cache_tag_is_protected() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("pytest.ini"), "[pytest]\n").unwrap();
        fs::create_dir(root.path().join(".pytest_cache")).unwrap();
        fs::write(
            root.path().join(".pytest_cache/CACHEDIR.TAG"),
            "not a cache tag\n",
        )
        .unwrap();

        let assessment = PythonProvider
            .assess(&root.path().join(".pytest_cache"), &context())
            .unwrap();

        assert_eq!(assessment.kind, CandidateKind::PytestCache);
        assert_eq!(assessment.decision, Decision::Protected);
    }

    #[test]
    fn virtual_environment_is_review_only() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname='demo'\n",
        )
        .unwrap();
        fs::create_dir(root.path().join(".venv")).unwrap();
        fs::write(root.path().join(".venv/pyvenv.cfg"), "home = /usr/bin\n").unwrap();

        let assessment = PythonProvider
            .assess(&root.path().join(".venv"), &context())
            .unwrap();

        assert_eq!(assessment.kind, CandidateKind::PythonVirtualEnv);
        assert_eq!(assessment.decision, Decision::Review);
    }
}
